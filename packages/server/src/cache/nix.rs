//! Store paths in Nix caches.

use core::error::Error;
use std::io;
use std::sync::{Arc, Mutex, PoisonError};

use async_compression::tokio::bufread::{XzDecoder, ZstdDecoder};
use bytes::Bytes;
use calf_granary_server_db::client::GenericClient;
use calf_granary_server_db::queries::content::{entry_exists, insert_entry};
use calf_granary_server_db::queries::nix::{
    find_nix_path, find_nix_upload, insert_nix_path, insert_nix_upload, missing_nix_paths,
};
use deadpool_postgres::{Client, Pool, PoolError};
use futures::stream::BoxStream;
use futures::{Stream, StreamExt as _, TryStreamExt as _};
use tokio_util::io::{ReaderStream, StreamReader};

use super::{Cache, CacheName, NoStore, ObjectKey, Stored};
use crate::content::{Content, WriteError};
use crate::nix::{InvalidStorePath, PathInfo, StorePath, StorePathHash, UploadedNarInfo};
use crate::sha256::{Hasher, InvalidSha256};

/// A store path found in a cache.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NixHit {
    /// The cache that had the path.
    pub(crate) cache: CacheName,
    /// The path's metadata.
    pub(crate) info: PathInfo,
    /// The blob with the NAR.
    pub(crate) blob_id: i64,
    /// The size of the NAR as zstd frames, if every chunk is one.
    pub(crate) file_size: Option<u64>,
}

impl Cache {
    /// Looks up a store path, trying the fallback cache when it's missing
    /// here.
    pub(crate) async fn find_nix_path(
        &self,
        client: &Client,
        hash: &StorePathHash,
    ) -> Result<Option<NixHit>, NixLookupError> {
        for cache in [Some(self), self.fallback.as_deref()].into_iter().flatten() {
            let row = find_nix_path()
                .bind(client, &cache.name.as_ref(), &hash.as_ref())
                .opt()
                .await?;
            let Some(row) = row else {
                continue;
            };
            let info = PathInfo {
                store_path: StorePath::new(*hash, &row.name)?,
                references: row
                    .refs
                    .into_iter()
                    .map(StorePath::from_base_name)
                    .collect::<Result<_, _>>()?,
                deriver: row.deriver.map(StorePath::from_base_name).transpose()?,
                system: row.system,
                ca: row.ca,
                nar_hash: row.hash.as_slice().try_into()?,
                nar_size: u64::try_from(row.size).unwrap_or_default(),
            };
            return Ok(Some(NixHit {
                cache: cache.name.clone(),
                info,
                blob_id: row.blob_id,
                file_size: row.file_size.and_then(|size| u64::try_from(size).ok()),
            }));
        }
        Ok(None)
    }

    /// Returns the hashes that neither this cache nor its fallback has.
    pub(crate) async fn missing_nix_paths(
        &self,
        client: &Client,
        hashes: &[StorePathHash],
    ) -> Result<Vec<String>, tokio_postgres::Error> {
        let hashes: Vec<&str> = hashes.iter().map(AsRef::as_ref).collect();
        let mut caches = vec![self.name.as_ref()];
        if let Some(fallback) = &self.fallback {
            caches.push(fallback.name.as_ref());
        }
        missing_nix_paths()
            .bind(client, &hashes.as_slice(), &caches.as_slice())
            .all()
            .await
    }

