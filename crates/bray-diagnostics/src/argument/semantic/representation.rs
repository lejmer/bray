use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an exact declared-layout failure argument.
    pub fn layout_problem(problem: crate::DiagnosticLayoutProblem) -> Self {
        Self::new(
            DiagnosticArgName::LayoutProblem,
            DiagnosticArgValue::LayoutProblem(problem),
        )
    }

    /// Creates an exact union-tag failure argument.
    pub fn union_tag_problem(problem: crate::DiagnosticUnionTagProblem) -> Self {
        Self::new(
            DiagnosticArgName::UnionTagProblem,
            DiagnosticArgValue::UnionTagProblem(problem),
        )
    }

    /// Creates an exact declared-copy failure argument.
    pub const fn copy_contract_problem(problem: crate::DiagnosticCopyContractProblem) -> Self {
        Self::new(
            DiagnosticArgName::CopyContractProblem,
            DiagnosticArgValue::CopyContractProblem(problem),
        )
    }

    /// Creates an exact invalid inline-storage type argument.
    pub const fn stored_type_problem(problem: crate::DiagnosticStoredTypeProblem) -> Self {
        Self::new(
            DiagnosticArgName::StoredTypeProblem,
            DiagnosticArgValue::StoredTypeProblem(problem),
        )
    }

    /// Creates an alignment-purpose argument.
    pub const fn alignment_kind(kind: DiagnosticAlignmentKind) -> Self {
        Self::new(
            DiagnosticArgName::AlignmentKind,
            DiagnosticArgValue::AlignmentKind(kind),
        )
    }

    /// Creates a required alignment argument.
    pub const fn required_alignment(alignment: u64) -> Self {
        Self::new(
            DiagnosticArgName::RequiredAlignment,
            DiagnosticArgValue::Count(alignment),
        )
    }

    /// Creates a maximum supported alignment argument.
    pub const fn maximum_alignment(alignment: u64) -> Self {
        Self::new(
            DiagnosticArgName::MaximumAlignment,
            DiagnosticArgValue::Count(alignment),
        )
    }
}

/// Locale-neutral alignment validation surfaces.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticAlignmentKind {
    /// Ordinary value storage.
    Storage,
    /// Dynamic allocation.
    Allocation,
    /// A foreign callable ABI boundary.
    CallableAbi,
}

impl DiagnosticAlignmentKind {
    /// Returns the stable machine key for this alignment surface.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Storage => "storage",
            Self::Allocation => "allocation",
            Self::CallableAbi => "callable_abi",
        }
    }
}
