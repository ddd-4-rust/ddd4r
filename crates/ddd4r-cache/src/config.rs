//! Cache configuration shared by managers and adapters.

use std::time::Duration;

use ddd4r_core::{DddError, DddResult};

/// Cache deployment topology.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CacheType {
    /// Process-local cache.
    #[default]
    Local,
    /// Shared remote cache.
    Remote,
    /// Local and remote multi-level cache.
    Both,
}

/// Java migration selector for `CacheKit::build`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LocalCacheType {
    /// Caffeine-compatible behavior, implemented with the native Rust cache.
    #[default]
    Caffeine,
    /// Guava migration behavior, implemented with the native Rust cache.
    Guava,
    /// Hutool migration behavior, implemented with the native Rust cache.
    Hutool,
}

/// Immutable cache policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheConfig {
    /// Logical cache name.
    pub name: String,
    /// Maximum number of entries. Zero means unbounded.
    pub maximum_size: u64,
    /// Time-to-live measured from a write.
    pub expire_after_write: Option<Duration>,
    /// Time-to-idle measured from the latest successful read.
    pub expire_after_access: Option<Duration>,
    /// Refresh interval for loading caches.
    pub refresh_after_write: Option<Duration>,
    /// Initial allocation hint.
    pub initial_capacity: usize,
    /// Whether the caller requested statistics.
    pub record_stats: bool,
    /// Deployment topology.
    pub cache_type: CacheType,
    /// Local entry limit for multi-level caches.
    pub local_limit: usize,
    /// Whether remote invalidation should be broadcast to local caches.
    pub sync_local: bool,
}

impl CacheConfig {
    /// Starts a validated configuration builder.
    pub fn builder(name: impl Into<String>) -> DddResult<CacheConfigBuilder> {
        CacheConfigBuilder::new(name)
    }
}

/// Builder matching the frozen ddd4j `CacheConfig` surface.
#[derive(Debug, Clone)]
pub struct CacheConfigBuilder {
    config: CacheConfig,
}

impl CacheConfigBuilder {
    /// Creates a builder and rejects an empty logical cache name.
    pub fn new(name: impl Into<String>) -> DddResult<Self> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(DddError::Adapter {
                adapter: "cache",
                message: "cache name must not be blank".to_owned(),
            });
        }
        Ok(Self {
            config: CacheConfig {
                name,
                maximum_size: 10_000,
                expire_after_write: None,
                expire_after_access: None,
                refresh_after_write: None,
                initial_capacity: 0,
                record_stats: false,
                cache_type: CacheType::Local,
                local_limit: 100,
                sync_local: false,
            },
        })
    }

    /// Sets the maximum number of entries.
    pub const fn maximum_size(mut self, maximum_size: u64) -> Self {
        self.config.maximum_size = maximum_size;
        self
    }

    /// Sets write TTL.
    pub const fn expire_after_write(mut self, duration: Duration) -> Self {
        self.config.expire_after_write = Some(duration);
        self
    }

    /// Sets access TTI.
    pub const fn expire_after_access(mut self, duration: Duration) -> Self {
        self.config.expire_after_access = Some(duration);
        self
    }

    /// Sets loading-cache refresh interval.
    pub const fn refresh_after_write(mut self, duration: Duration) -> Self {
        self.config.refresh_after_write = Some(duration);
        self
    }

    /// Sets initial allocation hint.
    pub const fn initial_capacity(mut self, initial_capacity: usize) -> Self {
        self.config.initial_capacity = initial_capacity;
        self
    }

    /// Enables or disables statistics collection.
    pub const fn record_stats(mut self, record_stats: bool) -> Self {
        self.config.record_stats = record_stats;
        self
    }

    /// Sets deployment topology.
    pub const fn cache_type(mut self, cache_type: CacheType) -> Self {
        self.config.cache_type = cache_type;
        self
    }

    /// Sets local limit for a multi-level cache.
    pub const fn local_limit(mut self, local_limit: usize) -> Self {
        self.config.local_limit = local_limit;
        self
    }

    /// Enables or disables local invalidation broadcasts.
    pub const fn sync_local(mut self, sync_local: bool) -> Self {
        self.config.sync_local = sync_local;
        self
    }

    /// Builds an immutable configuration.
    pub fn build(self) -> DddResult<CacheConfig> {
        if self.config.maximum_size == 0 && self.config.cache_type == CacheType::Both {
            return Err(DddError::Adapter {
                adapter: "cache",
                message: "multi-level cache maximum_size must be positive".to_owned(),
            });
        }
        Ok(self.config)
    }
}
