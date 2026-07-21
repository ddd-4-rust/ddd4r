//! Subject engine, RBAC policies and transport authentication.

use std::collections::BTreeSet;
use std::sync::Arc;

use futures::future::BoxFuture;
use serde_json::Value;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::{
    AuthError, AuthEvent, AuthId, AuthLogoutMode, AuthPrincipal, AuthRequest, AuthResult,
    InMemorySessionStore, SessionRecord, SessionStore, SubjectScope,
};

/// Loads authorization data that is not embedded in a principal.
pub trait SubjectDataProvider: Send + Sync {
    /// Returns permission markers for a principal.
    fn permissions<'a>(
        &'a self,
        principal: &'a AuthPrincipal,
    ) -> BoxFuture<'a, AuthResult<BTreeSet<String>>>;

    /// Returns role identifiers for a principal.
    fn roles<'a>(
        &'a self,
        principal: &'a AuthPrincipal,
    ) -> BoxFuture<'a, AuthResult<BTreeSet<String>>>;

    /// Applies a business-owned account disable policy.
    fn is_disabled<'a>(&'a self, _login_id: &'a AuthId) -> BoxFuture<'a, AuthResult<bool>> {
        Box::pin(async { Ok(false) })
    }
}

/// Uses authorization data already carried by `AuthPrincipal`.
#[derive(Debug, Default)]
pub struct DefaultSubjectDataProvider;

impl SubjectDataProvider for DefaultSubjectDataProvider {
    fn permissions<'a>(
        &'a self,
        principal: &'a AuthPrincipal,
    ) -> BoxFuture<'a, AuthResult<BTreeSet<String>>> {
        Box::pin(async move { Ok(principal.permissions.clone()) })
    }

    fn roles<'a>(
        &'a self,
        principal: &'a AuthPrincipal,
    ) -> BoxFuture<'a, AuthResult<BTreeSet<String>>> {
        Box::pin(async move { Ok(principal.role_identifiers()) })
    }
}

/// Customizes token creation, authority matching and expiry checks.
pub trait SubjectStrategy: Send + Sync {
    /// Creates an opaque token for a validated request.
    fn create_token(&self, request: &AuthRequest) -> AuthResult<String>;

    /// Matches one authority. Implementations may add wildcard semantics explicitly.
    fn has_element(&self, elements: &BTreeSet<String>, expected: &str) -> bool {
        !expected.trim().is_empty() && elements.contains(expected)
    }

    /// Returns whether a session is expired according to its policy.
    fn is_expired(&self, session: &SessionRecord, now: OffsetDateTime) -> bool {
        let absolute_expired = session.config.timeout_seconds >= 0
            && now >= session.created_at + Duration::seconds(session.config.timeout_seconds);
        let idle_expired = session
            .config
            .active_timeout_seconds
            .is_some_and(|seconds| {
                seconds >= 0 && now >= session.last_seen_at + Duration::seconds(seconds)
            });
        absolute_expired || idle_expired
    }
}

/// UUIDv7-based opaque-token strategy.
#[derive(Debug, Default)]
pub struct DefaultSubjectStrategy;

impl SubjectStrategy for DefaultSubjectStrategy {
    fn create_token(&self, request: &AuthRequest) -> AuthResult<String> {
        let realm = request
            .realm
            .as_deref()
            .filter(|realm| !realm.trim().is_empty())
            .unwrap_or("default");
        Ok(format!("{realm}:{}", Uuid::now_v7()))
    }
}

/// Receives auditable authentication lifecycle events.
pub trait AuthEventPublisher: Send + Sync {
    /// Publishes one event. Implementations must redact credentials in external sinks.
    fn publish(&self, event: AuthEvent) -> BoxFuture<'_, AuthResult<()>>;
}

/// Event publisher used when an application has not configured an audit sink.
#[derive(Debug, Default)]
pub struct NoopAuthEventPublisher;

impl AuthEventPublisher for NoopAuthEventPublisher {
    fn publish(&self, _event: AuthEvent) -> BoxFuture<'_, AuthResult<()>> {
        Box::pin(async { Ok(()) })
    }
}

/// Shared authentication state machine used by all compatibility adapters.
#[derive(Clone)]
pub struct SubjectEngine {
    store: Arc<dyn SessionStore>,
    data_provider: Arc<dyn SubjectDataProvider>,
    strategy: Arc<dyn SubjectStrategy>,
    events: Arc<dyn AuthEventPublisher>,
}

impl std::fmt::Debug for SubjectEngine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SubjectEngine")
            .finish_non_exhaustive()
    }
}

