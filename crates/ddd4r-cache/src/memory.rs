//! Safe process-local implementation of the complete cache contract.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use ddd4r_core::{DddError, DddResult};
use futures::future::BoxFuture;
use uuid::Uuid;

use crate::{
    Cache, CacheCapabilities, CacheConfig, CacheKey, CacheLockLease, CacheStats, CacheValue,
    STOCK_ILLEGAL_ARGUMENT, STOCK_NOT_ENOUGH, STOCK_NOT_INITIALIZED, STOCK_ZERO,
};

#[derive(Debug, Clone)]
struct StoredValue {
    value: CacheValue,
    written_at: Instant,
    accessed_at: Instant,
    expires_at: Option<Instant>,
}

#[derive(Debug, Clone)]
struct StoredLock {
    owner: Uuid,
    expires_at: Instant,
}

#[derive(Default)]
struct Counters {
    hits: AtomicU64,
    misses: AtomicU64,
    loads: AtomicU64,
    puts: AtomicU64,
    removals: AtomicU64,
    evictions: AtomicU64,
}

/// Process-local cache useful for production local caching and tests.
#[derive(Clone)]
pub struct InMemoryCache {
    config: CacheConfig,
    values: Arc<DashMap<CacheKey, StoredValue>>,
    locks: Arc<DashMap<CacheKey, StoredLock>>,
    versions: Arc<AtomicU64>,
    counters: Arc<Counters>,
}

impl Default for InMemoryCache {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryCache {
    /// Creates an empty cache with the default policy.
    pub fn new() -> Self {
        let config = CacheConfig {
            name: "default".to_owned(),
            maximum_size: 10_000,
            expire_after_write: None,
            expire_after_access: None,
            refresh_after_write: None,
            initial_capacity: 0,
            record_stats: false,
            cache_type: crate::CacheType::Local,
            local_limit: 100,
            sync_local: false,
        };
        Self::with_config(config)
    }

    /// Creates an empty cache from an immutable policy.
    pub fn with_config(config: CacheConfig) -> Self {
        Self {
            values: Arc::new(DashMap::with_capacity(config.initial_capacity)),
            locks: Arc::new(DashMap::new()),
            config,
            versions: Arc::new(AtomicU64::new(0)),
            counters: Arc::new(Counters::default()),
        }
    }

    /// Returns the configured policy.
    pub const fn config(&self) -> &CacheConfig {
        &self.config
    }

    fn next_version(&self) -> u64 {
        self.versions
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1)
    }

    fn adapter_error(message: impl Into<String>) -> DddError {
        DddError::Adapter {
            adapter: "in-memory-cache",
            message: message.into(),
        }
    }

    fn expiry(&self, ttl: Option<Duration>, now: Instant) -> Option<Instant> {
        ttl.or(self.config.expire_after_write)
            .and_then(|duration| now.checked_add(duration))
    }

    fn is_expired(&self, value: &StoredValue, now: Instant) -> bool {
        let write_expired = value.expires_at.is_some_and(|expiry| expiry <= now);
        let idle_expired = self
            .config
            .expire_after_access
            .and_then(|tti| value.accessed_at.checked_add(tti))
            .is_some_and(|expiry| expiry <= now);
        write_expired || idle_expired
    }

    fn stored(&self, bytes: Vec<u8>, ttl: Option<Duration>) -> StoredValue {
        let now = Instant::now();
        StoredValue {
            value: CacheValue {
                bytes,
                version: self.next_version(),
            },
            written_at: now,
            accessed_at: now,
            expires_at: self.expiry(ttl, now),
        }
    }

