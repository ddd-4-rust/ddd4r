//! Stable authentication errors.

use thiserror::Error;

/// Result type returned by authentication APIs.
pub type AuthResult<T> = Result<T, AuthError>;

/// Framework-neutral and fail-closed authentication error model.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// A login request did not contain a usable identifier.
    #[error("loginId must not be blank")]
    InvalidLoginId,
    /// An account is disabled.
    #[error("account is disabled: {login_id}")]
    AccountDisabled {
        /// Stable account identifier.
        login_id: String,
    },
    /// A credential was absent or could not be verified.
    #[error("invalid or revoked token")]
    InvalidToken,
    /// A session exceeded its absolute or idle timeout.
    #[error("session expired")]
    SessionExpired,
    /// The operation requires an authenticated task scope.
    #[error("subject is not authenticated")]
    NotAuthenticated,
    /// A concurrent-login policy rejected the new session.
    #[error("concurrent login rejected for {login_id}")]
    ConcurrentLoginRejected {
        /// Stable account identifier.
        login_id: String,
    },
    /// A caller does not have the requested authority.
    #[error("access denied: {authority}")]
    AccessDenied {
        /// Permission or role that was required.
        authority: String,
    },
    /// A runtime subject provider was not registered.
    #[error("SubjectProvider not registered")]
    ProviderNotRegistered,
    /// A token generator returned a duplicate or blank token.
    #[error("invalid generated token")]
    InvalidGeneratedToken,
    /// Session storage failed without exposing sensitive state.
    #[error("session store failed: {message}")]
    Store {
        /// Sanitized storage error.
        message: String,
    },
    /// A framework registry operation failed.
    #[error("authentication infrastructure failed: {message}")]
    Infrastructure {
        /// Sanitized infrastructure error.
        message: String,
    },
}

impl From<ddd4r_core::DddError> for AuthError {
    fn from(value: ddd4r_core::DddError) -> Self {
        Self::Infrastructure {
            message: value.to_string(),
        }
    }
}
