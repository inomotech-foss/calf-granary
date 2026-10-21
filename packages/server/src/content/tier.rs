//! Moving chunks to the store of their longest-lived cache.
//!
//! A chunk stays in the store it was written to, even when a cache with a
//! longer retention starts using it. Moving it keeps long-lived content out
//! of stores that churn, so their compaction doesn't copy it again and again.

use calf_granary_server_db::queries::content::{
    MisplacedChunks, max_chunk_id, misplaced_chunks, move_chunk,
};
use deadpool_postgres::{Pool, PoolError};
use futures::{StreamExt as _, TryStreamExt as _, stream};
use object_store::ObjectStoreExt as _;
use object_store::path::Path;

use super::write::{WriteError, discard, upload};
use super::{Content, new_object_key};

/// How many chunk IDs one placement query covers.
const ID_SPAN: i64 = 10_000;

/// How many chunks are copied at the same time.
const COPY_CONCURRENCY: usize = 8;

/// Where the chunks of a cache belong.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Placement {
    /// The cache.
    pub(crate) cache: String,
    /// The store that new chunks of the cache go to.
    pub(crate) store: String,
    /// The cache's retention in seconds, infinite without one.
    pub(crate) retention: f64,
}

/// What tiering moved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TierStats {
    /// Chunks moved to another store.
    pub(crate) moved_chunks: u64,
    /// Stored bytes of the moved chunks.
    pub(crate) moved_bytes: u64,
}

impl Content {
    /// Moves every chunk to the store of the longest-lived cache that uses
    /// it.
    ///
    /// The old objects stay until collection sweeps them, so reads that
    /// started before a move still find them.
    pub(crate) async fn tier(
        &self,
        pool: &Pool,
        placements: &[Placement],
    ) -> Result<TierStats, TierError> {
        let caches: Vec<&str> = placements
            .iter()
            .map(|placement| placement.cache.as_str())
            .collect();
        let stores: Vec<&str> = placements
            .iter()
            .map(|placement| placement.store.as_str())
            .collect();
        let retentions: Vec<f64> = placements
            .iter()
            .map(|placement| placement.retention)
            .collect();
        let last = max_chunk_id()
            .bind(&pool.get().await?)
            .one()
            .await?
            .unwrap_or(0);
        let mut stats = TierStats::default();
        let mut after = 0i64;
        while after < last {
            let until = after.saturating_add(ID_SPAN);
            let misplaced = misplaced_chunks()
                .bind(
                    &pool.get().await?,
                    &caches.as_slice(),
                    &stores.as_slice(),
                    &retentions.as_slice(),
                    &after,
                    &until,
                )
                .all()
                .await?;
            after = until;
            let moved: Vec<u64> = stream::iter(misplaced)
                .map(|chunk| self.move_chunk(pool, chunk))
                .buffer_unordered(COPY_CONCURRENCY)
                .try_collect()
                .await?;
            for bytes in moved.into_iter().filter(|bytes| *bytes > 0) {
                stats.moved_chunks = stats.moved_chunks.saturating_add(1);
                stats.moved_bytes = stats.moved_bytes.saturating_add(bytes);
            }
        }
        Ok(stats)
    }

    /// Copies a chunk to its target store and points its row at the copy.
    ///
    /// Returns the stored bytes moved, or 0 if the chunk changed meanwhile.
    async fn move_chunk(&self, pool: &Pool, chunk: MisplacedChunks) -> Result<u64, TierError> {
        let source = self
            .stores
            .get(&chunk.store)
            .ok_or_else(|| TierError::UnknownStore(chunk.store.clone()))?;
        let target = self
            .stores
            .get(&chunk.target)
            .ok_or_else(|| TierError::UnknownStore(chunk.target.clone()))?;
        let old_key = Path::from(chunk.object_key.as_str());
        let object = match source.get(&old_key).await {
            Ok(object) => object,
            Err(object_store::Error::NotFound { path: _, source: _ }) => return Ok(0),
            Err(err) => return Err(TierError::Read(err)),
        };
        let new_key = new_object_key().map_err(|_random| TierError::Random)?;
        let (_, size) = upload(target.as_ref(), &new_key, object.into_stream()).await?;
        if i64::try_from(size).ok() != Some(chunk.stored_size) {
            discard(target.as_ref(), &new_key).await;
            return Err(TierError::Size(chunk.object_key));
        }
        let updated = move_chunk()
            .bind(
                &pool.get().await?,
                &chunk.target.as_str(),
                &new_key.as_ref(),
                &chunk.id,
                &chunk.store.as_str(),
                &chunk.object_key.as_str(),
            )
            .await;
        match updated {
            Ok(1) => Ok(size),
            Ok(_) => {
                discard(target.as_ref(), &new_key).await;
                Ok(0)
            }
            Err(err) => {
                discard(target.as_ref(), &new_key).await;
                Err(TierError::Database(err))
            }
        }
    }
}

/// Errors from tiering.
#[derive(Debug, thiserror::Error)]
pub(crate) enum TierError {
    /// Getting a database connection failed.
    #[error("failed to get a database connection")]
    Connect(#[from] PoolError),
    /// Finding or updating chunks failed.
    #[error("failed to update the chunk records")]
    Database(#[from] tokio_postgres::Error),
    /// No random object key could be generated.
    #[error("failed to generate an object key")]
    Random,
    /// Reading a chunk failed.
    #[error("failed to read a chunk")]
    Read(#[source] object_store::Error),
    /// A copy has another size than the chunk.
    #[error("the copy of {0} has the wrong size")]
    Size(String),
    /// A chunk is in a store that isn't configured.
    #[error("the store {0} isn't configured")]
    UnknownStore(String),
    /// Writing the copy failed.
    #[error(transparent)]
    Write(#[from] WriteError),
}
