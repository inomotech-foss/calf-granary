//! Deleting objects that weren't used within the retention.

use std::collections::HashMap;

use calf_granary_server_db::queries::access::{forget_accesses_before, last_accesses};
use chrono::{DateTime, TimeDelta, Utc};
use deadpool_postgres::Client;
use futures::{StreamExt as _, stream};
use object_store::ObjectMeta;
use object_store::path::Path;

use super::Cache;

/// How many objects are checked with one query.
const BATCH_SIZE: usize = 1000;

/// What pruning did to one cache.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PruneStats {
    /// Objects that were checked.
    pub(crate) scanned: u64,
    /// Objects that were deleted.
    pub(crate) deleted: u64,
    /// Bytes that the deleted objects took up.
    pub(crate) freed_bytes: u64,
}

impl PruneStats {
    /// Adds a batch of checked objects.
    fn scanned(&mut self, count: usize) {
        self.scanned = self
            .scanned
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }

    /// Adds a deleted object.
    const fn deleted(&mut self, size: u64) {
        self.deleted = self.deleted.saturating_add(1);
        self.freed_bytes = self.freed_bytes.saturating_add(size);
    }
}

impl Cache {
    /// Deletes objects that weren't read or written within the retention.
    ///
    /// An object's last use is the later of its last recorded read and its
    /// last modification. Returns `None` for caches without a retention.
    pub(crate) async fn prune(
        &self,
        client: &Client,
        now: DateTime<Utc>,
    ) -> Result<Option<PruneStats>, PruneError> {
        let Some(retention) = self.retention else {
            return Ok(None);
        };
        let cutoff = TimeDelta::from_std(retention)
            .ok()
            .and_then(|retention| now.checked_sub_signed(retention))
            .ok_or(PruneError::Retention)?;

        let mut stats = PruneStats::default();
        let mut listing = self.store.list(None);
        let mut batch = Vec::with_capacity(BATCH_SIZE);
        while let Some(meta) = listing.next().await {
            batch.push(meta.map_err(PruneError::List)?);
            if batch.len() >= BATCH_SIZE {
                self.prune_batch(client, cutoff, &mut batch, &mut stats)
                    .await?;
            }
        }
        self.prune_batch(client, cutoff, &mut batch, &mut stats)
            .await?;

        let prefix = if self.prefix.as_ref().is_empty() {
            String::new()
        } else {
            format!("{}/", self.prefix)
        };
        forget_accesses_before()
            .bind(client, &&*self.bucket, &prefix, &cutoff.fixed_offset())
            .await
            .map_err(PruneError::Database)?;
        Ok(Some(stats))
    }

    /// Deletes the expired objects of a batch and empties the batch.
    async fn prune_batch(
        &self,
        client: &Client,
        cutoff: DateTime<Utc>,
        batch: &mut Vec<ObjectMeta>,
        stats: &mut PruneStats,
    ) -> Result<(), PruneError> {
        if batch.is_empty() {
            return Ok(());
        }
        stats.scanned(batch.len());
        let keys: Vec<String> = batch
            .iter()
            .map(|meta| self.full_key(&meta.location).as_ref().to_owned())
            .collect();
        let accessed: HashMap<String, DateTime<Utc>> = last_accesses()
            .bind(client, &&*self.bucket, &keys)
            .all()
            .await
            .map_err(PruneError::Database)?
            .into_iter()
            .map(|row| (row.key, row.last_access.to_utc()))
            .collect();

        let mut sizes: HashMap<Path, u64> = HashMap::new();
        for (meta, key) in batch.drain(..).zip(keys) {
            let last_used = accessed.get(&key).map_or(meta.last_modified, |accessed| {
                (*accessed).max(meta.last_modified)
            });
            if last_used < cutoff {
                sizes.insert(meta.location, meta.size);
            }
        }
        let locations: Vec<_> = sizes.keys().cloned().map(Ok).collect();
        let mut deleted = self.store.delete_stream(stream::iter(locations).boxed());
        while let Some(result) = deleted.next().await {
            match result {
                Ok(location) => stats.deleted(sizes.get(&location).copied().unwrap_or_default()),
                Err(object_store::Error::NotFound { path: _, source: _ }) => {}
                Err(err) => return Err(PruneError::Delete(err)),
            }
        }
        Ok(())
    }
}

/// Errors from pruning a cache.
#[derive(Debug, thiserror::Error)]
pub(crate) enum PruneError {
    /// Reading or writing access times failed.
    #[error("failed to access the read records")]
    Database(#[source] tokio_postgres::Error),
    /// Deleting an object failed.
    #[error("failed to delete an object")]
    Delete(#[source] object_store::Error),
    /// Listing the objects failed.
    #[error("failed to list the objects")]
    List(#[source] object_store::Error),
    /// The retention reaches back further than timestamps can represent.
    #[error("the retention is too long")]
    Retention,
}
