//! Metrics for tasks.

use core::time::Duration;

use opentelemetry::metrics::{Counter, Histogram};
use opentelemetry::{KeyValue, global};

use super::TaskKind;
use crate::cache::{CacheName, PruneStats};
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
    /// Objects checked by pruning, by cache.
    scanned: Counter<u64>,
    /// Objects deleted by pruning, by cache.
    deleted: Counter<u64>,
    /// Bytes freed by pruning, by cache.
    freed: Counter<u64>,
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
            scanned: meter
                .u64_counter("granary.prune.scanned")
                .with_description("Objects checked by pruning")
                .build(),
            deleted: meter
                .u64_counter("granary.prune.deleted")
                .with_description("Objects deleted by pruning")
                .build(),
            freed: meter
                .u64_counter("granary.prune.freed")
                .with_unit("By")
                .with_description("Content deleted by pruning")
                .build(),
        }
    }

    /// Records what pruning did to a cache.
    pub(super) fn pruned(&self, cache: &CacheName, stats: PruneStats) {
        let attributes = [KeyValue::new("cache", cache.to_string())];
        self.scanned.add(stats.scanned, &attributes);
        self.deleted.add(stats.deleted, &attributes);
        self.freed.add(stats.freed_bytes, &attributes);
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
