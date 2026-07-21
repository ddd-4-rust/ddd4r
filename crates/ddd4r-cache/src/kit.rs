//! Unified registry and facade corresponding to ddd4j `CacheKit`.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use dashmap::DashMap;
use ddd4r_core::{DddError, DddResult};
use futures::future::BoxFuture;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::Mutex;

use crate::{
    Cache, CacheConfig, CacheKey, CacheLockLease, CacheStats, CacheValue, InMemoryCache,
    LocalCacheType,
};

/// Async cache loader. Returning `None` does not populate the cache.
pub type CacheLoader =
    Arc<dyn Fn(CacheKey) -> BoxFuture<'static, DddResult<Option<Vec<u8>>>> + Send + Sync + 'static>;

/// Unified application-cache registry and operation facade.
#[derive(Clone, Default)]
pub struct CacheKit {
    caches: Arc<DashMap<String, Arc<dyn Cache>>>,
    loaders: Arc<DashMap<String, CacheLoader>>,
    flights: Arc<DashMap<CacheKey, Arc<Mutex<()>>>>,
    default_type: Arc<AtomicU8>,
}

impl CacheKit {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    fn validate_name(name: &str) -> DddResult<()> {
        if name.trim().is_empty() {
            Err(DddError::Adapter {
                adapter: "cache-kit",
                message: "cache name must not be blank".to_owned(),
            })
        } else {
            Ok(())
        }
    }

    /// Returns the local implementation selected for compatibility builds.
    pub fn default_type(&self) -> LocalCacheType {
        match self.default_type.load(Ordering::Relaxed) {
            1 => LocalCacheType::Guava,
            2 => LocalCacheType::Hutool,
            _ => LocalCacheType::Caffeine,
        }
    }

    /// Updates the compatibility build selector.
    pub fn set_default_type(&self, cache_type: LocalCacheType) {
        let value = match cache_type {
            LocalCacheType::Caffeine => 0,
            LocalCacheType::Guava => 1,
            LocalCacheType::Hutool => 2,
        };
        self.default_type.store(value, Ordering::Relaxed);
    }

    /// Registers or replaces an externally managed cache.
    pub fn register(
        &self,
        name: impl Into<String>,
        cache: Arc<dyn Cache>,
    ) -> DddResult<Option<Arc<dyn Cache>>> {
        let name = name.into();
        Self::validate_name(&name)?;
        Ok(self.caches.insert(name, cache))
    }

    /// Registers a cache and a miss loader.
    pub fn register_loading(
        &self,
        name: impl Into<String>,
        cache: Arc<dyn Cache>,
        loader: CacheLoader,
    ) -> DddResult<Option<Arc<dyn Cache>>> {
        let name = name.into();
        Self::validate_name(&name)?;
        self.loaders.insert(name.clone(), loader);
        Ok(self.caches.insert(name, cache))
    }

    /// Builds and registers a native process-local cache.
    pub fn build(&self, config: CacheConfig) -> DddResult<Arc<InMemoryCache>> {
        Self::validate_name(&config.name)?;
        let name = config.name.clone();
        let cache = Arc::new(InMemoryCache::with_config(config));
        self.caches.insert(name, cache.clone());
        Ok(cache)
    }

    /// Builds a native process-local cache with a miss loader.
    pub fn build_loading(
        &self,
        config: CacheConfig,
        loader: CacheLoader,
    ) -> DddResult<Arc<InMemoryCache>> {
        let name = config.name.clone();
        let cache = self.build(config)?;
        self.loaders.insert(name, loader);
        Ok(cache)
    }

    /// Removes a cache and its loader.
    pub fn unregister(&self, name: &str) -> Option<Arc<dyn Cache>> {
        self.loaders.remove(name);
        self.caches.remove(name).map(|(_, cache)| cache)
    }

    /// Gets a registered cache without creating it.
    pub fn cache(&self, name: &str) -> Option<Arc<dyn Cache>> {
        self.caches.get(name).map(|entry| entry.value().clone())
    }

