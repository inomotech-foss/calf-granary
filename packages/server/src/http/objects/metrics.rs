//! Metrics for object reads and writes.

use opentelemetry::metrics::Counter;
use opentelemetry::{KeyValue, global};

use crate::telemetry::SCOPE;

/// The outcome of looking up an object.
#[derive(Clone, Copy, Debug)]
pub(super) enum Lookup {
    /// The cache had the object.
    Hit,
    /// The fallback cache had the object.
    Fallback,
    /// Neither cache had the object.
    Miss,
    /// Object storage failed.
    Error,
}

impl Lookup {
    /// The value of the `result` attribute.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Hit => "hit",
            Self::Fallback => "fallback",
            Self::Miss => "miss",
            Self::Error => "error",
        }
    }
}

/// The outcome of an upload.
#[derive(Clone, Copy, Debug)]
pub(super) enum Upload {
    /// The object was stored.
    Stored,
    /// The cache already had the object.
    Existing,
    /// The client stopped sending the body.
    Aborted,
    /// Object storage failed.
    Error,
}

impl Upload {
    /// The value of the `result` attribute.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Stored => "stored",
            Self::Existing => "existing",
            Self::Aborted => "aborted",
            Self::Error => "error",
        }
    }
}

/// The instruments that object reads and writes are recorded with.
#[derive(Clone, Debug)]
pub(super) struct ObjectMetrics {
    /// Object lookups by cache and outcome.
    lookups: Counter<u64>,
    /// Bytes of object content sent to clients, by cache.
    served: Counter<u64>,
    /// Uploads by cache and outcome.
    uploads: Counter<u64>,
    /// Bytes of stored objects, by cache.
    stored: Counter<u64>,
}

impl ObjectMetrics {
    /// Creates the instruments on the global meter provider.
    pub(super) fn new() -> Self {
        let meter = global::meter(SCOPE);
        Self {
            lookups: meter
                .u64_counter("granary.cache.lookups")
                .with_description("Object lookups by outcome")
                .build(),
            served: meter
                .u64_counter("granary.cache.served")
                .with_unit("By")
                .with_description("Object content sent to clients")
                .build(),
            uploads: meter
                .u64_counter("granary.cache.uploads")
                .with_description("Object uploads by outcome")
                .build(),
            stored: meter
                .u64_counter("granary.cache.stored")
                .with_unit("By")
                .with_description("Content of newly stored objects")
                .build(),
        }
    }

    /// Records the outcome of a lookup.
    pub(super) fn lookup(&self, cache: &str, lookup: Lookup) {
        self.lookups.add(
            1,
            &[
                KeyValue::new("cache", cache.to_owned()),
                KeyValue::new("result", lookup.as_str()),
            ],
        );
    }

    /// Records the outcome of an upload.
    pub(super) fn upload(&self, cache: &str, upload: Upload) {
        self.uploads.add(
            1,
            &[
                KeyValue::new("cache", cache.to_owned()),
                KeyValue::new("result", upload.as_str()),
            ],
        );
    }

    /// Records the content of a newly stored object.
    pub(super) fn stored(&self, cache: &str, size: u64) {
        self.stored
            .add(size, &[KeyValue::new("cache", cache.to_owned())]);
    }

    /// Records object content that is sent to a client.
    pub(super) fn served(&self, cache: &str, size: u64) {
        self.served
            .add(size, &[KeyValue::new("cache", cache.to_owned())]);
    }
}
