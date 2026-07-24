//! Authorized adapter for temporary tracing filter changes.
//!
//! The observability controller remains usable by trusted in-process code.
//! Management transports must call this adapter so every remote operation is
//! permission checked, time bounded, and audited.

#![forbid(unsafe_code)]

use std::time::Duration;

use ddd4r_diagnostics::{DiagnosticError, DiagnosticGrant, DiagnosticOperation, DiagnosticResult};
use ddd4r_observability::TraceController;

/// Applies authorized, temporary tracing filter changes.
#[derive(Debug, Default)]
pub struct AuthorizedTraceController;

impl AuthorizedTraceController {
    /// Reloads the filter for a bounded period.
    pub fn reload_for(
        grant: &DiagnosticGrant,
        controller: &TraceController,
        directive: &str,
        ttl: Duration,
    ) -> DiagnosticResult<u64> {
        grant.ensure(DiagnosticOperation::TraceReload)?;
        if ttl.is_zero() || ttl > grant.duration() {
            grant.failed("trace_ttl_out_of_bounds");
            return Err(DiagnosticError::InvalidConfiguration(
                "trace reload TTL must be non-zero and no longer than the grant".to_owned(),
            ));
        }

        match controller.reload_for(directive, ttl) {
            Ok(generation) => {
                grant.completed();
                Ok(generation)
            }
            Err(error) => {
                grant.failed("trace_reload_failed");
                Err(DiagnosticError::Backend(error.to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;
    use std::time::Duration;

    use ddd4r_diagnostics::{
        DiagnosticOperation, DiagnosticPrincipal, DiagnosticRequest, DiagnosticsGate,
    };
    use ddd4r_observability::{ObservabilityBuilder, ObservabilityRuntime};

    use super::*;

    fn grant(operation: DiagnosticOperation) -> DiagnosticGrant {
        DiagnosticsGate::default()
            .authorize(&DiagnosticRequest {
                principal: DiagnosticPrincipal::new("operator", [operation.permission()]),
                operation,
                peer_ip: "127.0.0.1".parse().expect("valid loopback IP"),
                requested_duration: Duration::from_millis(100),
            })
            .expect("authorized diagnostic request")
    }

    fn runtime() -> &'static ObservabilityRuntime {
        static RUNTIME: OnceLock<ObservabilityRuntime> = OnceLock::new();
        RUNTIME.get_or_init(|| {
            ObservabilityBuilder::default()
                .install()
                .expect("install observability")
        })
    }

    #[tokio::test]
    async fn authorized_reload_is_bounded_and_restored() {
        let trace_grant = grant(DiagnosticOperation::TraceReload);
        AuthorizedTraceController::reload_for(
            &trace_grant,
            runtime().trace_controller(),
            "debug",
            Duration::from_millis(10),
        )
        .expect("authorized reload");

        assert_eq!(runtime().trace_controller().current_filter(), "debug");
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(runtime().trace_controller().current_filter(), "info");
    }

    #[test]
    fn wrong_operation_and_oversized_ttl_are_rejected() {
        let wrong_grant = grant(DiagnosticOperation::CpuProfile);
        assert!(matches!(
            AuthorizedTraceController::reload_for(
                &wrong_grant,
                runtime().trace_controller(),
                "debug",
                Duration::from_millis(10)
            ),
            Err(DiagnosticError::OperationMismatch { .. })
        ));

        let trace_grant = grant(DiagnosticOperation::TraceReload);
        assert!(matches!(
            AuthorizedTraceController::reload_for(
                &trace_grant,
                runtime().trace_controller(),
                "debug",
                Duration::from_millis(101)
            ),
            Err(DiagnosticError::InvalidConfiguration(_))
        ));
    }
}
