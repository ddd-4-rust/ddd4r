//! Pluggable session storage and an in-memory reference implementation.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::RwLock;

use futures::future::BoxFuture;
use serde_json::Value;
use time::{Duration, OffsetDateTime};

use crate::{
    AuthError, AuthId, AuthPrincipal, AuthReplacedLoginExitMode, AuthReplacedRange, AuthResult,
    AuthSessionConfig,
};

/// Persisted session state shared by authentication adapters.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionRecord {
    /// Opaque bearer credential.
    pub token: String,
    /// Authenticated identity.
    pub principal: AuthPrincipal,
    /// Authentication realm.
    pub realm: String,
    /// Effective session policy.
    pub config: AuthSessionConfig,
    /// Session creation time.
    pub created_at: OffsetDateTime,
    /// Last successful verification time.
    pub last_seen_at: OffsetDateTime,
    /// Session-scoped attributes.
    pub attributes: BTreeMap<String, Value>,
}

impl SessionRecord {
    fn login_id(&self) -> AuthResult<&AuthId> {
        self.principal
            .login_id
            .as_ref()
            .ok_or(AuthError::InvalidLoginId)
    }

    fn is_expired(&self, now: OffsetDateTime) -> bool {
        let absolute_expired = self.config.timeout_seconds >= 0
            && now >= self.created_at + Duration::seconds(self.config.timeout_seconds);
        let idle_expired = self.config.active_timeout_seconds.is_some_and(|seconds| {
            seconds >= 0 && now >= self.last_seen_at + Duration::seconds(seconds)
        });
        absolute_expired || idle_expired
    }
}

/// Object-safe persistence SPI for authentication sessions.
pub trait SessionStore: Send + Sync {
    /// Establishes a session while atomically enforcing concurrency policy.
    fn establish(&self, session: SessionRecord) -> BoxFuture<'_, AuthResult<SessionRecord>>;

    /// Resolves and touches one live session.
    fn find_by_token<'a>(
        &'a self,
        token: &'a str,
    ) -> BoxFuture<'a, AuthResult<Option<SessionRecord>>>;

    /// Resolves the newest live session for an account.
    fn find_by_login_id<'a>(
        &'a self,
        login_id: &'a AuthId,
    ) -> BoxFuture<'a, AuthResult<Option<SessionRecord>>>;

    /// Revokes one credential.
    fn revoke_token<'a>(
        &'a self,
        token: &'a str,
    ) -> BoxFuture<'a, AuthResult<Option<SessionRecord>>>;

    /// Revokes all credentials for an account.
    fn revoke_login<'a>(
        &'a self,
        login_id: &'a AuthId,
    ) -> BoxFuture<'a, AuthResult<Vec<SessionRecord>>>;

    /// Atomically rotates a credential.
    fn rotate<'a>(
        &'a self,
        old_token: &'a str,
        new_token: String,
    ) -> BoxFuture<'a, AuthResult<SessionRecord>>;

    /// Writes one session attribute.
    fn set_attribute<'a>(
        &'a self,
        token: &'a str,
        key: String,
        value: Value,
    ) -> BoxFuture<'a, AuthResult<()>>;

    /// Reads one session attribute.
    fn get_attribute<'a>(
        &'a self,
        token: &'a str,
        key: &'a str,
    ) -> BoxFuture<'a, AuthResult<Option<Value>>>;

    /// Disables an account permanently or for the supplied number of seconds.
    fn disable<'a>(
        &'a self,
        login_id: &'a AuthId,
        timeout_seconds: i64,
    ) -> BoxFuture<'a, AuthResult<()>>;

    /// Returns whether an account is currently disabled.
    fn is_disabled<'a>(&'a self, login_id: &'a AuthId) -> BoxFuture<'a, AuthResult<bool>>;

    /// Clears an account disable marker.
    fn enable<'a>(&'a self, login_id: &'a AuthId) -> BoxFuture<'a, AuthResult<()>>;
}

#[derive(Debug, Clone, Copy)]
enum DisabledUntil {
    Permanent,
    Until(OffsetDateTime),
}

#[derive(Debug, Default)]
struct StoreState {
    sessions: HashMap<String, SessionRecord>,
    login_tokens: HashMap<AuthId, BTreeSet<String>>,
    disabled: HashMap<AuthId, DisabledUntil>,
}

/// Single-process session store for tests, examples and local development.
#[derive(Debug, Default)]
pub struct InMemorySessionStore {
    state: RwLock<StoreState>,
}

