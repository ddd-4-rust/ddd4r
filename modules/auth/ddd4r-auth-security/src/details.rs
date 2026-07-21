//! Security user details carrying a framework-neutral principal.

use std::collections::BTreeSet;

use ddd4r_auth::AuthPrincipal;
use serde::Serialize;
use zeroize::Zeroizing;

/// User-details bridge for credential providers and web adapters.
#[derive(Clone, PartialEq, Serialize)]
pub struct AuthUserDetails {
    username: String,
    #[serde(skip)]
    credential_hash: Zeroizing<String>,
    enabled: bool,
    authorities: BTreeSet<String>,
    principal: AuthPrincipal,
}

impl std::fmt::Debug for AuthUserDetails {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthUserDetails")
            .field("username", &self.username)
            .field("credential_hash", &"[REDACTED]")
            .field("enabled", &self.enabled)
            .field("authorities", &self.authorities)
            .field("principal", &self.principal)
            .finish()
    }
}

impl AuthUserDetails {
    /// Creates user details. `credential_hash` must already be password-hashed.
    pub fn new(
        username: impl Into<String>,
        credential_hash: impl Into<String>,
        enabled: bool,
        authorities: impl IntoIterator<Item = String>,
        principal: AuthPrincipal,
    ) -> Self {
        Self {
            username: username.into(),
            credential_hash: Zeroizing::new(credential_hash.into()),
            enabled,
            authorities: authorities.into_iter().collect(),
            principal,
        }
    }

    /// Returns the username used by a credential provider.
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Returns whether authentication is enabled.
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Returns immutable authorities.
    pub const fn authorities(&self) -> &BTreeSet<String> {
        &self.authorities
    }

    /// Returns the framework-neutral principal.
    pub const fn auth_principal(&self) -> &AuthPrincipal {
        &self.principal
    }

    /// Delegates credential verification without exposing hashes through Debug or serialization.
    pub fn verify_credential<F>(&self, presented: &str, verifier: F) -> bool
    where
        F: FnOnce(&str, &str) -> bool,
    {
        verifier(presented, &self.credential_hash)
    }
}