    fn live_value(&self, key: &CacheKey) -> Option<CacheValue> {
        let now = Instant::now();
        let mut entry = self.values.get_mut(key)?;
        if self.is_expired(&entry, now) {
            drop(entry);
            self.values.remove(key);
            self.counters.removals.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        entry.accessed_at = now;
        Some(entry.value.clone())
    }

    fn evict_if_needed(&self) {
        let maximum_size = self.config.maximum_size;
        while maximum_size > 0 && self.values.len() as u64 > maximum_size {
            let oldest = self
                .values
                .iter()
                .min_by_key(|entry| entry.written_at)
                .map(|entry| entry.key().clone());
            let Some(oldest) = oldest else {
                break;
            };
            if self.values.remove(&oldest).is_some() {
                self.counters.evictions.fetch_add(1, Ordering::Relaxed);
                self.counters.removals.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn remove_expired_entries(&self) {
        let now = Instant::now();
        let before = self.values.len();
        self.values.retain(|_, value| !self.is_expired(value, now));
        let removed = before.saturating_sub(self.values.len());
        self.counters.removals.fetch_add(
            u64::try_from(removed).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }

    fn parse_i64(bytes: &[u8]) -> DddResult<i64> {
        std::str::from_utf8(bytes)
            .map_err(|_| Self::adapter_error("counter is not UTF-8"))?
            .parse::<i64>()
            .map_err(|_| Self::adapter_error("counter is not an i64"))
    }

    fn parse_f64(bytes: &[u8]) -> DddResult<f64> {
        std::str::from_utf8(bytes)
            .map_err(|_| Self::adapter_error("counter is not UTF-8"))?
            .parse::<f64>()
            .map_err(|_| Self::adapter_error("counter is not an f64"))
    }

    fn count_load(&self) {
        self.counters.loads.fetch_add(1, Ordering::Relaxed);
    }
}

impl Cache for InMemoryCache {
    fn get<'a>(&'a self, key: &'a CacheKey) -> BoxFuture<'a, DddResult<Option<CacheValue>>> {
        Box::pin(async move {
            let value = self.live_value(key);
            if value.is_some() {
                self.counters.hits.fetch_add(1, Ordering::Relaxed);
            } else {
                self.counters.misses.fetch_add(1, Ordering::Relaxed);
            }
            Ok(value)
        })
    }

    fn put(
        &self,
        key: CacheKey,
        bytes: Vec<u8>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'_, DddResult<CacheValue>> {
        Box::pin(async move {
            let stored = self.stored(bytes, ttl);
            let value = stored.value.clone();
            self.values.insert(key, stored);
            self.counters.puts.fetch_add(1, Ordering::Relaxed);
            self.evict_if_needed();
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
            let mut removed = 0_u64;
            for key in keys {
                if self.values.remove(&key).is_some() {
                    removed = removed.saturating_add(1);
                }
            }
            self.counters.removals.fetch_add(removed, Ordering::Relaxed);
            Ok(removed)
        })
    }

    fn estimated_size(&self) -> u64 {
        self.remove_expired_entries();
        u64::try_from(self.values.len()).unwrap_or(u64::MAX)
    }

    fn stats(&self) -> CacheStats {
        CacheStats {
            hits: self.counters.hits.load(Ordering::Relaxed),
            misses: self.counters.misses.load(Ordering::Relaxed),
            loads: self.counters.loads.load(Ordering::Relaxed),
            puts: self.counters.puts.load(Ordering::Relaxed),
            removals: self.counters.removals.load(Ordering::Relaxed),
            evictions: self.counters.evictions.load(Ordering::Relaxed),
        }
    }

    fn record_load(&self) {
        self.count_load();
    }

    fn capabilities(&self) -> CacheCapabilities {
        CacheCapabilities {
            cas: true,
            counters: true,
            ttl: true,
            locking: true,
        }
    }

    fn put_if_absent(
        &self,
        key: CacheKey,
        bytes: Vec<u8>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'_, DddResult<Option<CacheValue>>> {
        Box::pin(async move {
            if self.live_value(&key).is_some() {
                return Ok(None);
            }
            let stored = self.stored(bytes, ttl);
            let value = stored.value.clone();
            match self.values.entry(key) {
                Entry::Occupied(mut occupied)
                    if self.is_expired(occupied.get(), Instant::now()) =>
                {
                    occupied.insert(stored);
                }
                Entry::Occupied(_) => return Ok(None),
                Entry::Vacant(vacant) => {
                    vacant.insert(stored);
                }
            }
            self.counters.puts.fetch_add(1, Ordering::Relaxed);
            self.evict_if_needed();
            Ok(Some(value))
        })
    }

    fn replace(
        &self,
        key: CacheKey,
        expected: Vec<u8>,
        bytes: Vec<u8>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'_, DddResult<Option<CacheValue>>> {
        Box::pin(async move {
            let Some(mut entry) = self.values.get_mut(&key) else {
                return Ok(None);
            };
            if self.is_expired(&entry, Instant::now()) || entry.value.bytes != expected {
                return Ok(None);
            }
            let stored = self.stored(bytes, ttl);
            let value = stored.value.clone();
            *entry = stored;
            self.counters.puts.fetch_add(1, Ordering::Relaxed);
            Ok(Some(value))
        })
    }

    fn remove_if(&self, key: CacheKey, expected: Vec<u8>) -> BoxFuture<'_, DddResult<bool>> {
        Box::pin(async move {
            let removed = self
                .values
                .remove_if(&key, |_, value| {
                    !self.is_expired(value, Instant::now()) && value.value.bytes == expected
                })
                .is_some();
            if removed {
                self.counters.removals.fetch_add(1, Ordering::Relaxed);
            }
            Ok(removed)
        })
    }

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
            if self.is_expired(&entry, Instant::now()) || entry.value.version != expected_version {
                return Ok(None);
            }
            let stored = self.stored(bytes, ttl);
            let value = stored.value.clone();
            *entry = stored;
            self.counters.puts.fetch_add(1, Ordering::Relaxed);
            Ok(Some(value))
        })
    }

    fn expire<'a>(&'a self, key: &'a CacheKey, ttl: Duration) -> BoxFuture<'a, DddResult<bool>> {
        Box::pin(async move {
            let Some(mut entry) = self.values.get_mut(key) else {
                return Ok(false);
            };
            if self.is_expired(&entry, Instant::now()) {
                return Ok(false);
            }
            entry.expires_at = Instant::now().checked_add(ttl);
            Ok(true)
        })
    }

    fn remaining_ttl<'a>(
        &'a self,
        key: &'a CacheKey,
    ) -> BoxFuture<'a, DddResult<Option<Option<Duration>>>> {
        Box::pin(async move {
            let now = Instant::now();
            let Some(entry) = self.values.get(key) else {
                return Ok(None);
            };
            if self.is_expired(&entry, now) {
                drop(entry);
                self.values.remove(key);
                self.counters.removals.fetch_add(1, Ordering::Relaxed);
                return Ok(None);
            }
            Ok(Some(entry.expires_at.map(|expiry| {
                expiry.checked_duration_since(now).unwrap_or(Duration::ZERO)
            })))
        })
    }

    fn persist<'a>(&'a self, key: &'a CacheKey) -> BoxFuture<'a, DddResult<bool>> {
        Box::pin(async move {
            let Some(mut entry) = self.values.get_mut(key) else {
                return Ok(false);
            };
            if self.is_expired(&entry, Instant::now()) {
                return Ok(false);
            }
            entry.expires_at = None;
            Ok(true)
        })
    }

    fn increment(
        &self,
        key: CacheKey,
        delta: i64,
        ttl_on_create: Option<Duration>,
    ) -> BoxFuture<'_, DddResult<i64>> {
        Box::pin(async move {
            if delta < 0 {
                return Err(Self::adapter_error("increment delta must not be negative"));
            }
            let result = match self.values.entry(key) {
                Entry::Occupied(mut entry) => {
                    if self.is_expired(entry.get(), Instant::now()) {
                        entry.insert(self.stored(delta.to_string().into_bytes(), ttl_on_create));
                        self.counters.puts.fetch_add(1, Ordering::Relaxed);
                        return Ok(delta);
                    }
                    let current = Self::parse_i64(&entry.get().value.bytes)?;
                    let next = current
                        .checked_add(delta)
                        .ok_or_else(|| Self::adapter_error("integer counter overflow"))?;
                    let expires_at = entry.get().expires_at;
                    let mut stored = self.stored(next.to_string().into_bytes(), None);
                    stored.expires_at = expires_at;
                    entry.insert(stored);
                    next
                }
                Entry::Vacant(entry) => {
                    entry.insert(self.stored(delta.to_string().into_bytes(), ttl_on_create));
                    delta
                }
            };
            self.counters.puts.fetch_add(1, Ordering::Relaxed);
            Ok(result)
        })
    }

    fn decrement(
        &self,
        key: CacheKey,
        delta: i64,
        ttl_on_create: Option<Duration>,
    ) -> BoxFuture<'_, DddResult<i64>> {
        Box::pin(async move {
            if delta < 0 {
                return Err(Self::adapter_error("decrement delta must not be negative"));
            }
            let initial = delta
                .checked_neg()
                .ok_or_else(|| Self::adapter_error("integer counter underflow"))?;
            let result = match self.values.entry(key) {
                Entry::Occupied(mut entry) => {
                    if self.is_expired(entry.get(), Instant::now()) {
                        entry.insert(self.stored(initial.to_string().into_bytes(), ttl_on_create));
                        self.counters.puts.fetch_add(1, Ordering::Relaxed);
                        return Ok(initial);
                    }
                    let current = Self::parse_i64(&entry.get().value.bytes)?;
                    let next = current
                        .checked_sub(delta)
                        .ok_or_else(|| Self::adapter_error("integer counter underflow"))?;
                    let expires_at = entry.get().expires_at;
                    let mut stored = self.stored(next.to_string().into_bytes(), None);
                    stored.expires_at = expires_at;
                    entry.insert(stored);
                    next
                }
                Entry::Vacant(entry) => {
                    entry.insert(self.stored(initial.to_string().into_bytes(), ttl_on_create));
                    initial
                }
            };
            self.counters.puts.fetch_add(1, Ordering::Relaxed);
            Ok(result)
        })
    }

    fn increment_float(
        &self,
        key: CacheKey,
        delta: f64,
        ttl_on_create: Option<Duration>,
    ) -> BoxFuture<'_, DddResult<f64>> {
        Box::pin(async move {
            if !delta.is_finite() || delta < 0.0 {
                return Err(Self::adapter_error(
                    "floating counter delta must be finite and non-negative",
                ));
            }
            let result = match self.values.entry(key) {
                Entry::Occupied(mut entry) => {
                    if self.is_expired(entry.get(), Instant::now()) {
                        entry.insert(self.stored(delta.to_string().into_bytes(), ttl_on_create));
                        self.counters.puts.fetch_add(1, Ordering::Relaxed);
                        return Ok(delta);
                    }
                    let current = Self::parse_f64(&entry.get().value.bytes)?;
                    let next = current + delta;
                    if !next.is_finite() {
                        return Err(Self::adapter_error("floating counter overflow"));
                    }
                    let expires_at = entry.get().expires_at;
                    let mut stored = self.stored(next.to_string().into_bytes(), None);
                    stored.expires_at = expires_at;
                    entry.insert(stored);
                    next
                }
                Entry::Vacant(entry) => {
                    entry.insert(self.stored(delta.to_string().into_bytes(), ttl_on_create));
                    delta
                }
            };
            self.counters.puts.fetch_add(1, Ordering::Relaxed);
            Ok(result)
        })
    }

    fn decrement_float(
        &self,
        key: CacheKey,
        delta: f64,
        ttl_on_create: Option<Duration>,
    ) -> BoxFuture<'_, DddResult<f64>> {
        Box::pin(async move {
            if !delta.is_finite() || delta < 0.0 {
                return Err(Self::adapter_error(
                    "floating counter delta must be finite and non-negative",
                ));
            }
            let initial = -delta;
            let result = match self.values.entry(key) {
                Entry::Occupied(mut entry) => {
                    if self.is_expired(entry.get(), Instant::now()) {
                        entry.insert(self.stored(initial.to_string().into_bytes(), ttl_on_create));
                        self.counters.puts.fetch_add(1, Ordering::Relaxed);
                        return Ok(initial);
                    }
                    let current = Self::parse_f64(&entry.get().value.bytes)?;
                    let next = current - delta;
                    if !next.is_finite() {
                        return Err(Self::adapter_error("floating counter overflow"));
                    }
                    let expires_at = entry.get().expires_at;
                    let mut stored = self.stored(next.to_string().into_bytes(), None);
                    stored.expires_at = expires_at;
                    entry.insert(stored);
                    next
                }
                Entry::Vacant(entry) => {
                    entry.insert(self.stored(initial.to_string().into_bytes(), ttl_on_create));
                    initial
                }
            };
            self.counters.puts.fetch_add(1, Ordering::Relaxed);
            Ok(result)
        })
    }

    fn stock_decrement(&self, key: CacheKey, quantity: i64) -> BoxFuture<'_, DddResult<i64>> {
        Box::pin(async move {
            if quantity <= 0 {
                return Ok(STOCK_ILLEGAL_ARGUMENT);
            }
            let Entry::Occupied(mut entry) = self.values.entry(key) else {
                return Ok(STOCK_NOT_INITIALIZED);
            };
            if self.is_expired(entry.get(), Instant::now()) {
                entry.remove();
                self.counters.removals.fetch_add(1, Ordering::Relaxed);
                return Ok(STOCK_NOT_INITIALIZED);
            }
            let current = Self::parse_i64(&entry.get().value.bytes)?;
            if current == 0 {
                return Ok(STOCK_ZERO);
            }
            if current < quantity {
                return Ok(STOCK_NOT_ENOUGH);
            }
            let next = current - quantity;
            entry.get_mut().value.bytes = next.to_string().into_bytes();
            entry.get_mut().value.version = self.next_version();
            self.counters.puts.fetch_add(1, Ordering::Relaxed);
            Ok(next)
        })
    }

    fn stock_increment(&self, key: CacheKey, quantity: i64) -> BoxFuture<'_, DddResult<i64>> {
        Box::pin(async move {
            if quantity <= 0 {
                return Ok(STOCK_ILLEGAL_ARGUMENT);
            }
            let next = match self.values.entry(key) {
                Entry::Occupied(mut entry) => {
                    if self.is_expired(entry.get(), Instant::now()) {
                        entry.insert(self.stored(quantity.to_string().into_bytes(), None));
                        self.counters.puts.fetch_add(1, Ordering::Relaxed);
                        return Ok(quantity);
                    }
                    let current = Self::parse_i64(&entry.get().value.bytes)?;
                    let next = current
                        .checked_add(quantity)
                        .ok_or_else(|| Self::adapter_error("stock overflow"))?;
                    entry.get_mut().value.bytes = next.to_string().into_bytes();
                    entry.get_mut().value.version = self.next_version();
                    next
                }
                Entry::Vacant(entry) => {
                    entry.insert(self.stored(quantity.to_string().into_bytes(), None));
                    quantity
                }
            };
            self.counters.puts.fetch_add(1, Ordering::Relaxed);
            Ok(next)
        })
    }

    fn try_lock(
        &self,
        key: CacheKey,
        wait: Duration,
        lease: Duration,
    ) -> BoxFuture<'_, DddResult<Option<CacheLockLease>>> {
        Box::pin(async move {
            if lease.is_zero() {
                return Err(Self::adapter_error("lock lease must be positive"));
            }
            let deadline = Instant::now()
                .checked_add(wait)
                .unwrap_or_else(Instant::now);
            loop {
                let now = Instant::now();
                let owner = Uuid::now_v7();
                let acquired = match self.locks.entry(key.clone()) {
                    Entry::Vacant(entry) => {
                        entry.insert(StoredLock {
                            owner,
                            expires_at: now.checked_add(lease).unwrap_or(now),
                        });
                        true
                    }
                    Entry::Occupied(mut entry) if entry.get().expires_at <= now => {
                        entry.insert(StoredLock {
                            owner,
                            expires_at: now.checked_add(lease).unwrap_or(now),
                        });
                        true
                    }
                    Entry::Occupied(_) => false,
                };
                if acquired {
                    return Ok(Some(CacheLockLease { key, owner }));
                }
                if wait.is_zero() || Instant::now() >= deadline {
                    return Ok(None);
                }
                tokio::time::sleep(Duration::from_millis(5).min(wait)).await;
            }
        })
    }

    fn unlock<'a>(&'a self, lease: &'a CacheLockLease) -> BoxFuture<'a, DddResult<bool>> {
        Box::pin(async move {
            Ok(self
                .locks
                .remove_if(&lease.key, |_, lock| lock.owner == lease.owner)
                .is_some())
        })
    }
}
