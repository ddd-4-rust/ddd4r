//! Explicit OpenTelemetry OTLP export for ddd4r.
//!
//! `ddd4r-observability` remains the default local runtime. Applications that
//! deliberately add this crate can fan out traces, metrics, and structured
//! logs to an OpenTelemetry Collector while retaining local health and metric
//! snapshots.
//!
//! The adapter follows the OpenTelemetry Rust lifecycle contract: keep the
//! returned [`OtlpRuntime`] alive for the application lifetime and call
//! [`OtlpRuntime::shutdown`] during graceful termination.

#![forbid(unsafe_code)]

mod config;
mod error;
mod metrics;
mod propagation;
mod runtime;

pub use config::{OtlpConfig, OtlpResource, OtlpSignals};
pub use error::{OtlpError, OtlpResult};
pub use metrics::OpenTelemetryMetricsRecorder;
pub use propagation::W3cPropagation;
pub use runtime::OtlpRuntime;

/// Common imports for explicit OTLP export.
pub mod prelude {
    pub use crate::{
        OpenTelemetryMetricsRecorder, OtlpConfig, OtlpError, OtlpResource, OtlpResult, OtlpRuntime,
        OtlpSignals, W3cPropagation,
    };
}
