//! Process-wide observability installation.

use std::sync::Arc;

use tracing_subscriber::layer::{Layer, SubscriberExt};
use tracing_subscriber::registry::Registry;
use tracing_subscriber::reload;
use tracing_subscriber::util::SubscriberInitExt;

use crate::metrics::FanoutRecorder;
use crate::trace::parse_filter;
use crate::{
    BoxedMetricsRecorder, HealthRegistry, InMemoryRecorder, ObservabilityError,
    ObservabilityResult, TraceController,
};

/// A dynamically composed tracing layer accepted from optional adapters.
pub type BoxedObservabilityLayer = Box<dyn Layer<Registry> + Send + Sync + 'static>;

/// Structured log format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogFormat {
    /// Human-readable compact logs.
    #[default]
    Compact,
    /// JSON logs suitable for production collectors.
    Json,
}

/// Default observability configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservabilityConfig {
    /// Stable service name used by exporters.
    pub service_name: String,
    /// Initial `tracing_subscriber::EnvFilter` directive.
    pub filter: String,
    /// Log rendering format.
    pub log_format: LogFormat,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            service_name: "ddd4r-application".to_owned(),
            filter: "info".to_owned(),
            log_format: LogFormat::Compact,
        }
    }
}

/// Builder for the process-wide observability runtime.
pub struct ObservabilityBuilder {
    config: ObservabilityConfig,
    health: Arc<HealthRegistry>,
    metrics: Arc<InMemoryRecorder>,
    metrics_exporter: Option<BoxedMetricsRecorder>,
    extra_layers: Vec<BoxedObservabilityLayer>,
}

impl std::fmt::Debug for ObservabilityBuilder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ObservabilityBuilder")
            .field("config", &self.config)
            .field("extra_layer_count", &self.extra_layers.len())
            .finish_non_exhaustive()
    }
}

impl Default for ObservabilityBuilder {
    fn default() -> Self {
        Self {
            config: ObservabilityConfig::default(),
            health: Arc::new(HealthRegistry::default()),
            metrics: Arc::new(InMemoryRecorder::default()),
            metrics_exporter: None,
            extra_layers: Vec::new(),
        }
    }
}

impl ObservabilityBuilder {
    /// Starts with the supplied configuration.
    pub fn new(config: ObservabilityConfig) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }

    /// Sets the stable service identity shared by local and remote adapters.
    pub fn service_name(mut self, service_name: impl Into<String>) -> Self {
        self.config.service_name = service_name.into();
        self
    }

    /// Uses a caller-owned health registry.
    pub fn health_registry(mut self, health: Arc<HealthRegistry>) -> Self {
        self.health = health;
        self
    }

    /// Uses a caller-owned default metrics recorder.
    pub fn metrics_recorder(mut self, metrics: Arc<InMemoryRecorder>) -> Self {
        self.metrics = metrics;
        self
    }

    /// Adds an explicitly compiled metrics exporter while retaining the
    /// default in-process recorder for health endpoints and tests.
    pub fn with_metrics_exporter(mut self, exporter: BoxedMetricsRecorder) -> Self {
        self.metrics_exporter = Some(exporter);
        self
    }

    /// Adds an explicitly compiled tracing layer such as Tokio Console.
    pub fn with_layer(mut self, layer: BoxedObservabilityLayer) -> Self {
        self.extra_layers.push(layer);
        self
    }

    /// Installs tracing and metrics exactly once for the process.
    pub fn install(self) -> ObservabilityResult<ObservabilityRuntime> {
        let filter = parse_filter(&self.config.filter)?;
        let (filter_layer, reload_handle) = reload::Layer::new(filter);
        let mut layers: Vec<BoxedObservabilityLayer> = vec![Box::new(filter_layer)];
        match self.config.log_format {
            LogFormat::Compact => layers.push(Box::new(tracing_subscriber::fmt::layer().compact())),
            LogFormat::Json => layers.push(Box::new(tracing_subscriber::fmt::layer().json())),
        }
        layers.extend(self.extra_layers);

        tracing_subscriber::registry()
            .with(layers)
            .try_init()
            .map_err(|_| ObservabilityError::TracingAlreadyInstalled)?;

        let global_metrics: BoxedMetricsRecorder = self.metrics_exporter.map_or_else(
            || self.metrics.clone() as BoxedMetricsRecorder,
            |exporter| {
                Arc::new(FanoutRecorder::new(self.metrics.clone(), exporter))
                    as BoxedMetricsRecorder
            },
        );
        metrics::set_global_recorder(global_metrics)
            .map_err(|_| ObservabilityError::MetricsAlreadyInstalled)?;

        metrics::describe_gauge!(
            "ddd4r_health_ready",
            "Whether the ddd4r runtime is ready to receive traffic"
        );
        metrics::gauge!("ddd4r_health_ready").set(1.0);

        Ok(ObservabilityRuntime {
            service_name: self.config.service_name,
            health: self.health,
            metrics: self.metrics,
            trace: TraceController::new(reload_handle, self.config.filter),
        })
    }
}

/// Installed observability runtime shared with health and management adapters.
#[derive(Debug, Clone)]
pub struct ObservabilityRuntime {
    service_name: String,
    health: Arc<HealthRegistry>,
    metrics: Arc<InMemoryRecorder>,
    trace: TraceController,
}

impl ObservabilityRuntime {
    /// Installs the default compact `info` runtime.
    pub fn install_default() -> ObservabilityResult<Self> {
        ObservabilityBuilder::default().install()
    }

    /// Stable service name.
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// Health registry for liveness/readiness adapters.
    pub fn health(&self) -> &Arc<HealthRegistry> {
        &self.health
    }

    /// Default in-process metric recorder.
    pub fn metrics(&self) -> &Arc<InMemoryRecorder> {
        &self.metrics
    }

    /// Authorized management adapters use this controller for bounded reloads.
    pub fn trace_controller(&self) -> &TraceController {
        &self.trace
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_enable_all_three_observability_primitives() {
        let runtime = ObservabilityBuilder::default()
            .install()
            .expect("first process-wide install succeeds");

        assert_eq!(runtime.trace_controller().current_filter(), "info");
        assert!(runtime.health().report().ready);
        let metrics = runtime.metrics().snapshot();
        assert!(
            metrics
                .iter()
                .any(|metric| metric.key.starts_with("ddd4r_health_ready")),
            "default metrics snapshot: {metrics:?}"
        );
    }
}
