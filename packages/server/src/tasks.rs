//! Background tasks recorded in Postgres.

mod metrics;

use core::error::Error as _;
use core::str::FromStr;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;
use std::{env, process};

use calf_granary_server_db::queries::content::{StoreSizes, store_sizes};
use calf_granary_server_db::queries::prune::{record_prune_collection, record_prune_expiry};
use chrono::Utc;
use deadpool_postgres::{Client, Pool, PoolError};
use tracing::Instrument as _;

use self::metrics::TaskMetrics;
use crate::cache::{CacheName, Caches, ExpireError};
use crate::content::{CollectError, CollectStats, Content};

/// A kind of task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TaskKind {
    /// Delete objects that weren't used within their cache's retention.
    Prune,
}

impl TaskKind {
    /// The name stored in the database.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Prune => "prune",
        }
    }
}

impl FromStr for TaskKind {
    type Err = UnknownTaskKind;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "prune" => Ok(Self::Prune),
            _ => Err(UnknownTaskKind),
        }
    }
}

/// A task kind that this binary doesn't know.
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("unknown task kind")]
pub(crate) struct UnknownTaskKind;

/// Identifies the process that runs a task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkerId(String);

impl WorkerId {
    /// The ID of this process, from the host name and the process ID.
    pub(crate) fn current() -> Self {
        let host = env::var("HOSTNAME").unwrap_or_else(|_unset| "unknown".to_owned());
        Self(format!("{host}:{}", process::id()))
    }
}

impl AsRef<str> for WorkerId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Runs tasks.
#[derive(Debug)]
pub(crate) struct Runner {
    /// Postgres connections.
    pool: Pool,
    /// The caches.
    caches: Arc<Caches>,
    /// The content of the entries.
    content: Arc<Content>,
    /// Records task runs.
    metrics: TaskMetrics,
}

impl Runner {
    /// Creates a runner.
    pub(crate) fn new(pool: Pool, caches: Arc<Caches>, content: Arc<Content>) -> Self {
        Self {
            pool,
            caches,
            content,
            metrics: TaskMetrics::new(),
        }
    }

    /// Runs a task.
    pub(crate) async fn run(&self, id: i64, kind: TaskKind) -> Result<(), TaskError> {
        let span = tracing::info_span!("task", otel.name = kind.name(), task.id = id);
        let start = Instant::now();
        let result = match kind {
            TaskKind::Prune => self.prune(id).instrument(span).await,
        };
        self.metrics.finished(kind, result.is_ok(), start.elapsed());
        result
    }

    /// Expires the entries of every cache, collects unreferenced content
    /// and records what was deleted.
    async fn prune(&self, id: i64) -> Result<(), TaskError> {
        let client = self.pool.get().await?;
        let now = Utc::now();
        let count = |value: u64| i64::try_from(value).unwrap_or(i64::MAX);
        for (name, cache) in self.caches.iter() {
            let expired = cache
                .expire(&client, now)
                .await
                .map_err(|source| TaskError::Expire {
                    cache: name.clone(),
                    source,
                })?;
            let Some(expired) = expired else {
                continue;
            };
            self.metrics.expired(name, expired);
            record_prune_expiry()
                .bind(&client, &id, &name.as_ref(), &count(expired))
                .await?;
            tracing::info!(cache = name.as_ref(), expired, "expired entries");
        }

        let stats = self.content.collect(&client, now).await?;
        self.metrics.collected(stats);
        let CollectStats {
            deleted_blobs,
            deleted_chunks,
            freed_bytes,
            swept_objects,
        } = stats;
        record_prune_collection()
            .bind(
                &client,
                &id,
                &count(deleted_blobs),
                &count(deleted_chunks),
                &count(freed_bytes),
                &count(swept_objects),
            )
            .await?;
        tracing::info!(
            deleted_blobs,
            deleted_chunks,
            freed_bytes,
            swept_objects,
            "collected unreferenced content"
        );
        self.record_store_sizes(&client).await
    }

    /// Records the size of every configured store.
    async fn record_store_sizes(&self, client: &Client) -> Result<(), TaskError> {
        let sizes: BTreeMap<String, StoreSizes> = store_sizes()
            .bind(client)
            .all()
            .await?
            .into_iter()
            .map(|row| (row.store.clone(), row))
            .collect();
        for store in self.content.store_names() {
            let (chunks, bytes) = sizes.get(store.as_ref()).map_or((0, 0), |row| {
                (
                    u64::try_from(row.chunks).unwrap_or_default(),
                    u64::try_from(row.stored_bytes).unwrap_or_default(),
                )
            });
            self.metrics.store(store.as_ref(), chunks, bytes);
        }
        Ok(())
    }
}

/// Errors from running a task.
#[derive(Debug, thiserror::Error)]
pub(crate) enum TaskError {
    /// Collecting unreferenced content failed.
    #[error("failed to collect unreferenced content")]
    Collect(#[from] CollectError),
    /// Recording the task's results failed.
    #[error("failed to record the results")]
    Database(#[from] tokio_postgres::Error),
    /// No database connection was available.
    #[error("failed to get a database connection")]
    Pool(#[from] PoolError),
    /// Expiring the entries of a cache failed.
    #[error("failed to expire the entries of cache {cache}")]
    Expire {
        /// The cache.
        cache: CacheName,
        /// The error from expiring.
        #[source]
        source: ExpireError,
    },
}

impl TaskError {
    /// Describes the error and its sources for the task record.
    pub(crate) fn describe(&self) -> String {
        let mut description = self.to_string();
        let mut source = self.source();
        while let Some(err) = source {
            description.push_str(": ");
            description.push_str(&err.to_string());
            source = err.source();
        }
        description
    }
}
