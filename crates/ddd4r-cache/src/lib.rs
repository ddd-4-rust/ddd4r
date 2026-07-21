//! Application-level cache contracts, separate from `RBatis` second-level caches.

#![forbid(unsafe_code)]

mod config;
mod kit;
mod memory;
mod model;

pub use config::{CacheConfig, CacheConfigBuilder, CacheType, LocalCacheType};
pub use kit::{CacheKit, CacheLoader};
pub use memory::InMemoryCache;
pub use model::{
    Cache, CacheCapabilities, CacheKey, CacheLockLease, CacheStats, CacheValue,
    STOCK_ILLEGAL_ARGUMENT, STOCK_NOT_ENOUGH, STOCK_NOT_INITIALIZED, STOCK_ZERO,
};

/// Java migration name for the default Rust local cache.
pub type CaffeineCache = InMemoryCache;
/// Java migration name; Rust uses the same safe local implementation.
pub type GuavaCache = InMemoryCache;
/// Java migration name; Rust uses the same safe local implementation.
pub type HutoolCache = InMemoryCache;
