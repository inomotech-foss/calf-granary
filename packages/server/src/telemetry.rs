//! Logs on stdout and traces, metrics and logs over OTLP.

use core::error::Error;
use std::env;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{KeyValue, global};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{ExporterBuildError, LogExporter, MetricExporter, SpanExporter};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::resource::{EnvResourceDetector, TelemetryResourceDetector};
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing::Metadata;
use tracing_subscriber::filter::{LevelFilter, filter_fn};
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::{SubscriberInitExt as _, TryInitError};
use tracing_subscriber::{EnvFilter, Layer as _, fmt};

/// The instrumentation scope of everything this crate emits.
pub(crate) const SCOPE: &str = "calf-granary-server";

/// The `service.namespace` that the commands share.
const SERVICE_NAMESPACE: &str = "calf-granary";

/// A kind of telemetry that OTLP carries.
#[derive(Clone, Copy, Debug)]
enum Signal {
    /// Log records.
    Logs,
    /// Metrics.
    Metrics,
    /// Spans.
    Traces,
}

impl Signal {
    /// Checks whether an OTLP endpoint is configured for the signal.
    fn enabled(self) -> bool {
        let specific = match self {
            Self::Logs => "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
            Self::Metrics => "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
            Self::Traces => "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
        };
        ["OTEL_EXPORTER_OTLP_ENDPOINT", specific]
            .into_iter()
            .any(|name| env::var_os(name).is_some_and(|value| !value.is_empty()))
    }
}

/// The OTLP providers, which flush on shutdown.
#[derive(Debug)]
pub(crate) struct Telemetry {
    /// Exports log records.
    logs: Option<SdkLoggerProvider>,
    /// Exports metrics.
    metrics: Option<SdkMeterProvider>,
    /// Exports spans.
    traces: Option<SdkTracerProvider>,
}

impl Telemetry {
    /// Installs the global subscriber and meter provider.
    ///
    /// Logs always go to stdout. A signal is exported over OTLP/HTTP only when
    /// `OTEL_EXPORTER_OTLP_ENDPOINT` or its signal-specific variant is set.
    /// `service` becomes the `service.name` unless `OTEL_SERVICE_NAME` is set.
    pub(crate) fn init(service: &'static str) -> Result<Self, TelemetryError> {
        let resource = resource(service);
        let traces = Signal::Traces
            .enabled()
            .then(|| {
                SpanExporter::builder().with_http().build().map(|exporter| {
                    SdkTracerProvider::builder()
                        .with_resource(resource.clone())
                        .with_batch_exporter(exporter)
                        .build()
                })
            })
            .transpose()?;
        let metrics = Signal::Metrics
            .enabled()
            .then(|| {
                MetricExporter::builder()
                    .with_http()
                    .build()
                    .map(|exporter| {
                        SdkMeterProvider::builder()
                            .with_resource(resource.clone())
                            .with_periodic_exporter(exporter)
                            .build()
                    })
            })
            .transpose()?;
        let logs = Signal::Logs
            .enabled()
            .then(|| {
                LogExporter::builder().with_http().build().map(|exporter| {
                    SdkLoggerProvider::builder()
                        .with_resource(resource)
                        .with_batch_exporter(exporter)
                        .build()
                })
            })
            .transpose()?;

        let spans = traces.as_ref().map(|provider| {
            tracing_opentelemetry::layer()
                .with_tracer(provider.tracer(SCOPE))
                .with_level(true)
                .with_filter(filter_fn(is_span))
        });
        tracing_subscriber::registry()
            .with(filter())
            .with(fmt::layer())
            .with(spans)
            .with(logs.as_ref().map(OpenTelemetryTracingBridge::new))
            .try_init()?;
        if let Some(metrics) = &metrics {
            global::set_meter_provider(metrics.clone());
        }
        Ok(Self {
            logs,
            metrics,
            traces,
        })
    }

    /// Installs a subscriber that only logs to stdout, for when [`Self::init`]
    /// fails.
    pub(crate) fn fallback() {
        fmt().with_env_filter(filter()).init();
    }

    /// Flushes and stops the providers.
    ///
    /// Blocks until the exporters are done.
    pub(crate) fn shutdown(self) {
        if let Some(traces) = self.traces
            && let Err(err) = traces.shutdown()
        {
            tracing::warn!(error = &err as &dyn Error, "failed to flush traces");
        }
        if let Some(metrics) = self.metrics
            && let Err(err) = metrics.shutdown()
        {
            tracing::warn!(error = &err as &dyn Error, "failed to flush metrics");
        }
        if let Some(logs) = self.logs
            && let Err(err) = logs.shutdown()
        {
            tracing::warn!(error = &err as &dyn Error, "failed to flush logs");
        }
    }
}

/// Errors from setting up telemetry.
#[derive(Debug, thiserror::Error)]
pub(crate) enum TelemetryError {
    /// An OTLP exporter can't be created.
    #[error("failed to create an OTLP exporter")]
    Exporter(#[from] ExporterBuildError),
    /// A global subscriber is already installed.
    #[error("failed to install the tracing subscriber")]
    Subscriber(#[from] TryInitError),
}

/// The `RUST_LOG` filter, `info` by default.
fn filter() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy()
}

/// Checks whether the callsite is a span, not an event.
///
/// Events are exported as log records only.
fn is_span(metadata: &Metadata<'_>) -> bool {
    metadata.is_span()
}

/// Describes this process.
///
/// `OTEL_RESOURCE_ATTRIBUTES` overrides the defaults.
fn resource(service: &'static str) -> Resource {
    let mut defaults = vec![
        KeyValue::new("service.namespace", SERVICE_NAMESPACE),
        KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
    ];
    if let Ok(host) = env::var("HOSTNAME") {
        defaults.push(KeyValue::new("service.instance.id", host));
    }
    let mut builder = Resource::builder_empty()
        .with_detector(Box::new(TelemetryResourceDetector))
        .with_service_name(service)
        .with_attributes(defaults)
        .with_detector(Box::new(EnvResourceDetector::new()));
    if let Ok(name) = env::var("OTEL_SERVICE_NAME")
        && !name.is_empty()
    {
        builder = builder.with_service_name(name);
    }
    builder.build()
}
