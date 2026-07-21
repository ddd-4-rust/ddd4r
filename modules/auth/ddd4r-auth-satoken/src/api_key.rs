//! Scoped internal API-key contracts.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, RwLock};

use ddd4r_auth::{AuthError, AuthResult};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;
use zeroize::Zeroizing;

/// Metadata stored for one API key; the raw secret is never retained.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiKeyRecord {
    /// Public identifier useful for audit correlation.
    pub key_id: String,
    /// Granted scopes.
    pub scopes: BTreeSet<String>,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Optional absolute expiry.
    pub expires_at: Option<OffsetDateTime>,
}

/// Asynchronous API-key storage contract.
pub trait ApiKeyStore: Send + Sync {
    /// Persists a digest and its metadata.
    fn insert(&self, digest: blake3::Hash, record: ApiKeyRecord) -> BoxFuture<'_, AuthResult<()>>;

    /// Resolves metadata by secret digest.
    fn find(&self, digest: blake3::Hash) -> BoxFuture<'_, AuthResult<Option<ApiKeyRecord>>>;

    /// Revokes a secret digest.
    fn remove(&self, digest: blake3::Hash) -> BoxFuture<'_, AuthResult<bool>>;
}

/// In-memory hashed-secret API-key store.
#[derive(Debug, Default)]
pub struct InMemoryApiKeyStore {
    records: RwLock<HashMap<blake3::Hash, ApiKeyRecord>>,
}

impl InMemoryApiKeyStore {
    /// Creates an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    fn read(
        &self,
    ) -> AuthResult<std::sync::RwLockReadGuard<'_, HashMap<blake3::Hash, ApiKeyRecord>>> {
        self.records.read().map_err(|_| AuthError::Store {
            message: "API-key store lock poisoned".to_owned(),
        })
    }

    fn write(
        &self,
    ) -> AuthResult<std::sync::RwLockWriteGuard<'_, HashMap<blake3::Hash, ApiKeyRecord>>> {
        self.records.write().map_err(|_| AuthError::Store {
            message: "API-key store lock poisoned".to_owned(),
        })
    }
}

impl ApiKeyStore for InMemoryApiKeyStore {
    fn insert(&self, digest: blake3::Hash, record: ApiKeyRecord) -> BoxFuture<'_, AuthResult<()>> {
        Box::pin(async move {
            self.write()?.insert(digest, record);
            Ok(())
        })
    }

    fn find(&self, digest: blake3::Hash) -> BoxFuture<'_, AuthResult<Option<ApiKeyRecord>>> {
        Box::pin(async move { Ok(self.read()?.get(&digest).cloned()) })
    }

    fn remove(&self, digest: blake3::Hash) -> BoxFuture<'_, AuthResult<bool>> {
        Box::pin(async move { Ok(self.write()?.remove(&digest).is_some()) })
    }
}

/// Issues, validates and revokes scoped API keys.
#[derive(Clone)]
pub struct ApiKeyKit {
    store: Arc<dyn ApiKeyStore>,
    pepper: Arc<Zeroizing<Vec<u8>>>,
}

impl std::fmt::Debug for ApiKeyKit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("ApiKeyKit").finish_non_exhaustive()
    }
}

impl ApiKeyKit {
    /// Creates a kit. Production callers should provide a secret application pepper.
    pub fn new(store: Arc<dyn ApiKeyStore>, pepper: impl Into<Vec<u8>>) -> AuthResult<Self> {
        let pepper = pepper.into();
        if pepper.len() < 32 {
            return Err(AuthError::Infrastructure {
                message: "API-key pepper must contain at least 32 bytes".to_owned(),
            });
        }
        Ok(Self {
            store,
            pepper: Arc::new(Zeroizing::new(pepper)),
        })
    }

    /// Issues a raw key once and stores only its keyed BLAKE3 digest.
    pub async fn issue(
        &self,
        scopes: impl IntoIterator<Item = String>,
        timeout_seconds: i64,
    ) -> AuthResult<(String, ApiKeyRecord)> {
        let key_id = Uuid::now_v7().to_string();
        let secret = format!("ak_{key_id}_{}", Uuid::now_v7());
        let now = OffsetDateTime::now_utc();
        let record = ApiKeyRecord {
            key_id,
            scopes: scopes
                .into_iter()
                .filter(|scope| !scope.trim().is_empty())
                .collect(),
            created_at: now,
            expires_at: (timeout_seconds >= 0).then(|| now + Duration::seconds(timeout_seconds)),
        };
        self.store
            .insert(self.digest(&secret), record.clone())
            .await?;
        Ok((secret, record))
    }

    /// Validates existence, expiry and all required scopes.
    pub async fn check(
        &self,
        api_key: &str,
        required_scopes: &[String],
    ) -> AuthResult<ApiKeyRecord> {
        if api_key.trim().is_empty() {
            return Err(AuthError::AccessDenied {
                authority: "internal-api-key".to_owned(),
            });
        }
        let digest = self.digest(api_key);
        let record = self
            .store
            .find(digest)
            .await?
            .ok_or_else(|| AuthError::AccessDenied {
                authority: "internal-api-key".to_owned(),
            })?;
        if record
            .expires_at
            .is_some_and(|expires_at| expires_at <= OffsetDateTime::now_utc())
        {
            let _removed = self.store.remove(digest).await?;
            return Err(AuthError::AccessDenied {
                authority: "internal-api-key".to_owned(),
            });
        }
        if let Some(missing) = required_scopes
            .iter()
            .find(|scope| !record.scopes.contains(scope.as_str()))
        {
            return Err(AuthError::AccessDenied {
                authority: format!("api-key-scope:{missing}"),
            });
        }
        Ok(record)
    }

    /// Revokes one raw API key.
    pub async fn revoke(&self, api_key: &str) -> AuthResult<bool> {
        self.store.remove(self.digest(api_key)).await
    }

    fn digest(&self, api_key: &str) -> blake3::Hash {
        let mut key = [0_u8; 32];
        key.copy_from_slice(&self.pepper[..32]);
        blake3::keyed_hash(&key, api_key.as_bytes())
    }
}
