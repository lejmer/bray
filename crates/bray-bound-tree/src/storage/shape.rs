use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{CallableExecution, TypeId};

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
    finalization_execution: Option<CallableExecution>,
    destruction_execution: Option<CallableExecution>,
    quiescence_execution: Option<CallableExecution>,
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
            finalization_execution: None,
            destruction_execution: None,
            quiescence_execution: None,
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

    /// Records independently resolved execution modes. An open type can retain an unresolved mode.
    pub fn with_execution(
        mut self,
        finalization: Option<CallableExecution>,
        destruction: Option<CallableExecution>,
        quiescence: Option<CallableExecution>,
    ) -> Self {
        self.finalization_execution = finalization;
        self.destruction_execution = destruction;
        self.quiescence_execution = quiescence;

        self
    }

    /// Returns the execution mode of the whole-value finalizer.
    pub const fn finalization_execution(&self) -> Option<CallableExecution> {
        self.finalization_execution
    }

    /// Returns the execution mode of destruction, including represented child cleanup.
    pub const fn destruction_execution(&self) -> Option<CallableExecution> {
        self.destruction_execution
    }

    /// Returns the execution mode needed to terminate owned runs while retaining their values.
    pub const fn quiescence_execution(&self) -> Option<CallableExecution> {
        self.quiescence_execution
    }

    /// Returns the execution mode required to finalize and destroy this value.
    pub fn lifecycle_execution(&self) -> Option<CallableExecution> {
        Some(
            self.finalization_execution?
                .max(self.destruction_execution?),
        )
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
