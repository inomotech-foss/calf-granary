//! Reading content.

use std::io;
use std::sync::Arc;

use bytes::Bytes;
use calf_granary_server_db::client::GenericClient;
use calf_granary_server_db::queries::content::blob_chunks;
use futures::stream::{self, BoxStream};
use futures::{StreamExt as _, TryStreamExt as _};
use object_store::path::Path;
use object_store::{ObjectStore, ObjectStoreExt as _};
use tokio::task::{self, JoinError};
use zstd::bulk;

use super::{Content, UNCOMPRESSED, ZSTD};

/// A chunk to read.
struct Part {
    /// The store that holds the object.
    store: Arc<dyn ObjectStore>,
    /// The object.
    key: Path,
    /// Whether the object is a zstd frame rather than plain bytes.
    compressed: bool,
    /// The plain size in bytes.
    size: usize,
}

impl Part {
    /// Streams the plain bytes of the chunk.
    ///
    /// Plain chunks are streamed as they are. Compressed chunks are small, so
    /// they are fetched whole and decompressed.
    async fn open(self) -> Result<BoxStream<'static, Result<Bytes, ChunkError>>, ChunkError> {
        let object = self.store.get(&self.key).await.map_err(ChunkError::Store)?;
        if !self.compressed {
            return Ok(object.into_stream().map_err(ChunkError::Store).boxed());
        }
        let frame = object.bytes().await.map_err(ChunkError::Store)?;
        let plain = task::spawn_blocking(move || bulk::decompress(&frame, self.size))
            .await
            .map_err(ChunkError::Task)?
            .map_err(ChunkError::Decompress)?;
        Ok(stream::once(async { Ok(Bytes::from(plain)) }).boxed())
    }
}

impl Content {
    /// Streams the plain content of a blob.
    ///
    /// The chunk list is read up front, so the stream doesn't hold a
    /// database connection.
    pub(crate) async fn read<C>(
        &self,
        client: &C,
        blob_id: i64,
    ) -> Result<BoxStream<'static, Result<Bytes, ChunkError>>, ReadError>
    where
        C: GenericClient + Sync,
    {
        let mut parts = Vec::new();
        for chunk in blob_chunks().bind(client, &blob_id).all().await? {
            let compressed = match chunk.compression.as_str() {
                UNCOMPRESSED => false,
                ZSTD => true,
                _ => return Err(ReadError::Compression(chunk.compression)),
            };
            let store = self
                .stores
                .get(&chunk.store)
                .ok_or_else(|| ReadError::UnknownStore(chunk.store.clone()))?;
            parts.push(Part {
                store: Arc::clone(store),
                key: Path::from(chunk.object_key),
                compressed,
                size: usize::try_from(chunk.size).map_err(|_overflow| ReadError::TooLarge)?,
            });
        }
        Ok(stream::iter(parts).then(Part::open).try_flatten().boxed())
    }
}

/// Errors from reading a blob's chunk list.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ReadError {
    /// A chunk uses an unknown compression.
    #[error("unknown chunk compression {0:?}")]
    Compression(String),
    /// Looking up the chunks failed.
    #[error("failed to look up the chunks")]
    Database(#[from] tokio_postgres::Error),
    /// A chunk is larger than memory can hold.
    #[error("a chunk is too large")]
    TooLarge,
    /// A chunk is in a store that isn't configured.
    #[error("the store {0} isn't configured")]
    UnknownStore(String),
}

/// Errors from streaming a chunk.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ChunkError {
    /// The chunk isn't a valid zstd frame of the recorded size.
    #[error("failed to decompress a chunk")]
    Decompress(#[source] io::Error),
    /// Object storage failed.
    #[error("failed to read a chunk")]
    Store(#[source] object_store::Error),
    /// A decompression task panicked.
    #[error("the decompression task failed")]
    Task(#[source] JoinError),
}
