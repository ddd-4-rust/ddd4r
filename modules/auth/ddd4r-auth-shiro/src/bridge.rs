//! Credential Realm and `SessionDAO` bridges for Shiro migration.

use std::sync::Arc;

use ddd4r_auth::{AuthId, AuthPrincipal, AuthResult, SessionRecord, SessionStore};
use futures::future::BoxFuture;
use zeroize::Zeroizing;

/// Credential submitted to a Shiro-style Realm.
#[derive(Clone, PartialEq, Eq)]
pub struct ShiroAuthenticationToken {
    login_id: AuthId,
    credential: Zeroizing<String>,
    realm: Option<String>,
    remember_me: bool,
}

impl std::fmt::Debug for ShiroAuthenticationToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ShiroAuthenticationToken")
            .field("login_id", &self.login_id)
            .field("credential", &"[REDACTED]")
            .field("realm", &self.realm)
            .field("remember_me", &self.remember_me)
            .finish()
    }
}

impl ShiroAuthenticationToken {
    /// Creates a credential token.
    pub fn new(
        login_id: AuthId,
        credential: impl Into<String>,
        realm: Option<String>,
        remember_me: bool,
    ) -> Self {
        Self {
            login_id,
            credential: Zeroizing::new(credential.into()),
            realm,
            remember_me,
        }
    }

    /// Returns the account identifier.
    pub const fn login_id(&self) -> &AuthId {
        &self.login_id
    }

    /// Returns the selected realm.
    pub fn realm(&self) -> Option<&str> {
        self.realm.as_deref()
    }

    /// Returns whether remember-me was requested.
    pub const fn remember_me(&self) -> bool {
        self.remember_me
    }

    /// Delegates verification while keeping the credential out of Debug output.
    pub fn verify_credential<F>(&self, expected_hash: &str, verifier: F) -> bool
    where
        F: FnOnce(&str, &str) -> bool,
    {
        verifier(&self.credential, expected_hash)
    }
}

/// Asynchronous equivalent of a Shiro Realm authentication boundary.
pub trait ShiroRealmAuthenticator: Send + Sync {
    /// Authenticates a credential and returns a complete ddd4r principal.
    fn authenticate<'a>(
        &'a self,
        token: &'a ShiroAuthenticationToken,
    ) -> BoxFuture<'a, AuthResult<AuthPrincipal>>;
}

/// Shiro-style `SessionDAO` facade over the common `SessionStore` SPI.
#[derive(Clone)]
pub struct ShiroSessionDao {
    store: Arc<dyn SessionStore>,
}

impl std::fmt::Debug for ShiroSessionDao {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ShiroSessionDao")
            .finish_non_exhaustive()
    }
}

impl ShiroSessionDao {
    /// Creates a DAO over the same store used by `SubjectEngine`.
    pub const fn new(store: Arc<dyn SessionStore>) -> Self {
        Self { store }
    }

    /// Reads and touches a live session.
    pub async fn read(&self, token: &str) -> AuthResult<Option<SessionRecord>> {
        self.store.find_by_token(token).await
    }

    /// Reads the newest live session for an account.
    pub async fn read_by_login_id(&self, login_id: &AuthId) -> AuthResult<Option<SessionRecord>> {
        self.store.find_by_login_id(login_id).await
    }

    /// Deletes one session.
    pub async fn delete(&self, token: &str) -> AuthResult<Option<SessionRecord>> {
        self.store.revoke_token(token).await
    }

    /// Deletes every session for an account.
    pub async fn delete_by_login_id(&self, login_id: &AuthId) -> AuthResult<Vec<SessionRecord>> {
        self.store.revoke_login(login_id).await
    }
}
