//! Default observability primitives for ddd4r applications.
//!
//! Library crates emit `tracing` spans and `metrics` measurements. The final
//! application installs this runtime once so tracing, metrics, and health are
//! available in every generated ddd4r service.

#![forbid(unsafe_code)]

mod error;
mod health;
mod metrics;
mod runtime;
mod trace;

pub use error::{ObservabilityError, ObservabilityResult};
pub use health::{ComponentHealth, HealthRegistry, HealthReport, HealthStatus};
pub use metrics::{
    BoxedMetricsRecorder, HistogramSnapshot, InMemoryRecorder, MetricSnapshot, MetricValue,
    MetricsPolicy,
};
pub use runtime::{
    BoxedObservabilityLayer, LogFormat, ObservabilityBuilder, ObservabilityConfig,
    ObservabilityRuntime,
};
pub use trace::TraceController;

/// Common imports for ddd4r observability users.
pub mod prelude {
    pub use crate::{
        ComponentHealth, HealthRegistry, HealthReport, HealthStatus, InMemoryRecorder, LogFormat,
        MetricSnapshot, MetricValue, MetricsPolicy, ObservabilityBuilder, ObservabilityConfig,
        ObservabilityError, ObservabilityResult, ObservabilityRuntime, TraceController,
    };
}
