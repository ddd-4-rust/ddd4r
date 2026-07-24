//! Explicit OTLP pipeline construction and provider lifecycle.

use std::sync::Arc;

use ddd4r_observability::{
    BoxedMetricsRecorder, BoxedObservabilityLayer, ObservabilityBuilder, ObservabilityRuntime,
};
use opentelemetry::KeyValue;
use opentelemetry::metrics::MeterProvider as _;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, MetricExporter, Protocol, SpanExporter, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::trace::{
    BatchConfigBuilder, BatchSpanProcessor, Sampler, SdkTracerProvider,
};
use opentelemetry_semantic_conventions::attribute::{
    DEPLOYMENT_ENVIRONMENT_NAME, SERVICE_INSTANCE_ID, SERVICE_NAMESPACE, SERVICE_VERSION,
};
use tracing_subscriber::Registry;

use crate::metrics::OpenTelemetryMetricsRecorder;
use crate::{OtlpConfig, OtlpError, OtlpResult};

/// Installed local observability plus explicit OpenTelemetry provider guards.
pub struct OtlpRuntime {
    observability: ObservabilityRuntime,
    providers: Providers,
}

impl std::fmt::Debug for OtlpRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OtlpRuntime")
            .field("service_name", &self.observability.service_name())
            .field("traces", &self.providers.traces.is_some())
            .field("metrics", &self.providers.metrics.is_some())
            .field("logs", &self.providers.logs.is_some())
            .finish_non_exhaustive()
    }
}

impl OtlpRuntime {
    /// Builds the selected OTLP signals and installs ddd4r observability once.
    ///
    /// The adapter uses OTLP/HTTP protobuf. A plaintext endpoint is accepted
    /// only for loopback development collectors; remote endpoints require
    /// HTTPS.
    pub fn install(config: OtlpConfig, builder: ObservabilityBuilder) -> OtlpResult<Self> {
        let endpoint = config.validate()?;
        let resource = build_resource(&config);
        let mut layers = Vec::new();
        let mut providers = Providers::default();
        let mut metrics_exporter = None;

        if config.signals.traces {
            let exporter = SpanExporter::builder()
                .with_http()
                .with_protocol(Protocol::HttpBinary)
                .with_endpoint(OtlpConfig::signal_endpoint(&endpoint, "traces"))
                .with_timeout(config.export_timeout)
                .build()
                .map_err(exporter_error)?;
            let batch = BatchSpanProcessor::builder(exporter)
                .with_batch_config(
                    BatchConfigBuilder::default()
                        .with_max_queue_size(config.trace_max_queue_size)
                        .with_max_export_batch_size(config.trace_max_export_batch_size)
                        .with_scheduled_delay(config.trace_scheduled_delay)
                        .build(),
                )
                .build();
            let provider = SdkTracerProvider::builder()
                .with_resource(resource.clone())
                .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
                    config.trace_sample_ratio,
                ))))
                .with_max_attributes_per_span(config.trace_max_attributes_per_span)
                .with_max_events_per_span(config.trace_max_events_per_span)
                .with_span_processor(batch)
                .build();
            let tracer = provider.tracer("ddd4r-observability");
            layers.push(
                Box::new(tracing_opentelemetry::layer::<Registry>().with_tracer(tracer))
                    as BoxedObservabilityLayer,
            );
            providers.traces = Some(provider);
        }

        if config.signals.metrics {
            let exporter = MetricExporter::builder()
                .with_http()
                .with_protocol(Protocol::HttpBinary)
                .with_endpoint(OtlpConfig::signal_endpoint(&endpoint, "metrics"))
                .with_timeout(config.export_timeout)
                .build()
                .map_err(exporter_error)?;
            let reader = PeriodicReader::builder(exporter)
                .with_interval(config.metric_export_interval)
                .build();
            let provider = SdkMeterProvider::builder()
                .with_resource(resource.clone())
                .with_reader(reader)
                .build();
            metrics_exporter = Some(Arc::new(OpenTelemetryMetricsRecorder::new(
                provider.meter("ddd4r-observability"),
                config.metrics_policy,
            )) as BoxedMetricsRecorder);
            providers.metrics = Some(provider);
        }

        if config.signals.logs {
            let exporter = LogExporter::builder()
                .with_http()
                .with_protocol(Protocol::HttpBinary)
                .with_endpoint(OtlpConfig::signal_endpoint(&endpoint, "logs"))
                .with_timeout(config.export_timeout)
                .build()
                .map_err(exporter_error)?;
            let provider = SdkLoggerProvider::builder()
                .with_resource(resource)
                .with_batch_exporter(exporter)
                .build();
            layers.push(
                Box::new(OpenTelemetryTracingBridge::new(&provider)) as BoxedObservabilityLayer
            );
            providers.logs = Some(provider);
        }

        let mut builder = builder.service_name(config.resource.service_name);
        for layer in layers {
            builder = builder.with_layer(layer);
        }
        if let Some(exporter) = metrics_exporter {
            builder = builder.with_metrics_exporter(exporter);
        }
        let observability = builder.install()?;

        Ok(Self {
            observability,
            providers,
        })
    }

    /// Accesses the default local tracing, metrics, and health runtime.
    pub fn observability(&self) -> &ObservabilityRuntime {
        &self.observability
    }

    /// Flushes every enabled provider without shutting it down.
    pub fn force_flush(&self) -> OtlpResult<()> {
        self.providers.force_flush()
    }

    /// Flushes and shuts down every enabled provider.
    pub fn shutdown(mut self) -> OtlpResult<()> {
        self.providers.shutdown()
    }
}

