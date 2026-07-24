//! Bounded OTLP and resource configuration.

use std::net::IpAddr;
use std::time::Duration;

use ddd4r_observability::MetricsPolicy;
use url::Url;

use crate::{OtlpError, OtlpResult};

/// Signals sent by the explicit OTLP adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OtlpSignals {
    /// Export distributed traces.
    pub traces: bool,
    /// Export application metrics.
    pub metrics: bool,
    /// Export structured tracing events as OpenTelemetry logs.
    pub logs: bool,
}

impl Default for OtlpSignals {
    fn default() -> Self {
        Self {
            traces: true,
            metrics: true,
            logs: false,
        }
    }
}

/// Resource identity attached to every exported signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtlpResource {
    /// Required OpenTelemetry `service.name`.
    pub service_name: String,
    /// Optional `service.namespace`.
    pub service_namespace: Option<String>,
    /// Optional `service.version`.
    pub service_version: Option<String>,
    /// Optional `service.instance.id`.
    pub service_instance_id: Option<String>,
    /// Optional `deployment.environment.name`.
    pub deployment_environment: Option<String>,
}

impl Default for OtlpResource {
    fn default() -> Self {
        Self {
            service_name: "ddd4r-application".to_owned(),
            service_namespace: None,
            service_version: None,
            service_instance_id: None,
            deployment_environment: None,
        }
    }
}

/// Explicit OTLP/HTTP protobuf exporter configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct OtlpConfig {
    /// Collector base URL. Signal paths are added by the adapter.
    pub endpoint: String,
    /// Signals to export.
    pub signals: OtlpSignals,
    /// Common resource identity.
    pub resource: OtlpResource,
    /// Per-request export timeout.
    pub export_timeout: Duration,
    /// Periodic metric export interval.
    pub metric_export_interval: Duration,
    /// Bounds applied independently to remotely exported metric series.
    pub metrics_policy: MetricsPolicy,
    /// Parent-based root trace sampling ratio from 0.0 to 1.0.
    pub trace_sample_ratio: f64,
    /// Maximum queued spans before the SDK starts dropping them.
    pub trace_max_queue_size: usize,
    /// Maximum spans in one export request.
    pub trace_max_export_batch_size: usize,
    /// Maximum delay before a partial trace batch is exported.
    pub trace_scheduled_delay: Duration,
    /// Maximum attributes retained on one span.
    pub trace_max_attributes_per_span: u32,
    /// Maximum events retained on one span.
    pub trace_max_events_per_span: u32,
}

impl Default for OtlpConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:4318".to_owned(),
            signals: OtlpSignals::default(),
            resource: OtlpResource::default(),
            export_timeout: Duration::from_secs(5),
            metric_export_interval: Duration::from_secs(30),
            metrics_policy: MetricsPolicy::default(),
            trace_sample_ratio: 1.0,
            trace_max_queue_size: 2_048,
            trace_max_export_batch_size: 512,
            trace_scheduled_delay: Duration::from_secs(5),
            trace_max_attributes_per_span: 128,
            trace_max_events_per_span: 128,
        }
    }
}

impl OtlpConfig {
    pub(crate) fn validate(&self) -> OtlpResult<Url> {
        if !self.signals.traces && !self.signals.metrics && !self.signals.logs {
            return Err(invalid("at least one OTLP signal must be enabled"));
        }
        validate_resource(&self.resource)?;
        if self.export_timeout.is_zero() || self.export_timeout > Duration::from_secs(30) {
            return Err(invalid("export_timeout must be within 1ns..=30s"));
        }
        if self.metric_export_interval < Duration::from_secs(1)
            || self.metric_export_interval > Duration::from_mins(5)
        {
            return Err(invalid("metric_export_interval must be within 1s..=300s"));
        }
        if self.metrics_policy.max_series == 0
            || self.metrics_policy.max_labels_per_series == 0
            || self.metrics_policy.max_field_length == 0
        {
            return Err(invalid("metrics policy limits must be non-zero"));
        }
        if !self.trace_sample_ratio.is_finite() || !(0.0..=1.0).contains(&self.trace_sample_ratio) {
            return Err(invalid("trace_sample_ratio must be within 0.0..=1.0"));
        }
        if self.trace_max_queue_size == 0 || self.trace_max_queue_size > 65_536 {
            return Err(invalid("trace_max_queue_size must be within 1..=65536"));
        }
        if self.trace_max_export_batch_size == 0
            || self.trace_max_export_batch_size > self.trace_max_queue_size
        {
            return Err(invalid(
                "trace_max_export_batch_size must be non-zero and no larger than the queue",
            ));
        }
        if self.trace_scheduled_delay.is_zero()
            || self.trace_scheduled_delay > Duration::from_mins(1)
        {
            return Err(invalid("trace_scheduled_delay must be within 1ns..=60s"));
        }
        if self.trace_max_attributes_per_span == 0 || self.trace_max_attributes_per_span > 1_024 {
            return Err(invalid(
                "trace_max_attributes_per_span must be within 1..=1024",
            ));
        }
        if self.trace_max_events_per_span == 0 || self.trace_max_events_per_span > 1_024 {
            return Err(invalid("trace_max_events_per_span must be within 1..=1024"));
        }
        validate_endpoint(&self.endpoint)
    }

