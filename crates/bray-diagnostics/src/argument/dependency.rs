use super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an unavailable dependency subject category argument.
    pub const fn dependency_subject_kind(kind: DiagnosticDependencySubjectKind) -> Self {
        Self::new(
            DiagnosticArgName::DependencySubjectKind,
            DiagnosticArgValue::DependencySubjectKind(kind),
        )
    }

    /// Creates an unavailable dependency requirement category argument.
    pub const fn dependency_requirement_kind(kind: DiagnosticDependencyRequirementKind) -> Self {
        Self::new(
            DiagnosticArgName::DependencyRequirementKind,
            DiagnosticArgValue::DependencyRequirementKind(kind),
        )
    }
}

/// Locale-neutral dependency subject category retained by await diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticDependencySubjectKind {
    Storage,
    StorageAccess,
    BorrowCapability,
    ScopedCapability,
    SelectedImplementation,
    LifecycleObligation,
    SuspensionState,
}

impl DiagnosticDependencySubjectKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Storage => "storage",
            Self::StorageAccess => "storage_access",
            Self::BorrowCapability => "borrow_capability",
            Self::ScopedCapability => "scoped_capability",
            Self::SelectedImplementation => "selected_implementation",
            Self::LifecycleObligation => "lifecycle_obligation",
            Self::SuspensionState => "suspension_state",
        }
    }
}

/// Locale-neutral state required from an await dependency.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticDependencyRequirementKind {
    StorageAlive,
    StorageInitialized,
    SharedBorrowActive,
    MutableBorrowActive,
    ExclusiveMutationAuthority,
    ScopedCapabilityLive,
    DestructionAttached,
    FinalizationAttached,
    CancellationAttached,
    JoiningAttached,
    SuspensionStateAvailable,
}

impl DiagnosticDependencyRequirementKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StorageAlive => "storage_alive",
            Self::StorageInitialized => "storage_initialized",
            Self::SharedBorrowActive => "shared_borrow_active",
            Self::MutableBorrowActive => "mutable_borrow_active",
            Self::ExclusiveMutationAuthority => "exclusive_mutation_authority",
            Self::ScopedCapabilityLive => "scoped_capability_live",
            Self::DestructionAttached => "destruction_attached",
            Self::FinalizationAttached => "finalization_attached",
            Self::CancellationAttached => "cancellation_attached",
            Self::JoiningAttached => "joining_attached",
            Self::SuspensionStateAvailable => "suspension_state_available",
        }
    }
}