impl Drop for OtlpRuntime {
    fn drop(&mut self) {
        let _ = self.providers.shutdown();
    }
}

#[derive(Default)]
struct Providers {
    traces: Option<SdkTracerProvider>,
    metrics: Option<SdkMeterProvider>,
    logs: Option<SdkLoggerProvider>,
}

impl Providers {
    fn force_flush(&self) -> OtlpResult<()> {
        let mut errors = Vec::new();
        if let Some(provider) = &self.traces
            && let Err(error) = provider.force_flush()
        {
            errors.push(format!("traces: {error}"));
        }
        if let Some(provider) = &self.metrics
            && let Err(error) = provider.force_flush()
        {
            errors.push(format!("metrics: {error}"));
        }
        if let Some(provider) = &self.logs
            && let Err(error) = provider.force_flush()
        {
            errors.push(format!("logs: {error}"));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(OtlpError::Flush(errors.join("; ")))
        }
    }

    fn shutdown(&mut self) -> OtlpResult<()> {
        let mut errors = Vec::new();
        if let Some(provider) = self.traces.take()
            && let Err(error) = provider.shutdown()
        {
            errors.push(format!("traces: {error}"));
        }
        if let Some(provider) = self.metrics.take()
            && let Err(error) = provider.shutdown()
        {
            errors.push(format!("metrics: {error}"));
        }
        if let Some(provider) = self.logs.take()
            && let Err(error) = provider.shutdown()
        {
            errors.push(format!("logs: {error}"));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(OtlpError::Shutdown(errors.join("; ")))
        }
    }
}

fn build_resource(config: &OtlpConfig) -> Resource {
    let mut attributes = Vec::new();
    push_attribute(
        &mut attributes,
        SERVICE_NAMESPACE,
        config.resource.service_namespace.as_deref(),
    );
    push_attribute(
        &mut attributes,
        SERVICE_VERSION,
        config.resource.service_version.as_deref(),
    );
    push_attribute(
        &mut attributes,
        SERVICE_INSTANCE_ID,
        config.resource.service_instance_id.as_deref(),
    );
    push_attribute(
        &mut attributes,
        DEPLOYMENT_ENVIRONMENT_NAME,
        config.resource.deployment_environment.as_deref(),
    );

    Resource::builder()
        .with_service_name(config.resource.service_name.clone())
        .with_attributes(attributes)
        .build()
}

fn push_attribute(attributes: &mut Vec<KeyValue>, key: &'static str, value: Option<&str>) {
    if let Some(value) = value {
        attributes.push(KeyValue::new(key, value.to_owned()));
    }
}

fn exporter_error(error: impl std::fmt::Display) -> OtlpError {
    OtlpError::ExporterBuild(error.to_string())
}
