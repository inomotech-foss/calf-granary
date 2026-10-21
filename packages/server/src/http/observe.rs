//! Spans and metrics for requests.

use std::time::Instant;

use axum::extract::{MatchedPath, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use opentelemetry::metrics::Histogram;
use opentelemetry::{KeyValue, global};
use tracing::Instrument as _;
use tracing::field::Empty;

use crate::telemetry::SCOPE;

/// Upper bounds of the request duration buckets, in seconds.
///
/// The semantic convention buckets, extended for large uploads.
const DURATION_BUCKETS: [f64; 18] = [
    0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0, 30.0, 60.0,
    120.0, 300.0,
];

/// The instruments that requests are recorded with.
#[derive(Clone, Debug)]
pub(super) struct RequestMetrics {
    /// How long requests took until the response headers were sent.
    duration: Histogram<f64>,
}

impl RequestMetrics {
    /// Creates the instruments on the global meter provider.
    pub(super) fn new() -> Self {
        let meter = global::meter(SCOPE);
        Self {
            duration: meter
                .f64_histogram("http.server.request.duration")
                .with_unit("s")
                .with_description("Duration of HTTP server requests")
                .with_boundaries(DURATION_BUCKETS.to_vec())
                .build(),
        }
    }
}

/// Runs the request in a span and records its duration.
pub(super) async fn observe(
    State(metrics): State<RequestMetrics>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().as_str().to_owned();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or_else(String::new, |path| path.as_str().to_owned());
    let span = tracing::info_span!(
        "request",
        otel.name = format!("{method} {route}"),
        otel.kind = "server",
        http.request.method = method,
        http.route = route,
        http.response.status_code = Empty,
    );
    let start = Instant::now();
    let response = next.run(request).instrument(span.clone()).await;
    let status = response.status().as_u16();
    span.record("http.response.status_code", status);
    metrics.duration.record(
        start.elapsed().as_secs_f64(),
        &[
            KeyValue::new("http.request.method", method),
            KeyValue::new("http.route", route),
            KeyValue::new("http.response.status_code", i64::from(status)),
        ],
    );
    response
}
