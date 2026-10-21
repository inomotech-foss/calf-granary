//! Metrics for tasks.

use core::time::Duration;

use opentelemetry::metrics::{Counter, Gauge, Histogram};
use opentelemetry::{KeyValue, global};

use super::TaskKind;
use crate::cache::CacheName;
use crate::content::{CollectStats, TierStats};
use crate::telemetry::SCOPE;

/// Upper bounds of the task duration buckets, in seconds.
const DURATION_BUCKETS: [f64; 10] = [
    1.0, 5.0, 15.0, 30.0, 60.0, 300.0, 900.0, 1800.0, 3600.0, 7200.0,
];

/// The instruments that tasks are recorded with.
#[derive(Clone, Debug)]
pub(super) struct TaskMetrics {
    /// How long finished tasks ran, by kind and outcome.
    duration: Histogram<f64>,
    /// Entries expired by pruning, by cache.
    expired: Counter<u64>,
    /// Chunks deleted by collection.
    deleted_chunks: Counter<u64>,
    /// Stored bytes freed by collection.
    freed: Counter<u64>,
    /// Chunks moved to longer-lived stores.
    tiered_chunks: Counter<u64>,
    /// Stored bytes moved to longer-lived stores.
    tiered_bytes: Counter<u64>,
    /// Stored bytes per store.
    store_size: Gauge<u64>,
    /// Chunks per store.
    store_chunks: Gauge<u64>,
}

impl TaskMetrics {
    /// Creates the instruments on the global meter provider.
    pub(super) fn new() -> Self {
        let meter = global::meter(SCOPE);
        Self {
            duration: meter
                .f64_histogram("granary.task.duration")
                .with_unit("s")
                .with_description("Duration of finished task runs")
                .with_boundaries(DURATION_BUCKETS.to_vec())
                .build(),
            expired: meter
                .u64_counter("granary.prune.expired")
                .with_description("Entries expired by pruning")
                .build(),
            deleted_chunks: meter
                .u64_counter("granary.collect.chunks")
                .with_description("Chunks deleted by collection")
                .build(),
            freed: meter
                .u64_counter("granary.collect.freed")
                .with_unit("By")
                .with_description("Stored content deleted by collection")
                .build(),
            tiered_chunks: meter
                .u64_counter("granary.tier.chunks")
                .with_description("Chunks moved to longer-lived stores")
                .build(),
            tiered_bytes: meter
                .u64_counter("granary.tier.moved")
                .with_unit("By")
                .with_description("Stored content moved to longer-lived stores")
                .build(),
            store_size: meter
                .u64_gauge("granary.store.size")
                .with_unit("By")
                .with_description("Stored content per store")
                .build(),
            store_chunks: meter
                .u64_gauge("granary.store.chunks")
                .with_description("Chunks per store")
                .build(),
        }
    }

    /// Records the entries expired in a cache.
    pub(super) fn expired(&self, cache: &CacheName, count: u64) {
        self.expired
            .add(count, &[KeyValue::new("cache", cache.to_shared())]);
    }

    /// Records what collection deleted.
    pub(super) fn collected(&self, stats: CollectStats) {
        self.deleted_chunks.add(stats.deleted_chunks, &[]);
        self.freed.add(stats.freed_bytes, &[]);
    }

    /// Records what tiering moved.
    pub(super) fn tiered(&self, stats: TierStats) {
        self.tiered_chunks.add(stats.moved_chunks, &[]);
        self.tiered_bytes.add(stats.moved_bytes, &[]);
    }

    /// Records the size of a store.
    pub(super) fn store(&self, store: &str, chunks: u64, bytes: u64) {
        let attributes = [KeyValue::new("store", store.to_owned())];
        self.store_size.record(bytes, &attributes);
        self.store_chunks.record(chunks, &attributes);
    }

    /// Records a finished task run.
    pub(super) fn finished(&self, kind: TaskKind, succeeded: bool, duration: Duration) {
        let result = if succeeded { "completed" } else { "failed" };
        self.duration.record(
            duration.as_secs_f64(),
            &[
                KeyValue::new("kind", kind.name()),
                KeyValue::new("result", result),
            ],
        );
    }
}
