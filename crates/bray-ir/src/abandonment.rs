/// One ownership step in the abnormal-exit fallback for a retained cleanup error.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirAbandonmentAction {
    /// Terminates owned runs while retaining initialized payloads and inactive captures.
    Quiesce,
    /// Destroys an already quiescent value and its initialized represented parts.
    Destroy,
    /// Invokes a consuming source destructor with abandonment cleanup for its receiver remainder.
    Destructor,
}

impl MirAbandonmentAction {
    /// Returns the execution mode required by this ownership step.
    pub const fn execution(
        self,
        cleanup: &bray_bound_tree::StorageCleanupType,
    ) -> Option<bray_symbols::CallableExecution> {
        match self {
            Self::Quiesce => cleanup.quiescence_execution(),
            Self::Destroy | Self::Destructor => Some(bray_symbols::CallableExecution::Synchronous),
        }
    }
}
