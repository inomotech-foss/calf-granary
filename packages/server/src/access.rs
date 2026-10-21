//! Recording when objects were last read.
//!
//! Reads are collected in memory and written to Postgres about once a minute.
//! An object that was written recently isn't written again, because retention
//! is measured in days.

use core::error::Error;
use core::time::Duration;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use calf_granary_server_db::queries::content::record_entry_accesses;
use deadpool_postgres::{Pool, PoolError};
use tokio::sync::oneshot;
use tokio::time::{self, MissedTickBehavior};

use crate::cache::{CacheName, ObjectKey};

/// How often reads are written to Postgres.
const FLUSH_INTERVAL: Duration = Duration::from_secs(60);

/// How long an object isn't written again after it was written.
const RECORD_INTERVAL: Duration = Duration::from_hours(1);

/// The most keys written by one query.
const BATCH_SIZE: usize = 1000;

/// An entry that was read.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Accessed {
    /// The cache of the entry.
    pub(crate) cache: CacheName,
    /// The key of the entry.
    pub(crate) key: ObjectKey,
}

/// Reads that haven't been written to Postgres yet.
#[derive(Debug, Default)]
struct Accesses {
    /// Objects read since the last flush.
    pending: HashSet<Accessed>,
    /// When objects were last written.
    written: HashMap<Accessed, Instant>,
}

impl Accesses {
    /// Records a read unless the object was written recently.
    fn record(&mut self, object: Accessed, now: Instant) {
        let recent = self
            .written
            .get(&object)
            .is_some_and(|written| now.duration_since(*written) < RECORD_INTERVAL);
        if !recent {
            self.pending.insert(object);
        }
    }

    /// Takes the pending reads and forgets old writes.
    fn take(&mut self, now: Instant) -> Vec<Accessed> {
        self.written
            .retain(|_, written| now.duration_since(*written) < RECORD_INTERVAL);
        self.pending.drain().collect()
    }

    /// Notes that objects were written.
    fn written(&mut self, objects: &[Accessed], now: Instant) {
        for object in objects {
            self.written.insert(object.clone(), now);
        }
    }

    /// Puts back reads that failed to be written.
    fn restore(&mut self, objects: Vec<Accessed>) {
        self.pending.extend(objects);
    }
}

/// Collects object reads and writes them to Postgres in batches.
#[derive(Debug, Default)]
pub(crate) struct AccessLog {
    /// The reads.
    accesses: Mutex<Accesses>,
}

impl AccessLog {
    /// Records that an object was read.
    pub(crate) fn record(&self, object: Accessed) {
        self.lock().record(object, Instant::now());
    }

    /// Writes reads to Postgres periodically until `stop` resolves, then
    /// writes the remaining reads.
    pub(crate) async fn run(self: Arc<Self>, pool: Pool, mut stop: oneshot::Receiver<()>) {
        let mut interval = time::interval(FLUSH_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = interval.tick() => self.flush(&pool).await,
                _ = &mut stop => break,
            }
        }
        self.flush(&pool).await;
    }

    /// Writes the pending reads. Failed reads are kept for the next flush.
    async fn flush(&self, pool: &Pool) {
        let now = Instant::now();
        let objects = self.lock().take(now);
        if objects.is_empty() {
            return;
        }
        match write(pool, &objects).await {
            Ok(()) => {
                tracing::debug!(count = objects.len(), "recorded object reads");
                self.lock().written(&objects, now);
            }
            Err(err) => {
                tracing::error!(error = &err as &dyn Error, "failed to record object reads");
                self.lock().restore(objects);
            }
        }
    }

    /// Locks the reads, ignoring poisoning since every update is complete.
    fn lock(&self) -> MutexGuard<'_, Accesses> {
        self.accesses.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Errors from writing reads to Postgres.
#[derive(Debug, thiserror::Error)]
enum FlushError {
    /// No database connection was available.
    #[error("failed to get a database connection")]
    Pool(#[from] PoolError),
    /// The query failed.
    #[error("failed to record object reads")]
    Query(#[from] tokio_postgres::Error),
}

/// Writes reads to Postgres.
async fn write(pool: &Pool, entries: &[Accessed]) -> Result<(), FlushError> {
    let mut caches: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for entry in entries {
        caches
            .entry(entry.cache.as_ref())
            .or_default()
            .push(entry.key.as_ref());
    }
    let client = pool.get().await?;
    let statement = record_entry_accesses();
    for (cache, keys) in caches {
        for batch in keys.chunks(BATCH_SIZE) {
            statement.bind(&client, &batch, &cache).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(key: &str) -> Accessed {
        Accessed {
            cache: CacheName::try_from("main".to_owned()).unwrap(),
            key: ObjectKey::try_from(key).unwrap(),
        }
    }

    #[test]
    fn take_returns_reads_once() {
        let now = Instant::now();
        let mut accesses = Accesses::default();
        accesses.record(object("a"), now);
        accesses.record(object("a"), now);
        assert_eq!(accesses.take(now), [object("a")], "reads are deduplicated");
        assert!(accesses.take(now).is_empty(), "reads are taken once");
    }

    #[test]
    fn recent_writes_are_skipped() {
        let now = Instant::now();
        let mut accesses = Accesses::default();
        accesses.written(&[object("a")], now);
        accesses.record(object("a"), now + RECORD_INTERVAL / 2);
        assert!(
            accesses.take(now + RECORD_INTERVAL / 2).is_empty(),
            "a recent write is not repeated"
        );
        accesses.record(object("a"), now + RECORD_INTERVAL);
        assert_eq!(
            accesses.take(now + RECORD_INTERVAL),
            [object("a")],
            "an old write is repeated"
        );
    }

    #[test]
    fn restored_reads_are_retried() {
        let now = Instant::now();
        let mut accesses = Accesses::default();
        accesses.record(object("a"), now);
        let taken = accesses.take(now);
        accesses.restore(taken);
        assert_eq!(accesses.take(now), [object("a")], "failed reads are kept");
    }
}