    /// Stores a store path from its uncompressed NAR unless this cache has
    /// it already.
    ///
    /// The NAR must match the hash and size in `info`.
    pub(crate) async fn put_nix_path<S, E>(
        &self,
        content: &Content,
        pool: &Pool,
        info: &PathInfo,
        body: S,
    ) -> Result<Stored, NixPutError>
    where
        S: Stream<Item = Result<Bytes, E>> + Unpin + Send,
        E: Error + Send + Sync + 'static,
    {
        let key = info.store_path.hash().as_ref();
        let exists = entry_exists()
            .bind(&pool.get().await?, &self.name.as_ref(), &key)
            .one()
            .await?;
        if exists {
            return Ok(Stored::Existing);
        }
        let blob = content.write_chunked(pool, self.store()?, body).await?;
        if blob.hash() != &info.nar_hash || blob.size != info.nar_size {
            return Err(NixPutError::Mismatch);
        }
        let mut client = pool.get().await?;
        let transaction = client.transaction().await?;
        let blob_id = content.commit(&transaction, &blob).await?;
        let created = insert_entry()
            .bind(&transaction, &self.name.as_ref(), &key, &blob_id)
            .opt()
            .await?;
        if created.is_none() {
            return Ok(Stored::Existing);
        }
        insert_path(&transaction, &self.name, info).await?;
        transaction.commit().await?;
        Ok(Stored::New { size: blob.size })
    }
}

/// How a client compressed a NAR file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NarCompression {
    /// Not at all.
    None,
    /// With xz.
    Xz,
    /// With zstd.
    Zstd,
}

impl NarCompression {
    /// Detects the compression from the file name, such as `<hash>.nar.xz`.
    pub(crate) fn of_file(name: &str) -> Option<Self> {
        match name.rsplit_once(".nar")?.1 {
            "" => Some(Self::None),
            ".xz" => Some(Self::Xz),
            ".zst" => Some(Self::Zstd),
            _ => None,
        }
    }
}

impl Cache {
    /// Stores a NAR file that `nix copy` uploads before its narinfo.
    ///
    /// The file is decompressed, so its chunks deduplicate with every other
    /// NAR. Its hash and size are kept to check the narinfo against.
    pub(crate) async fn put_nar_file<S, E>(
        &self,
        content: &Content,
        pool: &Pool,
        key: &ObjectKey,
        compression: NarCompression,
        body: S,
    ) -> Result<Stored, NixPutError>
    where
        S: Stream<Item = Result<Bytes, E>> + Unpin + Send + 'static,
        E: Error + Send + Sync + 'static,
    {
        let exists = entry_exists()
            .bind(&pool.get().await?, &self.name.as_ref(), &key.as_ref())
            .one()
            .await?;
        if exists {
            return Ok(Stored::Existing);
        }
        let file = Arc::new(Mutex::new((Hasher::new(), 0u64)));
        let counted = Arc::clone(&file);
        let raw = body
            .inspect_ok(move |bytes| {
                let mut state = counted.lock().unwrap_or_else(PoisonError::into_inner);
                state.0.update(bytes);
                state.1 = state
                    .1
                    .saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
            })
            .map_err(io::Error::other);
        let reader = StreamReader::new(raw);
        let plain: BoxStream<'static, io::Result<Bytes>> = match compression {
            NarCompression::None => ReaderStream::new(reader).boxed(),
            NarCompression::Xz => {
                let mut decoder = XzDecoder::new(reader);
                decoder.multiple_members(true);
                ReaderStream::new(decoder).boxed()
            }
            NarCompression::Zstd => {
                let mut decoder = ZstdDecoder::new(reader);
                decoder.multiple_members(true);
                ReaderStream::new(decoder).boxed()
            }
        };
        let blob = content.write_chunked(pool, self.store()?, plain).await?;
        let (file_hash, file_size) = {
            let file = file.lock().unwrap_or_else(PoisonError::into_inner);
            (file.0.clone().finish(), file.1)
        };
        let file_size = i64::try_from(file_size).map_err(|_overflow| NixPutError::TooLarge)?;

