use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::TypeId;

use crate::{AsyncStorageCleanupRequirement, StorageCleanupProjection, StorageProtocolCall};

/// Checked cleanup phases and represented members of one substituted semantic type.
///
/// Members include storage hidden from source-level field lookup. A missing decomposition means
/// that only whole-value cleanup has been checked, not that the type has no represented members.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StorageCleanupType {
    ty: TypeId,
    cleanup: AsyncStorageCleanupRequirement,
    components: Option<Arc<[StorageCleanupProjection]>>,
    release: Option<StorageProtocolCall>,
    requires_whole_value: bool,
}

impl StorageCleanupType {
    /// Records the independently checked whole-value cleanup requirement.
    pub const fn new(ty: TypeId, cleanup: AsyncStorageCleanupRequirement) -> Self {
        Self {
            ty,
            cleanup,
            components: None,
            release: None,
            requires_whole_value: false,
        }
    }

    /// Records all represented components in declaration order and any final policy release.
    /// A declared lifecycle permits decomposition only inside that lifecycle's own receiver.
    pub fn with_components(
        mut self,
        components: impl IntoIterator<Item = StorageCleanupProjection>,
        release: Option<StorageProtocolCall>,
        requires_whole_value: bool,
    ) -> Self {
        self.components = Some(shared_slice(components));
        self.release = release;
        self.requires_whole_value = requires_whole_value;

        self
    }

    /// Returns the substituted semantic type whose storage was checked.
    pub const fn ty(&self) -> TypeId {
        self.ty
    }

    /// Returns the required whole-value cleanup phases.
    pub const fn cleanup(&self) -> AsyncStorageCleanupRequirement {
        self.cleanup
    }

    /// Returns the complete checked decomposition, when one was requested.
    pub fn components(&self) -> Option<&[StorageCleanupProjection]> {
        self.components.as_deref()
    }

    /// Returns the release required after cleaning surviving owned target components.
    pub const fn release(&self) -> Option<StorageProtocolCall> {
        self.release
    }

    /// Returns whether a declared lifecycle forbids decomposition outside its receiver.
    pub const fn requires_whole_value(&self) -> bool {
        self.requires_whole_value
    }
}
