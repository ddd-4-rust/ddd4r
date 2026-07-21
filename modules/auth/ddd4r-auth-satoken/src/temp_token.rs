//! Temporary-token model and storage SPI.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use ddd4r_auth::{AuthError, AuthResult};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

/// Temporary credential payload used by mixed-login flows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SaTempToken {
    /// Authentication mechanism such as password, QR code, SMS or OAuth.
    pub auth_type: Option<String>,
    /// Per-client open identifier.
    pub openid: Option<String>,
    /// Cross-client union identifier.
    pub union_id: Option<String>,
    /// Account identifier required for login promotion.
    pub login_id: Option<String>,
    /// Login epoch timestamp.
    pub login_time: Option<i64>,
    /// Display name.
    pub nickname: Option<String>,
    /// Avatar URL.
    pub avatar: Option<String>,
    /// Phone number.
    pub phone: Option<String>,
    /// Email address.
    pub email: Option<String>,
    /// Gender code.
    pub gender: Option<i32>,
    /// Birthday epoch timestamp.
    pub birthday: Option<i64>,
    /// Age.
    pub age: Option<i32>,
    /// Region code.
    pub region_code: Option<String>,
    /// Country.
    pub country: Option<String>,
    /// Province.
    pub province: Option<String>,
    /// City.
    pub city: Option<String>,
    /// Area.
    pub area: Option<String>,
    /// Longitude.
    pub longitude: Option<f64>,
    /// Latitude.
    pub latitude: Option<f64>,
    /// Language code.
    pub lang: Option<String>,
    /// Time-zone identifier.
    pub zone: Option<String>,
    /// Client application identifier.
    pub app_id: Option<String>,
    /// Client channel.
    pub app_channel: Option<String>,
    /// Client version.
    pub app_version: Option<String>,
    /// Source IP address.
    pub ip_address: Option<String>,
    /// Device type.
    pub device_type: Option<String>,
    /// Device identifier.
    pub device_id: Option<String>,
    /// User-agent value.
    pub user_agent: Option<String>,
}

impl SaTempToken {
    /// Validates the minimum payload required for promotion to a session.
    pub fn validate(&self) -> AuthResult<()> {
        if self
            .login_id
            .as_deref()
            .is_none_or(|login_id| login_id.trim().is_empty())
        {
            return Err(AuthError::InvalidLoginId);
        }
        Ok(())
    }
}

/// Asynchronous temporary-token persistence contract.
pub trait TempTokenStore: Send + Sync {
    /// Persists one token payload.
    fn create<'a>(
        &'a self,
        service: &'a str,
        value: SaTempToken,
        timeout_seconds: i64,
    ) -> BoxFuture<'a, AuthResult<String>>;

    /// Parses one live token for a service.
    fn parse<'a>(
        &'a self,
        service: &'a str,
        token: &'a str,
    ) -> BoxFuture<'a, AuthResult<Option<SaTempToken>>>;

    /// Returns remaining seconds; `-1` means permanent and `-2` invalid.
    fn timeout<'a>(&'a self, service: &'a str, token: &'a str) -> BoxFuture<'a, AuthResult<i64>>;

    /// Revokes one token.
    fn delete<'a>(&'a self, service: &'a str, token: &'a str) -> BoxFuture<'a, AuthResult<bool>>;
}

#[derive(Debug, Clone)]
struct TempEntry {
    service: String,
    value: SaTempToken,
    expires_at: Option<OffsetDateTime>,
}

/// In-memory hashed-key temporary-token store.
#[derive(Debug, Default)]
pub struct InMemoryTempTokenStore {
    entries: RwLock<HashMap<blake3::Hash, TempEntry>>,
}

impl InMemoryTempTokenStore {
    /// Creates an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    fn read(&self) -> AuthResult<std::sync::RwLockReadGuard<'_, HashMap<blake3::Hash, TempEntry>>> {
        self.entries.read().map_err(|_| AuthError::Store {
            message: "temporary-token store lock poisoned".to_owned(),
        })
    }

    fn write(
        &self,
    ) -> AuthResult<std::sync::RwLockWriteGuard<'_, HashMap<blake3::Hash, TempEntry>>> {
        self.entries.write().map_err(|_| AuthError::Store {
            message: "temporary-token store lock poisoned".to_owned(),
        })
    }

    fn key(service: &str, token: &str) -> blake3::Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(service.as_bytes());
        hasher.update(&[0]);
        hasher.update(token.as_bytes());
        hasher.finalize()
    }
}

