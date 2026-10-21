//! Writing content.

use core::error::Error;
use core::mem;
use std::io;

use aws_lc_rs::error::Unspecified;
use bytes::Bytes;
use calf_granary_server_db::client::GenericClient;
use calf_granary_server_db::queries::content::{
    insert_blob, insert_blob_chunks, insert_chunk, touch_chunk,
};
use calf_granary_server_db::types::ChunkCompression;
use deadpool_postgres::{Pool, PoolError};
use futures::stream::FuturesOrdered;
use futures::{Stream, StreamExt as _};
use object_store::path::Path;
use object_store::{
    ObjectStore, ObjectStoreExt as _, PutMultipartOptions, PutOptions, PutPayload, WriteMultipart,
};
use tokio::task::{self, JoinError};
use zstd::bulk;

use super::chunker::Chunker;
use super::upload::Upload;
use super::{Content, ZSTD_LEVEL, new_object_key};
use crate::sha256::{Hasher, Sha256};
use crate::store::StoreName;

/// The size of the parts of multipart uploads. Smaller objects are uploaded
/// in one request.
const PART_SIZE: usize = 8 * 1024 * 1024;

/// How many parts of one object are uploaded at the same time.
const PART_CONCURRENCY: usize = 4;

/// How many chunks of one blob are stored at the same time.
const CHUNK_CONCURRENCY: usize = 8;

/// A blob whose chunks are stored but which no entry references yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NewBlob {
    /// The SHA-256 of the plain content.
    hash: Sha256,
    /// The size of the plain content in bytes.
    pub(crate) size: u64,
    /// The chunk IDs in order.
    chunks: Vec<i64>,
}

impl NewBlob {
    /// The SHA-256 of the plain content.
    pub(crate) const fn hash(&self) -> &Sha256 {
        &self.hash
    }
}

impl Content {
    /// Stores content as one uncompressed chunk in `store`.
    ///
    /// A database connection is only taken once the object is stored. If an
    /// identical chunk exists, the new object is deleted again and the
    /// existing chunk is used.
    pub(crate) async fn write<S, E>(
        &self,
        pool: &Pool,
        store: &StoreName,
        body: S,
    ) -> Result<NewBlob, WriteError>
    where
        S: Stream<Item = Result<Bytes, E>> + Unpin + Send,
        E: Error + Send + Sync + 'static,
    {
        let objects = self
            .stores
            .get(store.as_ref())
            .ok_or_else(|| WriteError::UnknownStore(store.clone()))?;
        let key = new_object_key().map_err(WriteError::Random)?;
        let (hash, size) = upload(objects.as_ref(), &key, body).await?;
        let stored_size = i64::try_from(size).map_err(|_overflow| WriteError::TooLarge)?;
        let client = match pool.get().await {
            Ok(client) => client,
            Err(err) => {
                discard(objects.as_ref(), &key).await;
                return Err(WriteError::Connect(err));
            }
        };
        let inserted = insert_chunk()
            .bind(
                &client,
                &hash.as_bytes().as_slice(),
                &stored_size,
                &ChunkCompression::Plain,
                &stored_size,
                &store.as_ref(),
                &key.as_ref(),
            )
            .one()
            .await;
        let chunk = match inserted {
            Ok(chunk) => chunk,
            Err(err) => {
                discard(objects.as_ref(), &key).await;
                return Err(WriteError::Database(err));
            }
        };
        if !chunk.inserted {
            discard(objects.as_ref(), &key).await;
        }
        Ok(NewBlob {
            hash,
            size,
            chunks: vec![chunk.id],
        })
    }

    /// Splits content into chunks and stores the new ones compressed with
    /// zstd in `store`.
    ///
    /// Chunks that exist already, in any store, are reused.
    pub(crate) async fn write_chunked<S, E>(
        &self,
        pool: &Pool,
        store: &StoreName,
        mut body: S,
    ) -> Result<NewBlob, WriteError>
    where
        S: Stream<Item = Result<Bytes, E>> + Unpin + Send,
        E: Error + Send + Sync + 'static,
    {
        let objects = self
            .stores
            .get(store.as_ref())
            .ok_or_else(|| WriteError::UnknownStore(store.clone()))?
            .as_ref();
        let mut hash = Hasher::new();
        let mut size = 0u64;
        let mut chunker = Chunker::default();
        let mut pending = FuturesOrdered::new();
        let mut chunks = Vec::new();
        let mut finished = false;
        while !finished {
            let ready = if let Some(bytes) = body.next().await {
                let bytes = bytes.map_err(|source| WriteError::Body(Box::new(source)))?;
                hash.update(&bytes);
                size = size.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
                chunker.push(&bytes)
            } else {
                finished = true;
                mem::take(&mut chunker).finish()
            };
            for chunk in ready {
                if pending.len() >= CHUNK_CONCURRENCY
                    && let Some(id) = pending.next().await
                {
                    chunks.push(id?);
                }
                pending.push_back(store_chunk(pool, objects, store, chunk));
            }
        }
        while let Some(id) = pending.next().await {
            chunks.push(id?);
        }
        Ok(NewBlob {
            hash: hash.finish(),
            size,
            chunks,
        })
    }

    /// Records a blob and its chunks and returns its ID.
    ///
    /// Run this in the transaction that references the blob.
    pub(crate) async fn commit<C>(&self, client: &C, blob: &NewBlob) -> Result<i64, WriteError>
    where
        C: GenericClient + Sync,
    {
        let size = i64::try_from(blob.size).map_err(|_overflow| WriteError::TooLarge)?;
        let row = insert_blob()
            .bind(client, &blob.hash.as_bytes().as_slice(), &size)
            .one()
            .await?;
        if row.inserted {
            insert_blob_chunks()
                .bind(client, &row.id, &blob.chunks.as_slice())
                .await?;
        }
        Ok(row.id)
    }
}