    /// Lists all registered cache names in deterministic order.
    pub fn cache_names(&self) -> Vec<String> {
        let mut names = self
            .caches
            .iter()
            .map(|entry| entry.key().clone())
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    /// Reads a value and invokes a configured loader once per key on concurrent misses.
    pub async fn get(&self, name: &str, key: &str) -> DddResult<Option<CacheValue>> {
        let Some(cache) = self.cache(name) else {
            return Ok(None);
        };
        let cache_key = CacheKey::new(name, key);
        if let Some(value) = cache.get(&cache_key).await? {
            return Ok(Some(value));
        }
        let Some(loader) = self.loaders.get(name).map(|entry| entry.value().clone()) else {
            return Ok(None);
        };

        let flight = self
            .flights
            .entry(cache_key.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _guard = flight.lock().await;
        if let Some(value) = cache.get(&cache_key).await? {
            self.flights.remove(&cache_key);
            return Ok(Some(value));
        }
        cache.record_load();
        let loaded_bytes = loader(cache_key.clone()).await?;
        let value = match loaded_bytes {
            Some(bytes) => Some(cache.put(cache_key.clone(), bytes, None).await?),
            None => None,
        };
        self.flights.remove(&cache_key);
        Ok(value)
    }

    /// Reads and deserializes JSON into a typed value.
    pub async fn get_typed<T: DeserializeOwned>(
        &self,
        name: &str,
        key: &str,
    ) -> DddResult<Option<T>> {
        self.get(name, key)
            .await?
            .map(|value| serde_json::from_slice(&value.bytes).map_err(DddError::from))
            .transpose()
    }

    /// Serializes JSON and stores a typed value.
    pub async fn put_typed<T: Serialize + ?Sized>(
        &self,
        name: &str,
        key: &str,
        value: &T,
        ttl: Option<Duration>,
    ) -> DddResult<Option<CacheValue>> {
        let Some(cache) = self.cache(name) else {
            return Ok(None);
        };
        let bytes = serde_json::to_vec(value)?;
        cache
            .put(CacheKey::new(name, key), bytes, ttl)
            .await
            .map(Some)
    }

    /// Stores raw bytes.
    pub async fn put(
        &self,
        name: &str,
        key: &str,
        bytes: Vec<u8>,
        ttl: Option<Duration>,
    ) -> DddResult<Option<CacheValue>> {
        let Some(cache) = self.cache(name) else {
            return Ok(None);
        };
        cache
            .put(CacheKey::new(name, key), bytes, ttl)
            .await
            .map(Some)
    }

    /// Invalidates one key.
    pub async fn invalidate(&self, name: &str, key: &str) -> DddResult<bool> {
        let Some(cache) = self.cache(name) else {
            return Ok(false);
        };
        cache.remove(&CacheKey::new(name, key)).await
    }

    /// Clears all keys owned by a logical cache.
    pub async fn invalidate_all(&self, name: &str) -> DddResult<u64> {
        let Some(cache) = self.cache(name) else {
            return Ok(0);
        };
        cache.clear_namespace(name).await
    }

    /// Inserts only when a key is absent.
    pub async fn put_if_absent(
        &self,
        name: &str,
        key: &str,
        bytes: Vec<u8>,
        ttl: Option<Duration>,
    ) -> DddResult<Option<CacheValue>> {
        let cache = self.required_cache(name)?;
        cache
            .put_if_absent(CacheKey::new(name, key), bytes, ttl)
            .await
    }

    /// Replaces only when bytes match the expected value.
    pub async fn replace(
        &self,
        name: &str,
        key: &str,
        expected: Vec<u8>,
        bytes: Vec<u8>,
        ttl: Option<Duration>,
    ) -> DddResult<Option<CacheValue>> {
        let cache = self.required_cache(name)?;
        cache
            .replace(CacheKey::new(name, key), expected, bytes, ttl)
            .await
    }

    /// Removes only when bytes match the expected value.
    pub async fn remove_if(&self, name: &str, key: &str, expected: Vec<u8>) -> DddResult<bool> {
        let cache = self.required_cache(name)?;
        cache.remove_if(CacheKey::new(name, key), expected).await
    }

    /// Atomically adds a non-negative integer delta.
    pub async fn increment(&self, name: &str, key: &str, delta: i64) -> DddResult<i64> {
        self.required_cache(name)?
            .increment(CacheKey::new(name, key), delta, None)
            .await
    }

    /// Atomically subtracts a non-negative integer delta.
    pub async fn decrement(&self, name: &str, key: &str, delta: i64) -> DddResult<i64> {
        self.required_cache(name)?
            .decrement(CacheKey::new(name, key), delta, None)
            .await
    }

    /// Atomically adds a non-negative floating-point delta.
    pub async fn increment_float(&self, name: &str, key: &str, delta: f64) -> DddResult<f64> {
        self.required_cache(name)?
            .increment_float(CacheKey::new(name, key), delta, None)
            .await
    }

    /// Atomically subtracts a non-negative floating-point delta.
    pub async fn decrement_float(&self, name: &str, key: &str, delta: f64) -> DddResult<f64> {
        self.required_cache(name)?
            .decrement_float(CacheKey::new(name, key), delta, None)
            .await
    }

    /// Atomically deducts stock with the frozen ddd4j return-code contract.
    pub async fn stock_decrement(&self, name: &str, key: &str, quantity: i64) -> DddResult<i64> {
        self.required_cache(name)?
            .stock_decrement(CacheKey::new(name, key), quantity)
            .await
    }

    /// Atomically restores initialized stock.
    pub async fn stock_increment(&self, name: &str, key: &str, quantity: i64) -> DddResult<i64> {
        self.required_cache(name)?
            .stock_increment(CacheKey::new(name, key), quantity)
            .await
    }

    /// Changes a key's TTL.
    pub async fn expire(&self, name: &str, key: &str, ttl: Duration) -> DddResult<bool> {
        self.required_cache(name)?
            .expire(&CacheKey::new(name, key), ttl)
            .await
    }

    /// Returns remaining TTL using a typed Rust representation.
    pub async fn remaining_ttl(
        &self,
        name: &str,
        key: &str,
    ) -> DddResult<Option<Option<Duration>>> {
        self.required_cache(name)?
            .remaining_ttl(&CacheKey::new(name, key))
            .await
    }

    /// Removes expiration from a key.
    pub async fn persist(&self, name: &str, key: &str) -> DddResult<bool> {
        self.required_cache(name)?
            .persist(&CacheKey::new(name, key))
            .await
    }

    /// Refreshes one loading-cache key immediately.
    pub async fn refresh(&self, name: &str, key: &str) -> DddResult<Option<CacheValue>> {
        self.invalidate(name, key).await?;
        self.get(name, key).await
    }

    /// Returns cache statistics when the cache exists.
    pub fn stats(&self, name: &str) -> Option<CacheStats> {
        self.cache(name).map(|cache| cache.stats())
    }

    /// Acquires a lease, runs an async operation, then owner-safely releases it.
    pub async fn with_lock<T, F, Fut>(
        &self,
        name: &str,
        key: &str,
        wait: Duration,
        lease: Duration,
        operation: F,
    ) -> DddResult<Option<T>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = DddResult<T>>,
    {
        let Some(cache) = self.cache(name) else {
            return Ok(None);
        };
        let Some(lock) = cache
            .try_lock(CacheKey::new(name, key), wait, lease)
            .await?
        else {
            return Ok(None);
        };
        let result = operation().await;
        let unlocked = cache.unlock(&lock).await?;
        if !unlocked {
            return Err(DddError::Adapter {
                adapter: "cache-kit",
                message: "lock lease expired or ownership changed before unlock".to_owned(),
            });
        }
        result.map(Some)
    }

    /// Releases a previously acquired lease.
    pub async fn unlock(&self, name: &str, lease: &CacheLockLease) -> DddResult<bool> {
        let Some(cache) = self.cache(name) else {
            return Ok(false);
        };
        cache.unlock(lease).await
    }

    fn required_cache(&self, name: &str) -> DddResult<Arc<dyn Cache>> {
        self.cache(name).ok_or_else(|| DddError::ServiceNotFound {
            key: name.to_owned(),
            type_name: "ddd4r_cache::Cache",
        })
    }
}
