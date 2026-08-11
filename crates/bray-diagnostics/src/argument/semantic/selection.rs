use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates a semantic-selection category argument.
    pub const fn selection_kind(kind: DiagnosticSelectionKind) -> Self {
        Self::new(
            DiagnosticArgName::SelectionKind,
            DiagnosticArgValue::SelectionKind(kind),
        )
    }

    /// Creates an exact applicable selection-candidate set argument.
    pub fn selection_candidates(candidates: crate::DiagnosticSelectionCandidates) -> Self {
        Self::new(
            DiagnosticArgName::SelectionCandidates,
            DiagnosticArgValue::SelectionCandidates(candidates),
        )
    }

    /// Creates an exact rejected selection-candidate set argument.
    pub fn selection_rejections(rejections: crate::DiagnosticSelectionRejections) -> Self {
        Self::new(
            DiagnosticArgName::SelectionRejections,
            DiagnosticArgValue::SelectionRejections(rejections),
        )
    }
}

/// Locale-neutral semantic operation categories used by selection diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSelectionKind {
    /// A callable or overload arm.
    Callable,
    /// A receiver-associated member.
    Member,
    /// A unary or binary operator implementation.
    Operator,
    /// An element or slice indexing contract.
    Index,
    /// A struct, variant, or type-form construction operation.
    Construction,
    /// An explicit conversion operation.
    Conversion,
    /// A selected trait implementation.
    Implementation,
    /// An iterable and iterator protocol pair for one source expression.
    IterationSource,
}

impl DiagnosticSelectionKind {
    /// Returns the stable machine key for this semantic operation category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Callable => "callable",
            Self::Member => "member",
            Self::Operator => "operator",
            Self::Index => "index",
            Self::Construction => "construction",
            Self::Conversion => "conversion",
            Self::Implementation => "implementation",
            Self::IterationSource => "iteration_source",
        }
    }
}