impl InMemorySessionStore {
    /// Creates an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    fn read(&self) -> AuthResult<std::sync::RwLockReadGuard<'_, StoreState>> {
        self.state.read().map_err(|_| AuthError::Store {
            message: "in-memory store lock poisoned".to_owned(),
        })
    }

    fn write(&self) -> AuthResult<std::sync::RwLockWriteGuard<'_, StoreState>> {
        self.state.write().map_err(|_| AuthError::Store {
            message: "in-memory store lock poisoned".to_owned(),
        })
    }

    fn remove_locked(state: &mut StoreState, token: &str) -> Option<SessionRecord> {
        let removed = state.sessions.remove(token)?;
        if let Some(login_id) = removed.principal.login_id.as_ref()
            && let Some(tokens) = state.login_tokens.get_mut(login_id)
        {
            tokens.remove(token);
            if tokens.is_empty() {
                state.login_tokens.remove(login_id);
            }
        }
        Some(removed)
    }

    fn matching_tokens(state: &StoreState, session: &SessionRecord) -> AuthResult<Vec<String>> {
        let login_id = session.login_id()?;
        let tokens = state
            .login_tokens
            .get(login_id)
            .into_iter()
            .flatten()
            .filter(|token| {
                state.sessions.get(*token).is_some_and(|existing| {
                    existing.realm == session.realm
                        && (session.config.replaced_range == AuthReplacedRange::AllDeviceTypes
                            || existing.config.device_type == session.config.device_type)
                })
            })
            .cloned()
            .collect();
        Ok(tokens)
    }

    fn purge_expired_locked(state: &mut StoreState, now: OffsetDateTime) {
        let expired: Vec<String> = state
            .sessions
            .iter()
            .filter(|(_, session)| session.is_expired(now))
            .map(|(token, _)| token.clone())
            .collect();
        for token in expired {
            Self::remove_locked(state, &token);
        }
    }
}

