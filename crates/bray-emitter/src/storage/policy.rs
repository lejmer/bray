use std::time::{Duration, SystemTime};

/// Machine-local lifetimes and byte budget for optional build storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoragePolicy {
    /// Retire a product namespace after this long without a build or retained run.
    pub inactive_product_age: Duration,
    /// Evict an optional cache after this long without use.
    pub cache_max_age: Duration,
    /// Maximum retained bytes across optional caches in one output root.
    pub cache_max_bytes: u64,
}

impl Default for StoragePolicy {
    fn default() -> Self {
        Self {
            inactive_product_age: Duration::from_secs(30 * 24 * 60 * 60),
            cache_max_age: Duration::from_secs(7 * 24 * 60 * 60),
            cache_max_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
}

pub(super) fn elapsed(now: SystemTime, last_used: SystemTime) -> Duration {
    // A wall-clock rollback must not make recently used state appear expired.
    now.duration_since(last_used).unwrap_or_default()
}
