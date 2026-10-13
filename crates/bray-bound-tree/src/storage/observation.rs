/// Identity of a checked operation that creates a new contents observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageMutation {
    /// One explicit or scoped call.
    Operation(crate::BoundExecutionSite),
    /// One lifecycle invocation within an ordered cleanup.
    Lifecycle {
        /// Source boundary containing the invocation.
        occurrence: crate::BoundExecutionSite,
        /// Owner being cleaned up.
        access: crate::StorageAccessId,
        /// Prepared component within that owner.
        part: usize,
        /// Selected lifecycle phase.
        slot: bray_symbols::TypeAssociatedLifecycleSlot,
    },
}

impl StorageMutation {
    /// Returns the enclosing source occurrence.
    pub const fn occurrence(self) -> crate::BoundExecutionSite {
        match self {
            Self::Operation(occurrence) | Self::Lifecycle { occurrence, .. } => occurrence,
        }
    }
}
