//! Explicit Tokio Console integration.
//!
//! Compile with feature `runtime` and `RUSTFLAGS="--cfg tokio_unstable"`.
//! The server always binds to a loopback address because the Tokio Console
//! protocol does not provide application-level authorization.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::time::Duration;

use ddd4r_diagnostics::{DiagnosticError, DiagnosticGrant, DiagnosticOperation, DiagnosticResult};
use ddd4r_observability::BoxedObservabilityLayer;

/// Safe Tokio Console server configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokioConsoleConfig {
    /// Loopback management address.
    pub bind: SocketAddr,
    /// Maximum retained task history.
    pub retention: Duration,
}

impl Default for TokioConsoleConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([127, 0, 0, 1], 6_669)),
            retention: Duration::from_mins(1),
        }
    }
}

/// Builds an optional Tokio Console tracing layer.
#[derive(Debug, Default)]
pub struct TokioConsoleDiagnostics;

impl TokioConsoleDiagnostics {
    /// Whether the runtime implementation was explicitly compiled.
    pub const fn compiled() -> bool {
        cfg!(feature = "runtime")
    }

    /// Builds a layer consumed by `ObservabilityBuilder::with_layer`.
    #[cfg_attr(not(feature = "runtime"), allow(clippy::needless_pass_by_value))]
    pub fn layer(
        grant: DiagnosticGrant,
        config: TokioConsoleConfig,
    ) -> DiagnosticResult<BoxedObservabilityLayer> {
        grant.ensure(DiagnosticOperation::AsyncTasks)?;
        if !config.bind.ip().is_loopback() {
            grant.failed("console_non_loopback_rejected");
            return Err(DiagnosticError::LoopbackRequired);
        }
        if config.retention.is_zero() || config.retention > grant.duration() {
            grant.failed("console_retention_out_of_bounds");
            return Err(DiagnosticError::InvalidConfiguration(
                "Tokio Console retention must be non-zero and no longer than the grant".to_owned(),
            ));
        }

        #[cfg(feature = "runtime")]
        {
            let runtime = tokio::runtime::Handle::try_current().map_err(|error| {
                grant.failed("tokio_runtime_unavailable");
                DiagnosticError::Unavailable(error.to_string())
            })?;
            let (layer, server) = console_subscriber::ConsoleLayer::builder()
                .server_addr(config.bind)
                .retention(config.retention)
                .build();
            runtime.spawn(async move {
                match tokio::time::timeout(grant.duration(), server.serve()).await {
                    Err(_) | Ok(Ok(())) => grant.completed(),
                    Ok(Err(_)) => grant.failed("console_server_failed"),
                }
            });
            Ok(Box::new(layer))
        }

        #[cfg(not(feature = "runtime"))]
        {
            grant.failed("console_not_compiled");
            Err(DiagnosticError::Unavailable(
                "compile ddd4r-diagnostics-tokio-console with feature `runtime` and tokio_unstable"
                    .to_owned(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ddd4r_diagnostics::{
        DiagnosticOperation, DiagnosticPrincipal, DiagnosticRequest, DiagnosticsGate,
    };

    use super::*;

    fn grant() -> DiagnosticGrant {
        DiagnosticsGate::default()
            .authorize(&DiagnosticRequest {
                principal: DiagnosticPrincipal::new(
                    "operator",
                    [DiagnosticOperation::AsyncTasks.permission()],
                ),
                operation: DiagnosticOperation::AsyncTasks,
                peer_ip: "127.0.0.1".parse().expect("valid IP"),
                requested_duration: Duration::from_mins(2),
            })
            .expect("authorized")
    }

    #[test]
    fn public_bind_is_rejected_even_after_authorization() {
        let config = TokioConsoleConfig {
            bind: SocketAddr::from(([0, 0, 0, 0], 6_669)),
            retention: Duration::from_mins(1),
        };
        assert!(matches!(
            TokioConsoleDiagnostics::layer(grant(), config),
            Err(DiagnosticError::LoopbackRequired)
        ));
    }

    #[cfg(not(feature = "runtime"))]
    #[test]
    fn runtime_is_not_compiled_by_default() {
        assert!(!TokioConsoleDiagnostics::compiled());
        assert!(matches!(
            TokioConsoleDiagnostics::layer(grant(), TokioConsoleConfig::default()),
            Err(DiagnosticError::Unavailable(_))
        ));
    }

    #[cfg(feature = "runtime")]
    #[tokio::test]
    async fn explicitly_compiled_loopback_layer_is_created() {
        let config = TokioConsoleConfig {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            retention: Duration::from_mins(1),
        };

        let layer = TokioConsoleDiagnostics::layer(grant(), config)
            .expect("authorized loopback console layer");
        drop(layer);
        assert!(TokioConsoleDiagnostics::compiled());
    }
}
