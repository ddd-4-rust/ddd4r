//! Framework-neutral equivalent of the Spring Security exception handler.

use ddd4r_auth::AuthError;
use serde::Serialize;

/// Stable HTTP error payload consumed by concrete web adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SecurityErrorResponse {
    /// HTTP status code.
    pub status: u16,
    /// Stable machine-readable code.
    pub code: &'static str,
    /// Safe user-facing message.
    pub message: &'static str,
}

/// Maps authentication errors without leaking credentials or internal causes.
pub struct SecurityExceptionHandler;

impl SecurityExceptionHandler {
    /// Maps a framework-neutral authentication error to HTTP semantics.
    pub const fn map(error: &AuthError) -> SecurityErrorResponse {
        match error {
            AuthError::InvalidLoginId
            | AuthError::UnknownAccount
            | AuthError::BadCredentials
            | AuthError::InvalidToken
            | AuthError::SessionExpired
            | AuthError::NotAuthenticated
            | AuthError::AccountExpired { .. }
            | AuthError::CredentialsExpired { .. } => SecurityErrorResponse {
                status: 401,
                code: "unauthenticated",
                message: "未登录或登录已过期",
            },
            AuthError::AccountLocked { .. } | AuthError::AccountDisabled { .. } => {
                SecurityErrorResponse {
                    status: 403,
                    code: "account_unavailable",
                    message: "账号已被锁定或禁用",
                }
            }
            AuthError::AccessDenied { .. } | AuthError::ConcurrentLoginRejected { .. } => {
                SecurityErrorResponse {
                    status: 403,
                    code: "access_denied",
                    message: "无权限访问",
                }
            }
            AuthError::ProviderNotRegistered
            | AuthError::InvalidGeneratedToken
            | AuthError::Store { .. }
            | AuthError::Infrastructure { .. } => SecurityErrorResponse {
                status: 500,
                code: "authentication_infrastructure_failure",
                message: "认证服务暂不可用",
            },
        }
    }

    /// Preserves Spring's 401-vs-403 distinction for access-denied callbacks.
    pub const fn access_denied(authenticated: bool) -> SecurityErrorResponse {
        SecurityErrorResponse {
            status: if authenticated { 403 } else { 401 },
            code: "access_denied",
            message: "无权限访问",
        }
    }
}
