use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_symbols::StaticStorageDuration;

/// A retained static's lifecycle contribution to the final linked image.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativeStatic {
    symbol: NonEmptySharedStr,
    identity: [u8; 32],
    order_key: Arc<[u8]>,
    duration: StaticStorageDuration,
    dependencies: Arc<[[u8; 32]]>,
    requires_host: bool,
    requires_main_thread: bool,
}

impl NativeStatic {
    /// Creates a contribution naming an existing native static-host entry.
    pub fn new(
        symbol: NonEmptySharedStr,
        identity: [u8; 32],
        order_key: Arc<[u8]>,
        duration: StaticStorageDuration,
        dependencies: impl IntoIterator<Item = [u8; 32]>,
        requires_host: bool,
        requires_main_thread: bool,
    ) -> Self {
        let mut dependencies = dependencies.into_iter().collect::<Vec<_>>();

        dependencies.sort_unstable();
        dependencies.dedup();

        Self {
            symbol,
            identity,
            order_key,
            duration,
            dependencies: dependencies.into(),
            requires_host,
            requires_main_thread,
        }
    }

    /// Returns the native static-host entry symbol.
    pub fn symbol(&self) -> &str {
        self.symbol.as_str()
    }

    /// Returns the portable static identity.
    pub const fn identity(&self) -> [u8; 32] {
        self.identity
    }

    /// Returns the portable structural cleanup ordering key.
    pub fn order_key(&self) -> &[u8] {
        &self.order_key
    }

    /// Returns the storage lifetime domain.
    pub const fn duration(&self) -> StaticStorageDuration {
        self.duration
    }

    /// Returns statics that must remain alive until this static is cleaned up.
    pub fn dependencies(&self) -> &[[u8; 32]] {
        &self.dependencies
    }

    /// Returns whether this static needs lifecycle hosting in an executable or test product.
    pub const fn requires_host(&self) -> bool {
        self.requires_host
    }

    /// Returns whether cleanup must run on the main thread.
    pub const fn requires_main_thread(&self) -> bool {
        self.requires_main_thread
    }
}