        let mut client = pool.get().await?;
        let transaction = client.transaction().await?;
        let blob_id = content.commit(&transaction, &blob).await?;
        let created = insert_entry()
            .bind(&transaction, &self.name.as_ref(), &key.as_ref(), &blob_id)
            .opt()
            .await?;
        if created.is_none() {
            return Ok(Stored::Existing);
        }
        insert_nix_upload()
            .bind(
                &transaction,
                &self.name.as_ref(),
                &key.as_ref(),
                &file_hash.as_bytes().as_slice(),
                &file_size,
            )
            .await?;
        transaction.commit().await?;
        Ok(Stored::New { size: blob.size })
    }

    /// Records the store path of a narinfo that `nix copy` uploads after its
    /// NAR file.
    ///
    /// The NAR file must match the narinfo's hashes and sizes.
    pub(crate) async fn put_narinfo(
        &self,
        pool: &Pool,
        info: &UploadedNarInfo,
    ) -> Result<Stored, NixPutError> {
        let mut client = pool.get().await?;
        let transaction = client.transaction().await?;
        let upload = find_nix_upload()
            .bind(&transaction, &self.name.as_ref(), &info.url.as_str())
            .opt()
            .await?
            .ok_or(NixPutError::MissingNar)?;
        let path = &info.path;
        let matches = upload.hash.as_slice() == path.nar_hash.as_bytes()
            && u64::try_from(upload.size).ok() == Some(path.nar_size)
            && info
                .file_hash
                .as_ref()
                .is_none_or(|hash| hash.as_bytes() == upload.file_hash.as_slice())
            && info
                .file_size
                .is_none_or(|size| u64::try_from(upload.file_size).ok() == Some(size));
        if !matches {
            return Err(NixPutError::Mismatch);
        }
        let key = path.store_path.hash().as_ref();
        let created = insert_entry()
            .bind(&transaction, &self.name.as_ref(), &key, &upload.blob_id)
            .opt()
            .await?;
        if created.is_none() {
            return Ok(Stored::Existing);
        }
        insert_path(&transaction, &self.name, path).await?;
        transaction.commit().await?;
        Ok(Stored::New {
            size: path.nar_size,
        })
    }
}

/// Errors from looking up a store path.
#[derive(Debug, thiserror::Error)]
pub(crate) enum NixLookupError {
    /// The lookup failed.
    #[error("failed to look up the store path")]
    Database(#[from] tokio_postgres::Error),
    /// A recorded NAR hash isn't a SHA-256 digest.
    #[error("the database returned an invalid NAR hash")]
    Digest(#[from] InvalidSha256),
    /// A recorded store path is invalid.
    #[error(transparent)]
    StorePath(#[from] InvalidStorePath),
}

/// Errors from storing a store path.
#[derive(Debug, thiserror::Error)]
pub(crate) enum NixPutError {
    /// Getting a database connection failed.
    #[error("failed to get a database connection")]
    Connect(#[from] PoolError),
    /// Recording the store path failed.
    #[error("failed to record the store path")]
    Database(#[from] tokio_postgres::Error),
    /// The NAR doesn't match the claimed hash or size.
    #[error("the NAR doesn't match its hash and size")]
    Mismatch,
    /// The cache keeps no content.
    #[error(transparent)]
    NoStore(#[from] NoStore),
    /// A narinfo names a NAR file that wasn't uploaded.
    #[error("the narinfo names a NAR file that wasn't uploaded")]
    MissingNar,
    /// The NAR file is larger than Postgres can count.
    #[error("the NAR file is too large")]
    TooLarge,
    /// Storing the NAR failed.
    #[error(transparent)]
    Write(#[from] WriteError),
}

/// Records the metadata of a store path whose entry exists.
async fn insert_path<C>(
    client: &C,
    cache: &CacheName,
    path: &PathInfo,
) -> Result<(), tokio_postgres::Error>
where
    C: GenericClient + Sync,
{
    let references: Vec<&str> = path.references.iter().map(StorePath::base_name).collect();
    insert_nix_path()
        .bind(
            client,
            &cache.as_ref(),
            &path.store_path.hash().as_ref(),
            &path.store_path.name(),
            &references.as_slice(),
            &path.deriver.as_ref().map(StorePath::base_name),
            &path.system.as_deref(),
            &path.ca.as_deref(),
        )
        .await
        .map(drop)
}