impl Default for SubjectEngine {
    fn default() -> Self {
        Self::new(Arc::new(InMemorySessionStore::new()))
    }
}

impl SubjectEngine {
    /// Creates an engine over a replaceable session store.
    pub fn new(store: Arc<dyn SessionStore>) -> Self {
        Self {
            store,
            data_provider: Arc::new(DefaultSubjectDataProvider),
            strategy: Arc::new(DefaultSubjectStrategy),
            events: Arc::new(NoopAuthEventPublisher),
        }
    }

    /// Replaces the business authorization data provider.
    pub fn with_data_provider(mut self, provider: Arc<dyn SubjectDataProvider>) -> Self {
        self.data_provider = provider;
        self
    }

    /// Replaces token and authority matching strategy.
    pub fn with_strategy(mut self, strategy: Arc<dyn SubjectStrategy>) -> Self {
        self.strategy = strategy;
        self
    }

    /// Attaches an authentication audit event sink.
    pub fn with_event_publisher(mut self, publisher: Arc<dyn AuthEventPublisher>) -> Self {
        self.events = publisher;
        self
    }

    async fn account_disabled(&self, login_id: &AuthId) -> AuthResult<bool> {
        if self.store.is_disabled(login_id).await? {
            return Ok(true);
        }
        self.data_provider.is_disabled(login_id).await
    }

    async fn publish_best_effort(&self, event: AuthEvent) {
        let _ignored = self.events.publish(event).await;
    }

    async fn login_request(&self, mut request: AuthRequest) -> AuthResult<String> {
        let result = self.login_validated(&mut request).await;
        if let Err(error) = &result {
            self.publish_best_effort(AuthEvent::LoginFailed {
                login_id: request.login_id.clone(),
                reason: error_code(error).to_owned(),
                occurred_at: OffsetDateTime::now_utc(),
            })
            .await;
        }
        result
    }

    async fn login_validated(&self, request: &mut AuthRequest) -> AuthResult<String> {
        let login_id = request
            .login_id
            .as_ref()
            .filter(|identifier| !identifier.as_str().trim().is_empty())
            .cloned()
            .ok_or(AuthError::InvalidLoginId)?;
        if self.account_disabled(&login_id).await? {
            return Err(AuthError::AccountDisabled {
                login_id: login_id.to_string(),
            });
        }

        let mut principal = request
            .principal
            .take()
            .unwrap_or_else(|| AuthPrincipal::for_login(login_id.clone()));
        principal.login_id.get_or_insert_with(|| login_id.clone());
        principal.user_id.get_or_insert_with(|| login_id.clone());
        if principal.device_type.is_none() {
            principal
                .device_type
                .clone_from(&request.session.device_type);
        }
        if principal.device_id.is_none() {
            principal.device_id.clone_from(&request.session.device_id);
        }
        principal.profile.extend(request.extra.clone());
        principal.profile.insert(
            "realm".to_owned(),
            Value::String(effective_realm(request.realm.as_deref()).to_owned()),
        );

        let token = if let Some(token) = request
            .session
            .preset_token
            .as_ref()
            .filter(|token| !token.trim().is_empty())
        {
            token.clone()
        } else {
            self.strategy.create_token(request)?
        };
        if token.trim().is_empty() {
            return Err(AuthError::InvalidGeneratedToken);
        }
        let now = OffsetDateTime::now_utc();
        let established = self
            .store
            .establish(SessionRecord {
                token,
                principal,
                realm: effective_realm(request.realm.as_deref()).to_owned(),
                config: request.session.clone(),
                created_at: now,
                last_seen_at: now,
                attributes: request.extra.clone(),
            })
            .await?;
        SubjectScope::try_bind(established.token.clone());
        self.publish_best_effort(AuthEvent::LoginSucceeded {
            login_id,
            token: established.token.clone(),
            occurred_at: now,
        })
        .await;
        Ok(established.token)
    }

    async fn session_by_token(&self, token: &str) -> AuthResult<SessionRecord> {
        let session = self
            .store
            .find_by_token(token)
            .await?
            .ok_or(AuthError::InvalidToken)?;
        if self
            .strategy
            .is_expired(&session, OffsetDateTime::now_utc())
        {
            let _removed = self.store.revoke_token(token).await?;
            return Err(AuthError::SessionExpired);
        }
        let login_id = session
            .principal
            .login_id
            .as_ref()
            .ok_or(AuthError::InvalidLoginId)?;
        if self.account_disabled(login_id).await? {
            let _removed = self.store.revoke_token(token).await?;
            return Err(AuthError::AccountDisabled {
                login_id: login_id.to_string(),
            });
        }
        Ok(session)
    }

