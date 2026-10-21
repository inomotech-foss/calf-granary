//! Store paths in Nix caches.

use core::error::Error;

use bytes::Bytes;
use calf_granary_server_db::queries::content::{entry_exists, insert_entry};
use calf_granary_server_db::queries::nix::{find_nix_path, insert_nix_path, missing_nix_paths};
use deadpool_postgres::{Client, Pool, PoolError};
use futures::Stream;

use super::{Cache, CacheName, Stored};
use crate::content::{Content, WriteError};
use crate::nix::{InvalidStorePath, PathInfo, StorePath, StorePathHash};

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
            let parse = |base_name: &str| StorePath::from_base_name(base_name);
            let info = PathInfo {
                store_path: StorePath::try_from(row.store_path.as_str())?,
                references: row
                    .refs
                    .iter()
                    .map(|reference| parse(reference))
                    .collect::<Result<_, _>>()?,
                deriver: row.deriver.as_deref().map(parse).transpose()?,
                system: row.system,
                ca: row.ca,
                nar_hash: row.hash,
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
        let blob = content.write_chunked(pool, &self.store, body).await?;
        if blob.hash() != info.nar_hash.as_slice() || blob.size != info.nar_size {
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
        let references: Vec<String> = info.references.iter().map(StorePath::base_name).collect();
        insert_nix_path()
            .bind(
                &transaction,
                &self.name.as_ref(),
                &key,
                &info.store_path.to_string(),
                &references.as_slice(),
                &info.deriver.as_ref().map(StorePath::base_name),
                &info.system.as_deref(),
                &info.ca.as_deref(),
            )
            .await?;
        transaction.commit().await?;
        Ok(Stored::New { size: blob.size })
    }
}

/// Errors from looking up a store path.
#[derive(Debug, thiserror::Error)]
pub(crate) enum NixLookupError {
    /// The lookup failed.
    #[error("failed to look up the store path")]
    Database(#[from] tokio_postgres::Error),
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
    /// Storing the NAR failed.
    #[error(transparent)]
    Write(#[from] WriteError),
}
