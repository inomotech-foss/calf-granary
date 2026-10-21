//! Metrics for tasks.

use core::time::Duration;

use opentelemetry::metrics::Histogram;
use opentelemetry::{KeyValue, global};

use super::TaskKind;
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
        }
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
