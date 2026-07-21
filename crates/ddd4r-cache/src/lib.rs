//! Application-level cache contracts, separate from `RBatis` second-level caches.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use ddd4r_core::DddResult;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

/// Namespaced cache key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CacheKey {
    /// Logical namespace.
    pub namespace: String,
    /// Key inside the namespace.
    pub key: String,
}

impl CacheKey {
    /// Creates a cache key.
    pub fn new(namespace: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            key: key.into(),
        }
    }
}

/// Serialized cache value plus monotonic version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheValue {
    /// Backend-independent bytes.
    pub bytes: Vec<u8>,
    /// Version used for compare-and-set.
    pub version: u64,
}

/// Cache operation counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Successful reads.
    pub hits: u64,
    /// Missing or expired reads.
    pub misses: u64,
    /// Writes.
    pub puts: u64,
    /// Explicit or expiry removals.
    pub removals: u64,
}

/// Application cache SPI.
pub trait Cache: Send + Sync + 'static {
    /// Reads a value.
    fn get<'a>(&'a self, key: &'a CacheKey) -> BoxFuture<'a, DddResult<Option<CacheValue>>>;

    /// Writes a value with optional TTL.
    fn put(
        &self,
        key: CacheKey,
        bytes: Vec<u8>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'_, DddResult<CacheValue>>;

    /// Removes one value.
    fn remove<'a>(&'a self, key: &'a CacheKey) -> BoxFuture<'a, DddResult<bool>>;

    /// Clears one logical namespace.
    fn clear_namespace<'a>(&'a self, namespace: &'a str) -> BoxFuture<'a, DddResult<u64>>;

    /// Returns a point-in-time statistics snapshot.
    fn stats(&self) -> CacheStats;
}

/// Atomic cache operations.
pub trait AtomicCache: Cache {
    /// Replaces a value only when its current version matches `expected_version`.
    fn compare_and_set(
        &self,
        key: CacheKey,
        expected_version: u64,
        bytes: Vec<u8>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'_, DddResult<Option<CacheValue>>>;
}

#[derive(Debug, Clone)]
struct StoredValue {
    value: CacheValue,
    expires_at: Option<Instant>,
}

#[derive(Default)]
struct Counters {
    hits: AtomicU64,
    misses: AtomicU64,
    puts: AtomicU64,
    removals: AtomicU64,
}

/// Process-local cache useful for tests and single-instance deployments.
#[derive(Clone, Default)]
pub struct InMemoryCache {
    values: Arc<DashMap<CacheKey, StoredValue>>,
    versions: Arc<AtomicU64>,
    counters: Arc<Counters>,
}

impl InMemoryCache {
    /// Creates an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    fn next_version(&self) -> u64 {
        self.versions
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1)
    }

    fn expiry(ttl: Option<Duration>) -> Option<Instant> {
        ttl.and_then(|duration| Instant::now().checked_add(duration))
    }
}

impl Cache for InMemoryCache {
    fn get<'a>(&'a self, key: &'a CacheKey) -> BoxFuture<'a, DddResult<Option<CacheValue>>> {
        Box::pin(async move {
            if let Some(entry) = self.values.get(key) {
                let expired = entry
                    .expires_at
                    .is_some_and(|expires_at| expires_at <= Instant::now());
                if !expired {
                    self.counters.hits.fetch_add(1, Ordering::Relaxed);
                    return Ok(Some(entry.value.clone()));
                }
                drop(entry);
                self.values.remove(key);
                self.counters.removals.fetch_add(1, Ordering::Relaxed);
            }
            self.counters.misses.fetch_add(1, Ordering::Relaxed);
            Ok(None)
        })
    }

    fn put(
        &self,
        key: CacheKey,
        bytes: Vec<u8>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'_, DddResult<CacheValue>> {
        Box::pin(async move {
            let value = CacheValue {
                bytes,
                version: self.next_version(),
            };
            self.values.insert(
                key,
                StoredValue {
                    value: value.clone(),
                    expires_at: Self::expiry(ttl),
                },
            );
            self.counters.puts.fetch_add(1, Ordering::Relaxed);
            Ok(value)
        })
    }

    fn remove<'a>(&'a self, key: &'a CacheKey) -> BoxFuture<'a, DddResult<bool>> {
        Box::pin(async move {
            let removed = self.values.remove(key).is_some();
            if removed {
                self.counters.removals.fetch_add(1, Ordering::Relaxed);
            }
            Ok(removed)
        })
    }

    fn clear_namespace<'a>(&'a self, namespace: &'a str) -> BoxFuture<'a, DddResult<u64>> {
        Box::pin(async move {
            let keys = self
                .values
                .iter()
                .filter(|entry| entry.key().namespace == namespace)
                .map(|entry| entry.key().clone())
                .collect::<Vec<_>>();
            let removed = u64::try_from(keys.len()).unwrap_or(u64::MAX);
            for key in keys {
                self.values.remove(&key);
            }
            self.counters.removals.fetch_add(removed, Ordering::Relaxed);
            Ok(removed)
        })
    }

    fn stats(&self) -> CacheStats {
        CacheStats {
            hits: self.counters.hits.load(Ordering::Relaxed),
            misses: self.counters.misses.load(Ordering::Relaxed),
            puts: self.counters.puts.load(Ordering::Relaxed),
            removals: self.counters.removals.load(Ordering::Relaxed),
        }
    }
}

impl AtomicCache for InMemoryCache {
    fn compare_and_set(
        &self,
        key: CacheKey,
        expected_version: u64,
        bytes: Vec<u8>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'_, DddResult<Option<CacheValue>>> {
        Box::pin(async move {
            let Some(mut entry) = self.values.get_mut(&key) else {
                return Ok(None);
            };
            if entry.value.version != expected_version {
                return Ok(None);
            }
            let value = CacheValue {
                bytes,
                version: self.next_version(),
            };
            *entry = StoredValue {
                value: value.clone(),
                expires_at: Self::expiry(ttl),
            };
            self.counters.puts.fetch_add(1, Ordering::Relaxed);
            Ok(Some(value))
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{AtomicCache, Cache, CacheKey, InMemoryCache};

    #[tokio::test]
    async fn expires_and_tracks_stats() {
        let cache = InMemoryCache::new();
        let key = CacheKey::new("orders", "1");
        cache
            .put(key.clone(), b"value".to_vec(), Some(Duration::ZERO))
            .await
            .unwrap();
        assert!(cache.get(&key).await.unwrap().is_none());
        assert_eq!(cache.stats().misses, 1);
    }

    #[tokio::test]
    async fn compare_and_set_requires_current_version() {
        let cache = InMemoryCache::new();
        let key = CacheKey::new("orders", "1");
        let current = cache.put(key.clone(), vec![1], None).await.unwrap();
        assert!(
            cache
                .compare_and_set(key.clone(), current.version + 1, vec![2], None)
                .await
                .unwrap()
                .is_none()
        );
        let updated = cache
            .compare_and_set(key.clone(), current.version, vec![2], None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.bytes, vec![2]);
    }
}