    async fn current_session(&self) -> AuthResult<Option<SessionRecord>> {
        let Some(token) = SubjectScope::current_token() else {
            return Ok(None);
        };
        match self.session_by_token(&token).await {
            Ok(session) => Ok(Some(session)),
            Err(AuthError::InvalidToken | AuthError::SessionExpired) => {
                SubjectScope::clear();
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    async fn principal_by_login(&self, login_id: &AuthId) -> AuthResult<Option<AuthPrincipal>> {
        if self.account_disabled(login_id).await? {
            return Ok(None);
        }
        Ok(self
            .store
            .find_by_login_id(login_id)
            .await?
            .map(|session| session.principal))
    }

    async fn permitted(&self, principal: &AuthPrincipal, permission: &str) -> AuthResult<bool> {
        let permissions = self.data_provider.permissions(principal).await?;
        Ok(self.strategy.has_element(&permissions, permission))
    }

    async fn role_matches(&self, principal: &AuthPrincipal, role: &str) -> AuthResult<bool> {
        let roles = self.data_provider.roles(principal).await?;
        Ok(self.strategy.has_element(&roles, role))
    }
}

/// Object-safe Subject SPI with Rust-native asynchronous extension methods.
pub trait Subject: Send + Sync {
    /// Returns the shared state machine backing this adapter facade.
    fn engine(&self) -> &SubjectEngine;

    /// Stable migration adapter identifier.
    fn adapter_name(&self) -> &'static str {
        "ddd4r-auth"
    }

    /// Realm applied when the request does not select one.
    fn default_realm(&self) -> Option<&str> {
        None
    }

    /// Returns the current task's authenticated principal.
    fn principal(&self) -> BoxFuture<'_, AuthResult<Option<AuthPrincipal>>> {
        Box::pin(async {
            Ok(self
                .engine()
                .current_session()
                .await?
                .map(|session| session.principal))
        })
    }

    /// Resolves a principal by account identifier.
    fn principal_by_login_id<'a>(
        &'a self,
        login_id: &'a AuthId,
    ) -> BoxFuture<'a, AuthResult<Option<AuthPrincipal>>> {
        Box::pin(async move { self.engine().principal_by_login(login_id).await })
    }

    /// Resolves a principal by opaque credential without binding it.
    fn principal_by_token<'a>(
        &'a self,
        token: &'a str,
    ) -> BoxFuture<'a, AuthResult<Option<AuthPrincipal>>> {
        Box::pin(async move {
            match self.engine().session_by_token(token).await {
                Ok(session) => Ok(Some(session.principal)),
                Err(AuthError::InvalidToken | AuthError::SessionExpired) => Ok(None),
                Err(error) => Err(error),
            }
        })
    }

    /// Establishes a session and binds it when called inside `SubjectScope`.
    fn login(&self, mut request: AuthRequest) -> BoxFuture<'_, AuthResult<String>> {
        if request
            .realm
            .as_deref()
            .is_none_or(|realm| realm.trim().is_empty())
            && let Some(realm) = self.default_realm()
        {
            request.realm = Some(realm.to_owned());
        }
        Box::pin(async move { self.engine().login_request(request).await })
    }

    /// Logs out the current task's session. The operation is idempotent.
    fn logout(&self) -> BoxFuture<'_, AuthResult<()>> {
        Box::pin(async {
            let Some(token) = SubjectScope::current_token() else {
                return Ok(());
            };
            let removed = self.engine().store.revoke_token(&token).await?;
            SubjectScope::clear();
            if let Some(session) = removed
                && let Some(login_id) = session.principal.login_id
            {
                self.engine()
                    .publish_best_effort(AuthEvent::LoggedOut {
                        login_id,
                        mode: AuthLogoutMode::Logout,
                        occurred_at: OffsetDateTime::now_utc(),
                    })
                    .await;
            }
            Ok(())
        })
    }

    /// Revokes every session for an account.
    fn logout_login<'a>(&'a self, login_id: &'a AuthId) -> BoxFuture<'a, AuthResult<usize>> {
        Box::pin(async move {
            let removed = self.engine().store.revoke_login(login_id).await?;
            if SubjectScope::current_token()
                .is_some_and(|current| removed.iter().any(|session| session.token == current))
            {
                SubjectScope::clear();
            }
            Ok(removed.len())
        })
    }

    /// Administrative alias for account logout.
    fn kickout<'a>(&'a self, login_id: &'a AuthId) -> BoxFuture<'a, AuthResult<usize>> {
        self.logout_login(login_id)
    }

    /// Rotates the current opaque credential and keeps its session state.
    fn refresh(&self) -> BoxFuture<'_, AuthResult<String>> {
        Box::pin(async {
            let old_token = SubjectScope::current_token().ok_or(AuthError::NotAuthenticated)?;
            let session = self.engine().session_by_token(&old_token).await?;
            let login_id = session
                .principal
                .login_id
                .clone()
                .ok_or(AuthError::InvalidLoginId)?;
            let mut request = AuthRequest::new(login_id.clone()).with_realm(session.realm);
            request.session = session.config;
            request.session.preset_token = None;
            let new_token = self.engine().strategy.create_token(&request)?;
            let rotated = self.engine().store.rotate(&old_token, new_token).await?;
            SubjectScope::bind(rotated.token.clone())?;
            self.engine()
                .publish_best_effort(AuthEvent::TokenRefreshed {
                    login_id,
                    occurred_at: OffsetDateTime::now_utc(),
                })
                .await;
            Ok(rotated.token)
        })
    }

    /// Verifies a token and returns its principal.
    fn verify<'a>(&'a self, token: &'a str) -> BoxFuture<'a, AuthResult<AuthPrincipal>> {
        Box::pin(async move { Ok(self.engine().session_by_token(token).await?.principal) })
    }

    /// Checks a permission for the current task.
    fn is_permitted<'a>(&'a self, permission: &'a str) -> BoxFuture<'a, AuthResult<bool>> {
        Box::pin(async move {
            let Some(principal) = self.principal().await? else {
                return Ok(false);
            };
            self.engine().permitted(&principal, permission).await
        })
    }

    /// Checks a permission for an account.
    fn is_permitted_for<'a>(
        &'a self,
        login_id: &'a AuthId,
        permission: &'a str,
    ) -> BoxFuture<'a, AuthResult<bool>> {
        Box::pin(async move {
            let Some(principal) = self.principal_by_login_id(login_id).await? else {
                return Ok(false);
            };
            self.engine().permitted(&principal, permission).await
        })
    }

    /// Checks whether any requested permission is granted.
    fn is_permitted_any<'a>(
        &'a self,
        permissions: &'a [&'a str],
    ) -> BoxFuture<'a, AuthResult<bool>> {
        Box::pin(async move {
            for permission in permissions {
                if self.is_permitted(permission).await? {
                    return Ok(true);
                }
            }
            Ok(false)
        })
    }

    /// Checks whether every requested permission is granted; empty input is false.
    fn is_permitted_all<'a>(
        &'a self,
        permissions: &'a [&'a str],
    ) -> BoxFuture<'a, AuthResult<bool>> {
        Box::pin(async move {
            if permissions.is_empty() {
                return Ok(false);
            }
            for permission in permissions {
                if !self.is_permitted(permission).await? {
                    return Ok(false);
                }
            }
            Ok(true)
        })
    }

    /// Checks a role for the current task.
    fn has_role<'a>(&'a self, role: &'a str) -> BoxFuture<'a, AuthResult<bool>> {
        Box::pin(async move {
            let Some(principal) = self.principal().await? else {
                return Ok(false);
            };
            self.engine().role_matches(&principal, role).await
        })
    }

    /// Checks whether any requested role is assigned.
    fn has_any_role<'a>(&'a self, roles: &'a [&'a str]) -> BoxFuture<'a, AuthResult<bool>> {
        Box::pin(async move {
            for role in roles {
                if self.has_role(role).await? {
                    return Ok(true);
                }
            }
            Ok(false)
        })
    }

    /// Checks whether every requested role is assigned; empty input is false.
    fn has_all_roles<'a>(&'a self, roles: &'a [&'a str]) -> BoxFuture<'a, AuthResult<bool>> {
        Box::pin(async move {
            if roles.is_empty() {
                return Ok(false);
            }
            for role in roles {
                if !self.has_role(role).await? {
                    return Ok(false);
                }
            }
            Ok(true)
        })
    }

    /// Returns whether the current task has a live session.
    fn is_authenticated(&self) -> BoxFuture<'_, AuthResult<bool>> {
        Box::pin(async { Ok(self.principal().await?.is_some()) })
    }

    /// Returns whether the current session was restored by remember-me policy.
    fn is_remembered(&self) -> BoxFuture<'_, AuthResult<bool>> {
        Box::pin(async { Ok(false) })
    }

    /// Checks the current principal's trusted device ID.
    fn is_trusted_device<'a>(&'a self, device_id: &'a str) -> BoxFuture<'a, AuthResult<bool>> {
        Box::pin(async move {
            Ok(self
                .principal()
                .await?
                .and_then(|principal| principal.device_id)
                .is_some_and(|current| current == device_id))
        })
    }

    /// Disables an account and immediately revokes all of its sessions.
    fn disable<'a>(
        &'a self,
        login_id: &'a AuthId,
        timeout_seconds: i64,
    ) -> BoxFuture<'a, AuthResult<()>> {
        Box::pin(async move {
            let clear_current = self
                .principal()
                .await?
                .and_then(|principal| principal.login_id)
                .as_ref()
                == Some(login_id);
            self.engine()
                .store
                .disable(login_id, timeout_seconds)
                .await?;
            if clear_current {
                SubjectScope::clear();
            }
            Ok(())
        })
    }

    /// Returns whether an account is disabled by store or business policy.
    fn is_disabled<'a>(&'a self, login_id: &'a AuthId) -> BoxFuture<'a, AuthResult<bool>> {
        Box::pin(async move { self.engine().account_disabled(login_id).await })
    }

    /// Re-enables an account in the session store.
    fn untie_disable<'a>(&'a self, login_id: &'a AuthId) -> BoxFuture<'a, AuthResult<()>> {
        Box::pin(async move { self.engine().store.enable(login_id).await })
    }

    /// Writes a current-session attribute.
    fn set_attribute(&self, key: String, value: Value) -> BoxFuture<'_, AuthResult<()>> {
        Box::pin(async move {
            let token = SubjectScope::current_token().ok_or(AuthError::NotAuthenticated)?;
            self.engine().store.set_attribute(&token, key, value).await
        })
    }

    /// Reads a current-session attribute.
    fn get_attribute<'a>(&'a self, key: &'a str) -> BoxFuture<'a, AuthResult<Option<Value>>> {
        Box::pin(async move {
            let token = SubjectScope::current_token().ok_or(AuthError::NotAuthenticated)?;
            self.engine().store.get_attribute(&token, key).await
        })
    }

    /// Reads a profile value from a verified token.
    fn get_extra<'a>(
        &'a self,
        token: &'a str,
        key: &'a str,
    ) -> BoxFuture<'a, AuthResult<Option<Value>>> {
        Box::pin(async move {
            Ok(self
                .engine()
                .session_by_token(token)
                .await?
                .principal
                .profile
                .get(key)
                .cloned())
        })
    }
}