impl TempTokenStore for InMemoryTempTokenStore {
    fn create<'a>(
        &'a self,
        service: &'a str,
        value: SaTempToken,
        timeout_seconds: i64,
    ) -> BoxFuture<'a, AuthResult<String>> {
        Box::pin(async move {
            value.validate()?;
            let service = effective_service(service);
            let token = format!("tmp:{service}:{}", Uuid::now_v7());
            let expires_at = (timeout_seconds >= 0)
                .then(|| OffsetDateTime::now_utc() + Duration::seconds(timeout_seconds));
            self.write()?.insert(
                Self::key(service, &token),
                TempEntry {
                    service: service.to_owned(),
                    value,
                    expires_at,
                },
            );
            Ok(token)
        })
    }

    fn parse<'a>(
        &'a self,
        service: &'a str,
        token: &'a str,
    ) -> BoxFuture<'a, AuthResult<Option<SaTempToken>>> {
        Box::pin(async move {
            if token.trim().is_empty() {
                return Ok(None);
            }
            let service = effective_service(service);
            let key = Self::key(service, token);
            let expired = self.read()?.get(&key).is_some_and(|entry| {
                entry
                    .expires_at
                    .is_some_and(|until| until <= OffsetDateTime::now_utc())
            });
            if expired {
                self.write()?.remove(&key);
                return Ok(None);
            }
            Ok(self
                .read()?
                .get(&key)
                .filter(|entry| entry.service == service)
                .map(|entry| entry.value.clone()))
        })
    }

    fn timeout<'a>(&'a self, service: &'a str, token: &'a str) -> BoxFuture<'a, AuthResult<i64>> {
        Box::pin(async move {
            let service = effective_service(service);
            let key = Self::key(service, token);
            let Some(entry) = self.read()?.get(&key).cloned() else {
                return Ok(-2);
            };
            let Some(expires_at) = entry.expires_at else {
                return Ok(-1);
            };
            let remaining = (expires_at - OffsetDateTime::now_utc()).whole_seconds();
            if remaining < 0 {
                self.write()?.remove(&key);
                Ok(-2)
            } else {
                Ok(remaining)
            }
        })
    }

    fn delete<'a>(&'a self, service: &'a str, token: &'a str) -> BoxFuture<'a, AuthResult<bool>> {
        Box::pin(async move {
            Ok(self
                .write()?
                .remove(&Self::key(effective_service(service), token))
                .is_some())
        })
    }
}

/// Java-compatible facade over a replaceable temporary-token store.
#[derive(Clone)]
pub struct SaTempKit {
    store: Arc<dyn TempTokenStore>,
    service: String,
}

impl std::fmt::Debug for SaTempKit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SaTempKit")
            .field("service", &self.service)
            .finish_non_exhaustive()
    }
}

impl Default for SaTempKit {
    fn default() -> Self {
        Self::new(Arc::new(InMemoryTempTokenStore::new()), "default")
    }
}

impl SaTempKit {
    /// Creates a facade for one isolated business service namespace.
    pub fn new(store: Arc<dyn TempTokenStore>, service: impl Into<String>) -> Self {
        Self {
            store,
            service: service.into(),
        }
    }

    /// Creates a temporary token.
    pub async fn create_token(
        &self,
        value: SaTempToken,
        timeout_seconds: i64,
    ) -> AuthResult<String> {
        self.store
            .create(&self.service, value, timeout_seconds)
            .await
    }

    /// Parses a token without promoting it to an authenticated session.
    pub async fn parse_token(&self, token: &str) -> AuthResult<Option<SaTempToken>> {
        self.store.parse(&self.service, token).await
    }

    /// Returns remaining lifetime with ddd4j-compatible sentinel values.
    pub async fn get_timeout(&self, token: &str) -> AuthResult<i64> {
        self.store.timeout(&self.service, token).await
    }

    /// Revokes one token.
    pub async fn delete_token(&self, token: &str) -> AuthResult<bool> {
        self.store.delete(&self.service, token).await
    }

    /// Validates and returns one live temporary token.
    pub async fn check_temp_token(&self, token: &str) -> AuthResult<SaTempToken> {
        if token.trim().is_empty() {
            return Err(AuthError::InvalidToken);
        }
        let value = self
            .parse_token(token)
            .await?
            .ok_or(AuthError::InvalidToken)?;
        value.validate()?;
        Ok(value)
    }
}

fn effective_service(service: &str) -> &str {
    if service.trim().is_empty() {
        "default"
    } else {
        service
    }
}
