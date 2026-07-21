//! Object-safe application cache SPI.

use std::time::Duration;

use ddd4r_core::{DddError, DddResult};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stock is initialized but zero.
pub const STOCK_ZERO: i64 = -1;
/// Requested quantity exceeds stock.
pub const STOCK_NOT_ENOUGH: i64 = -2;
/// Stock key does not exist.
pub const STOCK_NOT_INITIALIZED: i64 = -3;
/// Quantity is not positive.
pub const STOCK_ILLEGAL_ARGUMENT: i64 = -4;

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
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CacheStats {
    /// Successful reads.
    pub hits: u64,
    /// Missing or expired reads.
    pub misses: u64,
    /// Loader executions.
    pub loads: u64,
    /// Writes.
    pub puts: u64,
    /// Explicit or expiry removals.
    pub removals: u64,
    /// Capacity evictions.
    pub evictions: u64,
}

impl CacheStats {
    /// Returns the hit ratio, or zero before the first lookup.
    #[allow(clippy::cast_precision_loss)]
    pub fn hit_rate(self) -> f64 {
        let requests = self.hits.saturating_add(self.misses);
        if requests == 0 {
            0.0
        } else {
            self.hits as f64 / requests as f64
        }
    }
}

/// Optional operation support advertised by a backend.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheCapabilities {
    /// Compare-and-set operations.
    pub cas: bool,
    /// Atomic integer and floating-point operations.
    pub counters: bool,
    /// Per-key TTL management.
    pub ttl: bool,
    /// Lease-based locking.
    pub locking: bool,
}

/// Ownership token returned by a successful lock acquisition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheLockLease {
    /// Lock key.
    pub key: CacheKey,
    /// Unforgeable owner identifier.
    pub owner: Uuid,
}

/// Application cache SPI. Optional operations fail explicitly by default.
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

    /// Returns the estimated number of live entries.
    fn estimated_size(&self) -> u64;

    /// Returns a point-in-time statistics snapshot.
    fn stats(&self) -> CacheStats;

    /// Records a loader execution for backends that expose load statistics.
    #[doc(hidden)]
    fn record_load(&self) {}

    /// Advertises optional operations.
    fn capabilities(&self) -> CacheCapabilities {
        CacheCapabilities::default()
    }

    /// Inserts only when a live value is absent.
    fn put_if_absent(
        &self,
        _key: CacheKey,
        _bytes: Vec<u8>,
        _ttl: Option<Duration>,
    ) -> BoxFuture<'_, DddResult<Option<CacheValue>>> {
        Box::pin(async { Err(DddError::unsupported("cache.put_if_absent")) })
    }

    /// Replaces only when the current bytes match.
    fn replace(
        &self,
        _key: CacheKey,
        _expected: Vec<u8>,
        _bytes: Vec<u8>,
        _ttl: Option<Duration>,
    ) -> BoxFuture<'_, DddResult<Option<CacheValue>>> {
        Box::pin(async { Err(DddError::unsupported("cache.replace")) })
    }

    /// Removes only when the current bytes match.
    fn remove_if(&self, _key: CacheKey, _expected: Vec<u8>) -> BoxFuture<'_, DddResult<bool>> {
        Box::pin(async { Err(DddError::unsupported("cache.remove_if")) })
    }

    /// Replaces only when the current backend version matches.
    fn compare_and_set(
        &self,
        _key: CacheKey,
        _expected_version: u64,
        _bytes: Vec<u8>,
        _ttl: Option<Duration>,
    ) -> BoxFuture<'_, DddResult<Option<CacheValue>>> {
        Box::pin(async { Err(DddError::unsupported("cache.compare_and_set")) })
    }

    /// Changes the expiration of a live key.
    fn expire<'a>(&'a self, _key: &'a CacheKey, _ttl: Duration) -> BoxFuture<'a, DddResult<bool>> {
        Box::pin(async { Err(DddError::unsupported("cache.expire")) })
    }

    /// Returns remaining TTL: `None` for a persistent key, outer `None` for a missing key.
    fn remaining_ttl<'a>(
        &'a self,
        _key: &'a CacheKey,
    ) -> BoxFuture<'a, DddResult<Option<Option<Duration>>>> {
        Box::pin(async { Err(DddError::unsupported("cache.remaining_ttl")) })
    }

    /// Removes expiration from a live key.
    fn persist<'a>(&'a self, _key: &'a CacheKey) -> BoxFuture<'a, DddResult<bool>> {
        Box::pin(async { Err(DddError::unsupported("cache.persist")) })
    }

    /// Atomically increments an integer encoded as an ASCII decimal.
    fn increment(
        &self,
        _key: CacheKey,
        _delta: i64,
        _ttl_on_create: Option<Duration>,
    ) -> BoxFuture<'_, DddResult<i64>> {
        Box::pin(async { Err(DddError::unsupported("cache.increment")) })
    }

    /// Atomically decrements an integer encoded as an ASCII decimal.
    fn decrement(
        &self,
        _key: CacheKey,
        _delta: i64,
        _ttl_on_create: Option<Duration>,
    ) -> BoxFuture<'_, DddResult<i64>> {
        Box::pin(async { Err(DddError::unsupported("cache.decrement")) })
    }

    /// Atomically increments a float encoded as an ASCII decimal.
    fn increment_float(
        &self,
        _key: CacheKey,
        _delta: f64,
        _ttl_on_create: Option<Duration>,
    ) -> BoxFuture<'_, DddResult<f64>> {
        Box::pin(async { Err(DddError::unsupported("cache.increment_float")) })
    }

    /// Atomically decrements a float encoded as an ASCII decimal.
    fn decrement_float(
        &self,
        _key: CacheKey,
        _delta: f64,
        _ttl_on_create: Option<Duration>,
    ) -> BoxFuture<'_, DddResult<f64>> {
        Box::pin(async { Err(DddError::unsupported("cache.decrement_float")) })
    }

    /// Deducts stock without allowing a negative stored value.
    fn stock_decrement(&self, _key: CacheKey, _quantity: i64) -> BoxFuture<'_, DddResult<i64>> {
        Box::pin(async { Err(DddError::unsupported("cache.stock_decrement")) })
    }

    /// Adds stock to an initialized counter.
    fn stock_increment(&self, _key: CacheKey, _quantity: i64) -> BoxFuture<'_, DddResult<i64>> {
        Box::pin(async { Err(DddError::unsupported("cache.stock_increment")) })
    }

    /// Acquires an owner-checked lease.
    fn try_lock(
        &self,
        _key: CacheKey,
        _wait: Duration,
        _lease: Duration,
    ) -> BoxFuture<'_, DddResult<Option<CacheLockLease>>> {
        Box::pin(async { Err(DddError::unsupported("cache.try_lock")) })
    }

    /// Releases a lease only when the owner token matches.
    fn unlock<'a>(&'a self, _lease: &'a CacheLockLease) -> BoxFuture<'a, DddResult<bool>> {
        Box::pin(async { Err(DddError::unsupported("cache.unlock")) })
    }
}