impl SessionStore for InMemorySessionStore {
    fn establish(&self, session: SessionRecord) -> BoxFuture<'_, AuthResult<SessionRecord>> {
        Box::pin(async move {
            if session.token.trim().is_empty() {
                return Err(AuthError::InvalidGeneratedToken);
            }
            let mut state = self.write()?;
            let now = OffsetDateTime::now_utc();
            Self::purge_expired_locked(&mut state, now);
            if state.sessions.contains_key(&session.token) {
                return Err(AuthError::InvalidGeneratedToken);
            }

            let login_id = session.login_id()?.clone();
            let matching = Self::matching_tokens(&state, &session)?;
            if session.config.concurrent
                && session.config.share
                && let Some(existing) = matching
                    .iter()
                    .filter_map(|token| state.sessions.get(token))
                    .max_by_key(|existing| existing.created_at)
            {
                return Ok(existing.clone());
            }

            if !session.config.concurrent && !matching.is_empty() {
                match session.config.replaced_login_exit_mode {
                    AuthReplacedLoginExitMode::NewDevice => {
                        return Err(AuthError::ConcurrentLoginRejected {
                            login_id: login_id.to_string(),
                        });
                    }
                    AuthReplacedLoginExitMode::OldDevice => {
                        for token in matching {
                            Self::remove_locked(&mut state, &token);
                        }
                    }
                }
            }

            if session.config.concurrent
                && !session.config.share
                && session.config.max_login_count > 0
            {
                let maximum = usize::try_from(session.config.max_login_count).map_err(|error| {
                    AuthError::Store {
                        message: error.to_string(),
                    }
                })?;
                let mut sessions: Vec<(String, OffsetDateTime)> = state
                    .login_tokens
                    .get(&login_id)
                    .into_iter()
                    .flatten()
                    .filter_map(|token| {
                        state
                            .sessions
                            .get(token)
                            .map(|record| (token.clone(), record.created_at))
                    })
                    .collect();
                sessions.sort_by_key(|(_, created_at)| *created_at);
                let overflow = sessions.len().saturating_add(1).saturating_sub(maximum);
                for (token, _) in sessions.into_iter().take(overflow) {
                    Self::remove_locked(&mut state, &token);
                }
            }

            state
                .login_tokens
                .entry(login_id)
                .or_default()
                .insert(session.token.clone());
            state
                .sessions
                .insert(session.token.clone(), session.clone());
            Ok(session)
        })
    }

    fn find_by_token<'a>(
        &'a self,
        token: &'a str,
    ) -> BoxFuture<'a, AuthResult<Option<SessionRecord>>> {
        Box::pin(async move {
            let mut state = self.write()?;
            let now = OffsetDateTime::now_utc();
            if state
                .sessions
                .get(token)
                .is_some_and(|session| session.is_expired(now))
            {
                Self::remove_locked(&mut state, token);
                return Err(AuthError::SessionExpired);
            }
            Ok(state.sessions.get_mut(token).map(|session| {
                session.last_seen_at = now;
                session.clone()
            }))
        })
    }

    fn find_by_login_id<'a>(
        &'a self,
        login_id: &'a AuthId,
    ) -> BoxFuture<'a, AuthResult<Option<SessionRecord>>> {
        Box::pin(async move {
            let mut state = self.write()?;
            let now = OffsetDateTime::now_utc();
            Self::purge_expired_locked(&mut state, now);
            let newest_token = state.login_tokens.get(login_id).and_then(|tokens| {
                tokens
                    .iter()
                    .filter_map(|token| {
                        state
                            .sessions
                            .get(token)
                            .map(|session| (token, session.created_at))
                    })
                    .max_by_key(|(_, created_at)| *created_at)
                    .map(|(token, _)| token.clone())
            });
            Ok(newest_token.and_then(|token| state.sessions.get(&token).cloned()))
        })
    }

    fn revoke_token<'a>(
        &'a self,
        token: &'a str,
    ) -> BoxFuture<'a, AuthResult<Option<SessionRecord>>> {
        Box::pin(async move {
            let mut state = self.write()?;
            Ok(Self::remove_locked(&mut state, token))
        })
    }

    fn revoke_login<'a>(
        &'a self,
        login_id: &'a AuthId,
    ) -> BoxFuture<'a, AuthResult<Vec<SessionRecord>>> {
        Box::pin(async move {
            let mut state = self.write()?;
            let tokens = state
                .login_tokens
                .get(login_id)
                .cloned()
                .unwrap_or_default();
            Ok(tokens
                .into_iter()
                .filter_map(|token| Self::remove_locked(&mut state, &token))
                .collect())
        })
    }

    fn rotate<'a>(
        &'a self,
        old_token: &'a str,
        new_token: String,
    ) -> BoxFuture<'a, AuthResult<SessionRecord>> {
        Box::pin(async move {
            if new_token.trim().is_empty() {
                return Err(AuthError::InvalidGeneratedToken);
            }
            let mut state = self.write()?;
            if state.sessions.contains_key(&new_token) {
                return Err(AuthError::InvalidGeneratedToken);
            }
            let mut session =
                Self::remove_locked(&mut state, old_token).ok_or(AuthError::InvalidToken)?;
            session.token.clone_from(&new_token);
            session.last_seen_at = OffsetDateTime::now_utc();
            let login_id = session.login_id()?.clone();
            state
                .login_tokens
                .entry(login_id)
                .or_default()
                .insert(new_token.clone());
            state.sessions.insert(new_token, session.clone());
            Ok(session)
        })
    }

    fn set_attribute<'a>(
        &'a self,
        token: &'a str,
        key: String,
        value: Value,
    ) -> BoxFuture<'a, AuthResult<()>> {
        Box::pin(async move {
            let mut state = self.write()?;
            let session = state
                .sessions
                .get_mut(token)
                .ok_or(AuthError::InvalidToken)?;
            session.attributes.insert(key, value);
            Ok(())
        })
    }

    fn get_attribute<'a>(
        &'a self,
        token: &'a str,
        key: &'a str,
    ) -> BoxFuture<'a, AuthResult<Option<Value>>> {
        Box::pin(async move {
            Ok(self
                .read()?
                .sessions
                .get(token)
                .and_then(|session| session.attributes.get(key).cloned()))
        })
    }

    fn disable<'a>(
        &'a self,
        login_id: &'a AuthId,
        timeout_seconds: i64,
    ) -> BoxFuture<'a, AuthResult<()>> {
        Box::pin(async move {
            let mut state = self.write()?;
            let until = if timeout_seconds < 0 {
                DisabledUntil::Permanent
            } else {
                DisabledUntil::Until(OffsetDateTime::now_utc() + Duration::seconds(timeout_seconds))
            };
            state.disabled.insert(login_id.clone(), until);
            let tokens = state
                .login_tokens
                .get(login_id)
                .cloned()
                .unwrap_or_default();
            for token in tokens {
                Self::remove_locked(&mut state, &token);
            }
            Ok(())
        })
    }

    fn is_disabled<'a>(&'a self, login_id: &'a AuthId) -> BoxFuture<'a, AuthResult<bool>> {
        Box::pin(async move {
            let mut state = self.write()?;
            match state.disabled.get(login_id).copied() {
                Some(DisabledUntil::Permanent) => Ok(true),
                Some(DisabledUntil::Until(until)) if until > OffsetDateTime::now_utc() => Ok(true),
                Some(DisabledUntil::Until(_)) => {
                    state.disabled.remove(login_id);
                    Ok(false)
                }
                None => Ok(false),
            }
        })
    }

    fn enable<'a>(&'a self, login_id: &'a AuthId) -> BoxFuture<'a, AuthResult<()>> {
        Box::pin(async move {
            self.write()?.disabled.remove(login_id);
            Ok(())
        })
    }
}
