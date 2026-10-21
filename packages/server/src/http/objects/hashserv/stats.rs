//! Request statistics of hash equivalence caches, as upstream reports them.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use opentelemetry::metrics::Histogram;
use opentelemetry::{KeyValue, global};
use serde_json::{Value, json};

use crate::telemetry::SCOPE;

/// Upper bounds of the request duration buckets, in seconds.
const DURATION_BUCKETS: [f64; 10] = [
    0.000_5, 0.001, 0.002_5, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 1.0,
];

/// Running statistics of request durations.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Requests {
    /// How many requests were answered.
    num: u64,
    /// Their total duration in seconds.
    total: f64,
    /// The longest duration in seconds.
    max: f64,
    /// The mean duration in seconds.
    mean: f64,
    /// The sum of squared differences from the mean.
    m2: f64,
}

impl Requests {
    /// Adds a request with Welford's method.
    fn add(&mut self, seconds: f64) {
        self.num = self.num.saturating_add(1);
        self.total += seconds;
        self.max = self.max.max(seconds);
        let delta = seconds - self.mean;
        self.mean += delta / count(self.num);
        self.m2 = delta.mul_add(seconds - self.mean, self.m2);
    }

    /// The statistics in the upstream format.
    fn to_json(self) -> Value {
        let stdev = if self.num > 1 {
            (self.m2 / (count(self.num) - 1.0)).sqrt()
        } else {
            0.0
        };
        json!({
            "requests": {
                "num": self.num,
                "total_time": self.total,
                "max_time": self.max,
                "average": self.mean,
                "stdev": stdev,
            }
        })
    }
}

/// Request statistics per hashserv cache.
#[derive(Debug)]
pub(crate) struct HashservStats {
    /// The statistics by cache.
    requests: Mutex<HashMap<String, Requests>>,
    /// Request durations by cache and command.
    duration: Histogram<f64>,
}

impl HashservStats {
    /// Creates empty statistics and the instruments on the global meter
    /// provider.
    pub(crate) fn new() -> Self {
        Self {
            requests: Mutex::new(HashMap::new()),
            duration: global::meter(SCOPE)
                .f64_histogram("granary.hashserv.request.duration")
                .with_unit("s")
                .with_description("Duration of hash equivalence requests outside streams")
                .with_boundaries(DURATION_BUCKETS.to_vec())
                .build(),
        }
    }

    /// Records a request.
    pub(super) fn record(&self, cache: &str, command: &str, seconds: f64) {
        self.lock()
            .entry(cache.to_owned())
            .or_default()
            .add(seconds);
        self.duration.record(
            seconds,
            &[
                KeyValue::new("cache", cache.to_owned()),
                KeyValue::new("command", command.to_owned()),
            ],
        );
    }

    /// The statistics of a cache.
    pub(super) fn get(&self, cache: &str) -> Value {
        self.lock()
            .get(cache)
            .copied()
            .unwrap_or_default()
            .to_json()
    }

    /// Resets the statistics of a cache and returns them as they were.
    pub(super) fn reset(&self, cache: &str) -> Value {
        self.lock().remove(cache).unwrap_or_default().to_json()
    }

    /// Locks the statistics, also after a panic elsewhere.
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Requests>> {
        self.requests.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A count as a float, saturating for absurd counts.
fn count(num: u64) -> f64 {
    f64::from(u32::try_from(num).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_upstream_statistics() {
        let mut requests = Requests::default();
        for seconds in [1.0, 2.0, 3.0, 4.0] {
            requests.add(seconds);
        }
        let stats = requests.to_json();
        let reported = &stats["requests"];
        assert_eq!(reported["num"], 4, "the count");
        assert_eq!(reported["total_time"], 10.0, "the total");
        assert_eq!(reported["max_time"], 4.0, "the maximum");
        assert_eq!(reported["average"], 2.5, "the mean");
        let stdev = reported["stdev"].as_f64().unwrap();
        assert!(
            (stdev - 1.290_994).abs() < 1e-6,
            "the sample deviation: {stdev}"
        );
    }
}