/// Errors from writing content.
#[derive(Debug, thiserror::Error)]
pub(crate) enum WriteError {
    /// Reading the body failed, usually because the client disconnected.
    #[error("failed to read the content")]
    Body(#[source] Box<dyn Error + Send + Sync>),
    /// Compressing a chunk failed.
    #[error("failed to compress a chunk")]
    Compress(#[source] io::Error),
    /// Getting a database connection failed.
    #[error("failed to get a database connection")]
    Connect(#[source] PoolError),
    /// Recording the content failed.
    #[error("failed to record the content")]
    Database(#[from] tokio_postgres::Error),
    /// No random object key could be generated.
    #[error("failed to generate an object key")]
    Random(#[source] Unspecified),
    /// Object storage failed.
    #[error("failed to store the content")]
    Store(#[source] object_store::Error),
    /// A compression task panicked.
    #[error("the compression task failed")]
    Task(#[source] JoinError),
    /// The content is larger than Postgres can count.
    #[error("the content is too large")]
    TooLarge,
    /// The store isn't configured.
    #[error("the store {0} isn't configured")]
    UnknownStore(StoreName),
}

/// Stores one chunk compressed with zstd unless it exists already, and
/// returns its ID.
async fn store_chunk(
    pool: &Pool,
    objects: &dyn ObjectStore,
    store: &StoreName,
    chunk: Bytes,
) -> Result<i64, WriteError> {
    let hash = Sha256::of(&chunk);
    let existing = touch_chunk()
        .bind(
            &pool.get().await.map_err(WriteError::Connect)?,
            &hash.as_bytes().as_slice(),
        )
        .opt()
        .await?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let size = i64::try_from(chunk.len()).map_err(|_overflow| WriteError::TooLarge)?;
    let compressed = task::spawn_blocking(move || bulk::compress(&chunk, ZSTD_LEVEL))
        .await
        .map_err(WriteError::Task)?
        .map_err(WriteError::Compress)?;
    let stored_size = i64::try_from(compressed.len()).map_err(|_overflow| WriteError::TooLarge)?;
    let key = new_object_key().map_err(WriteError::Random)?;
    objects
        .put(&key, PutPayload::from(compressed))
        .await
        .map_err(WriteError::Store)?;
    let inserted = match pool.get().await {
        Ok(client) => insert_chunk()
            .bind(
                &client,
                &hash.as_bytes().as_slice(),
                &size,
                &ChunkCompression::Zstd,
                &stored_size,
                &store.as_ref(),
                &key.as_ref(),
            )
            .one()
            .await
            .map_err(WriteError::Database),
        Err(err) => Err(WriteError::Connect(err)),
    };
    let row = match inserted {
        Ok(row) => row,
        Err(err) => {
            discard(objects, &key).await;
            return Err(err);
        }
    };
    if !row.inserted {
        discard(objects, &key).await;
    }
    Ok(row.id)
}

/// Uploads a stream to one object and returns its SHA-256 and size.
///
/// Objects that fit in one part are uploaded in one request, larger ones in
/// parts. An unfinished multipart upload is aborted.
pub(super) async fn upload<S, E>(
    objects: &dyn ObjectStore,
    key: &Path,
    mut body: S,
) -> Result<(Sha256, u64), WriteError>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin + Send,
    E: Error + Send + Sync + 'static,
{
    let mut hash = Hasher::new();
    let mut size = 0u64;
    let mut next = async || -> Result<Option<Bytes>, WriteError> {
        let Some(chunk) = body.next().await else {
            return Ok(None);
        };
        let chunk = chunk.map_err(|source| WriteError::Body(Box::new(source)))?;
        hash.update(&chunk);
        size = size.saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
        Ok(Some(chunk))
    };

    let mut parts = Vec::new();
    let mut buffered = 0usize;
    while buffered < PART_SIZE {
        let Some(chunk) = next().await? else {
            let payload: PutPayload = parts.into_iter().collect();
            objects
                .put_opts(key, payload, PutOptions::default())
                .await
                .map_err(WriteError::Store)?;
            drop(next);
            return Ok((hash.finish(), size));
        };
        buffered = buffered.saturating_add(chunk.len());
        parts.push(chunk);
    }

    let upload = objects
        .put_multipart_opts(key, PutMultipartOptions::default())
        .await
        .map_err(WriteError::Store)?;
    let mut upload = Upload::new(WriteMultipart::new_with_chunk_size(upload, PART_SIZE));
    for part in parts {
        upload
            .put(part, PART_CONCURRENCY)
            .await
            .map_err(WriteError::Store)?;
    }
    while let Some(chunk) = next().await? {
        upload
            .put(chunk, PART_CONCURRENCY)
            .await
            .map_err(WriteError::Store)?;
    }
    upload.finish().await.map_err(WriteError::Store)?;
    drop(next);
    Ok((hash.finish(), size))
}

/// Deletes an object that no chunk references, logging failures.
///
/// Objects that remain are swept by garbage collection.
pub(super) async fn discard(objects: &dyn ObjectStore, key: &Path) {
    if let Err(err) = objects.delete(key).await {
        tracing::warn!(
            error = &err as &dyn Error,
            key = key.as_ref(),
            "failed to delete an unused object"
        );
    }
}