    pub(crate) fn signal_endpoint(base: &Url, signal: &str) -> String {
        let mut endpoint = base.clone();
        endpoint.set_path(&format!("/v1/{signal}"));
        endpoint.to_string()
    }
}

fn validate_resource(resource: &OtlpResource) -> OtlpResult<()> {
    validate_resource_value("service_name", Some(&resource.service_name), true)?;
    validate_resource_value(
        "service_namespace",
        resource.service_namespace.as_deref(),
        false,
    )?;
    validate_resource_value(
        "service_version",
        resource.service_version.as_deref(),
        false,
    )?;
    validate_resource_value(
        "service_instance_id",
        resource.service_instance_id.as_deref(),
        false,
    )?;
    validate_resource_value(
        "deployment_environment",
        resource.deployment_environment.as_deref(),
        false,
    )
}

fn validate_resource_value(name: &str, value: Option<&str>, required: bool) -> OtlpResult<()> {
    match value {
        Some(value)
            if !value.trim().is_empty()
                && value.len() <= 256
                && !value.chars().any(char::is_control) =>
        {
            Ok(())
        }
        None if !required => Ok(()),
        _ => Err(invalid(format!(
            "{name} must be non-blank, control-character-free, and at most 256 bytes"
        ))),
    }
}

fn validate_endpoint(endpoint: &str) -> OtlpResult<Url> {
    let url =
        Url::parse(endpoint).map_err(|error| invalid(format!("invalid endpoint: {error}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(invalid("endpoint scheme must be http or https"));
    }
    if url.cannot_be_a_base() || url.host_str().is_none() {
        return Err(invalid("endpoint must be an absolute collector base URL"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid(
            "credentials must not be embedded in the endpoint URL",
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(invalid("endpoint must not contain a query or fragment"));
    }
    if !matches!(url.path(), "" | "/") {
        return Err(invalid(
            "endpoint must be a base URL without a signal-specific path",
        ));
    }
    if url.scheme() == "http" && !is_loopback_host(&url) {
        return Err(invalid(
            "non-loopback OTLP endpoints must use HTTPS transport",
        ));
    }
    Ok(url)
}

fn is_loopback_host(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn invalid(message: impl Into<String>) -> OtlpError {
    OtlpError::InvalidConfiguration(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_collector_is_loopback_and_signal_paths_are_explicit() {
        let config = OtlpConfig::default();
        let base = config.validate().expect("safe default");

        assert_eq!(
            OtlpConfig::signal_endpoint(&base, "traces"),
            "http://127.0.0.1:4318/v1/traces"
        );
    }

    #[test]
    fn rejects_insecure_remote_or_credential_bearing_endpoints() {
        for endpoint in [
            "http://collector.example.com:4318",
            "https://user:secret@collector.example.com:4318",
            "file:///tmp/collector",
        ] {
            let config = OtlpConfig {
                endpoint: endpoint.to_owned(),
                ..OtlpConfig::default()
            };
            assert!(config.validate().is_err(), "{endpoint}");
        }
    }

    #[test]
    fn rejects_unbounded_signal_configuration() {
        let config = OtlpConfig {
            trace_max_export_batch_size: 2_049,
            ..OtlpConfig::default()
        };
        assert!(config.validate().is_err());
    }
}
