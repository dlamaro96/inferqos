//! OpenTelemetry initialization and bounded-cardinality runtime instruments.
#![forbid(unsafe_code)]

use opentelemetry::{
    KeyValue, global,
    metrics::{Counter, Histogram, UpDownCounter},
    trace::TracerProvider,
};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{Resource, metrics::SdkMeterProvider, trace::SdkTracerProvider};
use std::time::Duration;
use thiserror::Error;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub struct TelemetryGuard {
    tracer: Option<SdkTracerProvider>,
    meter: Option<SdkMeterProvider>,
}

impl TelemetryGuard {
    pub fn shutdown(mut self) {
        if let Some(provider) = self.tracer.take() {
            let _ = provider.shutdown();
        }
        if let Some(provider) = self.meter.take() {
            let _ = provider.shutdown();
        }
    }
}

/// Initialize structured logs always, and OTLP traces/metrics only when an OTLP endpoint is set.
pub fn init() -> Result<TelemetryGuard, TelemetryError> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("inferqos=info"));
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
    if let Some(endpoint) = endpoint {
        let resource = Resource::builder().with_service_name("inferqos").build();
        let span_exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint.clone())
            .with_timeout(Duration::from_secs(5))
            .build()
            .map_err(|error| TelemetryError::Configuration(error.to_string()))?;
        let tracer_provider = SdkTracerProvider::builder()
            .with_resource(resource.clone())
            .with_batch_exporter(span_exporter)
            .build();
        let tracer = tracer_provider.tracer("inferqos");
        let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .with_timeout(Duration::from_secs(5))
            .build()
            .map_err(|error| TelemetryError::Configuration(error.to_string()))?;
        let meter_provider = SdkMeterProvider::builder()
            .with_resource(resource)
            .with_periodic_exporter(metric_exporter)
            .build();
        global::set_tracer_provider(tracer_provider.clone());
        global::set_meter_provider(meter_provider.clone());
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().json().with_target(false))
            .with(tracing_opentelemetry::layer().with_tracer(tracer))
            .try_init()
            .map_err(|error| TelemetryError::Configuration(error.to_string()))?;
        Ok(TelemetryGuard {
            tracer: Some(tracer_provider),
            meter: Some(meter_provider),
        })
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().with_target(false))
            .try_init()
            .map_err(|error| TelemetryError::Configuration(error.to_string()))?;
        Ok(TelemetryGuard {
            tracer: None,
            meter: None,
        })
    }
}

#[derive(Clone)]
pub struct RuntimeMetrics {
    requests: Counter<u64>,
    active: UpDownCounter<i64>,
    admission_latency: Histogram<f64>,
    queue_latency: Histogram<f64>,
    estimated_work: Histogram<f64>,
    actual_work: Histogram<f64>,
    estimation_error: Histogram<f64>,
    throttles: Counter<u64>,
    coordinator_failures: Counter<u64>,
}

impl Default for RuntimeMetrics {
    fn default() -> Self {
        let meter = global::meter("inferqos.runtime");
        Self {
            requests: meter.u64_counter("inferqos.requests").build(),
            active: meter
                .i64_up_down_counter("inferqos.active_requests")
                .build(),
            admission_latency: meter
                .f64_histogram("inferqos.admission.decision.duration")
                .with_unit("s")
                .build(),
            queue_latency: meter
                .f64_histogram("inferqos.scheduler.queue.duration")
                .with_unit("s")
                .build(),
            estimated_work: meter
                .f64_histogram("inferqos.work.estimated")
                .with_unit("{work_unit}")
                .build(),
            actual_work: meter
                .f64_histogram("inferqos.work.actual")
                .with_unit("{work_unit}")
                .build(),
            estimation_error: meter
                .f64_histogram("inferqos.work.estimation_error")
                .build(),
            throttles: meter.u64_counter("inferqos.provider.throttles").build(),
            coordinator_failures: meter.u64_counter("inferqos.coordinator.failures").build(),
        }
    }
}

impl RuntimeMetrics {
    fn class(class: &str) -> [KeyValue; 1] {
        [KeyValue::new("inferqos.service_class", class.to_owned())]
    }
    pub fn request(&self, class: &str, work: f64) {
        let labels = Self::class(class);
        self.requests.add(1, &labels);
        self.estimated_work.record(work, &labels);
    }
    pub fn active(&self, delta: i64, class: &str) {
        self.active.add(delta, &Self::class(class));
    }
    pub fn admission(&self, seconds: f64, class: &str) {
        self.admission_latency.record(seconds, &Self::class(class));
    }
    pub fn queue(&self, seconds: f64, class: &str) {
        self.queue_latency.record(seconds, &Self::class(class));
    }
    pub fn throttle(&self, provider: &'static str) {
        self.throttles
            .add(1, &[KeyValue::new("gen_ai.provider.name", provider)]);
    }
    pub fn coordinator_failure(&self) {
        self.coordinator_failures.add(1, &[]);
    }
    pub fn reconciliation(&self, estimated: f64, actual: f64, class: &str) {
        let labels = Self::class(class);
        self.actual_work.record(actual, &labels);
        if estimated > 0.0 {
            self.estimation_error
                .record((actual - estimated) / estimated, &labels);
        }
    }
}

#[derive(Debug, Error)]
pub enum TelemetryError {
    #[error("telemetry configuration failed: {0}")]
    Configuration(String),
}
