//! Deleting content that no entry references any more.

use core::error::Error;
use core::future;
use std::collections::HashSet;

use calf_granary_server_db::queries::content::{
    delete_chunks, delete_orphan_blobs, deleting_chunks, forget_retired_objects, known_objects,
    mark_orphan_chunks, retired_objects,
};
use chrono::{DateTime, TimeDelta, Utc};
use deadpool_postgres::Client;
use futures::{StreamExt as _, TryStreamExt as _, stream};
use object_store::path::Path;
use object_store::{ObjectMeta, ObjectStore, ObjectStoreExt as _};

use super::{CHUNK_PREFIX, Content};
use crate::store::StoreName;

/// How many chunks or objects are handled per query.
const BATCH_SIZE: usize = 1000;

/// How many objects are deleted at the same time.
const DELETE_CONCURRENCY: usize = 16;

/// What collection deleted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CollectStats {
    /// Blobs without entries.
    pub(crate) deleted_blobs: u64,
    /// Chunks without blobs.
    pub(crate) deleted_chunks: u64,
    /// Stored bytes of the deleted chunks.
    pub(crate) freed_bytes: u64,
    /// Objects without chunks, left behind by failed uploads.
    pub(crate) swept_objects: u64,
    /// Old objects of chunks that tiering moved.
    pub(crate) retired_objects: u64,
}

impl Content {
    /// Deletes blobs, chunks and objects that nothing references and that
    /// are older than the grace period.
    pub(crate) async fn collect(
        &self,
        client: &Client,
        now: DateTime<Utc>,
    ) -> Result<CollectStats, CollectError> {
        let grace_seconds = self.grace.as_secs_f64();
        let mut stats = CollectStats {
            deleted_blobs: delete_orphan_blobs().bind(client, &grace_seconds).await?,
            ..CollectStats::default()
        };
        mark_orphan_chunks().bind(client, &grace_seconds).await?;

        let mut after = 0i64;
        let limit = i64::try_from(BATCH_SIZE).unwrap_or(i64::MAX);
        loop {
            let batch = deleting_chunks().bind(client, &after, &limit).all().await?;
            let Some(last) = batch.last() else {
                break;
            };
            after = last.id;
            let deleted: Vec<(i64, i64)> = stream::iter(batch)
                .map(async |chunk| {
                    let store = self.stores.get(&chunk.store)?;
                    match store.delete(&Path::from(chunk.object_key.as_str())).await {
                        Ok(()) | Err(object_store::Error::NotFound { path: _, source: _ }) => {
                            Some((chunk.id, chunk.stored_size))
                        }
                        Err(err) => {
                            tracing::warn!(
                                error = &err as &dyn Error,
                                key = chunk.object_key,
                                "failed to delete a chunk"
                            );
                            None
                        }
                    }
                })
                .buffer_unordered(DELETE_CONCURRENCY)
                .filter_map(future::ready)
                .collect()
                .await;
            let ids: Vec<i64> = deleted.iter().map(|(id, _)| *id).collect();
            stats.deleted_chunks = stats
                .deleted_chunks
                .saturating_add(delete_chunks().bind(client, &ids.as_slice()).await?);
            for (_, size) in deleted {
                stats.freed_bytes = stats
                    .freed_bytes
                    .saturating_add(u64::try_from(size).unwrap_or_default());
            }
        }

        stats.retired_objects = self.delete_retired(client, grace_seconds).await?;

        let cutoff = TimeDelta::from_std(self.grace)
            .ok()
            .and_then(|grace| now.checked_sub_signed(grace))
            .ok_or(CollectError::Grace)?;
        for (name, store) in self.stores.iter() {
            let swept = sweep(client, name, store.as_ref(), cutoff).await?;
            stats.swept_objects = stats.swept_objects.saturating_add(swept);
        }
        Ok(stats)
    }

    /// Deletes the objects that tiering retired before the grace period and
    /// returns how many.
    async fn delete_retired(
        &self,
        client: &Client,
        grace_seconds: f64,
    ) -> Result<u64, CollectError> {
        let mut retired = 0u64;
        let mut after = 0i64;
        let limit = i64::try_from(BATCH_SIZE).unwrap_or(i64::MAX);
        loop {
            let batch = retired_objects()
                .bind(client, &after, &grace_seconds, &limit)
                .all()
                .await?;
            let Some(last) = batch.last() else {
                return Ok(retired);
            };
            after = last.id;
            let deleted: Vec<i64> = stream::iter(batch)
                .map(async |object| {
                    let store = self.stores.get(&object.store)?;
                    match store.delete(&Path::from(object.object_key.as_str())).await {
                        Ok(()) | Err(object_store::Error::NotFound { path: _, source: _ }) => {
                            Some(object.id)
                        }
                        Err(err) => {
                            tracing::warn!(
                                error = &err as &dyn Error,
                                key = object.object_key,
                                "failed to delete a retired object"
                            );
                            None
                        }
                    }
                })
                .buffer_unordered(DELETE_CONCURRENCY)
                .filter_map(future::ready)
                .collect()
                .await;
            retired = retired.saturating_add(
                forget_retired_objects()
                    .bind(client, &deleted.as_slice())
                    .await?,
            );
        }
    }
}

/// Errors from collecting unreferenced content.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CollectError {
    /// Finding or deleting rows failed.
    #[error("failed to update the content records")]
    Database(#[from] tokio_postgres::Error),
    /// Deleting an object failed.
    #[error("failed to delete an object")]
    Delete(#[source] object_store::Error),
    /// The grace period reaches back further than timestamps can represent.
    #[error("the grace period is too long")]
    Grace,
    /// Listing the objects failed.
    #[error("failed to list the objects")]
    List(#[source] object_store::Error),
}

/// Deletes the objects of a store that no chunk references and that are
/// older than `cutoff`.
async fn sweep(
    client: &Client,
    name: &StoreName,
    store: &dyn ObjectStore,
    cutoff: DateTime<Utc>,
) -> Result<u64, CollectError> {
    let mut swept = 0u64;
    let mut listing = store
        .list(Some(&Path::from(CHUNK_PREFIX)))
        .try_chunks(BATCH_SIZE);
    while let Some(batch) = listing.next().await {
        let batch: Vec<ObjectMeta> = batch.map_err(|err| CollectError::List(err.1))?;
        let keys: Vec<&str> = batch.iter().map(|meta| meta.location.as_ref()).collect();
        let known: HashSet<String> = known_objects()
            .bind(client, &name.as_ref(), &keys.as_slice())
            .all()
            .await?
            .into_iter()
            .collect();
        for meta in batch {
            if meta.last_modified >= cutoff || known.contains(meta.location.as_ref()) {
                continue;
            }
            match store.delete(&meta.location).await {
                Ok(()) | Err(object_store::Error::NotFound { path: _, source: _ }) => {
                    swept = swept.saturating_add(1);
                }
                Err(err) => return Err(CollectError::Delete(err)),
            }
        }
    }
    Ok(swept)
}