impl Subject for SubjectEngine {
    fn engine(&self) -> &SubjectEngine {
        self
    }
}

/// Standard HTTP Bearer authenticator used by web adapters.
pub struct BearerAuthenticator;

impl BearerAuthenticator {
    /// Verifies an Authorization header and binds it to the active `SubjectScope`.
    pub async fn authenticate(
        subject: &dyn Subject,
        authorization: Option<&str>,
    ) -> AuthResult<AuthPrincipal> {
        let header = authorization.ok_or(AuthError::InvalidToken)?;
        let mut parts = header.split_whitespace();
        let scheme = parts.next().ok_or(AuthError::InvalidToken)?;
        let token = parts.next().ok_or(AuthError::InvalidToken)?;
        if !scheme.eq_ignore_ascii_case("bearer") || parts.next().is_some() {
            return Err(AuthError::InvalidToken);
        }
        let principal = subject.verify(token).await?;
        SubjectScope::bind(token.to_owned())?;
        Ok(principal)
    }
}

fn effective_realm(realm: Option<&str>) -> &str {
    realm
        .filter(|realm| !realm.trim().is_empty())
        .unwrap_or("default")
}

const fn error_code(error: &AuthError) -> &'static str {
    match error {
        AuthError::InvalidLoginId => "invalid_login_id",
        AuthError::UnknownAccount => "unknown_account",
        AuthError::BadCredentials => "bad_credentials",
        AuthError::AccountLocked { .. } => "account_locked",
        AuthError::AccountDisabled { .. } => "account_disabled",
        AuthError::AccountExpired { .. } => "account_expired",
        AuthError::CredentialsExpired { .. } => "credentials_expired",
        AuthError::InvalidToken => "invalid_token",
        AuthError::SessionExpired => "session_expired",
        AuthError::NotAuthenticated => "not_authenticated",
        AuthError::ConcurrentLoginRejected { .. } => "concurrent_login_rejected",
        AuthError::AccessDenied { .. } => "access_denied",
        AuthError::ProviderNotRegistered => "provider_not_registered",
        AuthError::InvalidGeneratedToken => "invalid_generated_token",
        AuthError::Store { .. } => "store_failure",
        AuthError::Infrastructure { .. } => "infrastructure_failure",
    }
}
