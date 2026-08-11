use bray_source::SourceSpan;

use crate::DiagnosticArg;

/// Supporting source location involved in one diagnostic.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DiagnosticRelatedLocation {
    kind: DiagnosticRelatedLocationKind,
    span: SourceSpan,
    args: Vec<DiagnosticArg>,
}

impl DiagnosticRelatedLocation {
    /// Creates a related location with no message arguments.
    pub fn new(kind: DiagnosticRelatedLocationKind, span: SourceSpan) -> Self {
        Self {
            kind,
            span,
            args: Vec::new(),
        }
    }

    /// Adds one typed message argument.
    pub fn with_arg(mut self, arg: DiagnosticArg) -> Self {
        self.args.push(arg);

        self
    }

    /// Returns the stable relationship category.
    pub const fn kind(&self) -> DiagnosticRelatedLocationKind {
        self.kind
    }

    /// Returns the supporting source span.
    pub const fn span(&self) -> SourceSpan {
        self.span
    }

    /// Returns the typed message arguments.
    pub fn args(&self) -> &[DiagnosticArg] {
        &self.args
    }
}

/// Stable relationship between a diagnostic and another source location.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticRelatedLocationKind {
    /// The location that established a declaration or contract first.
    FirstDeclaration,
    /// The directive that selected a value before a duplicate directive.
    FirstDirective,
    /// Another declaration that conflicts with the primary source.
    ConflictingDeclaration,
    /// The source that introduced a dependency or requirement.
    RequirementOrigin,
    /// The expression that established a still-live conflicting borrow.
    BorrowOrigin,
    /// The expression that moved storage before the reported use.
    MoveOrigin,
    /// The operation that created the allocation involved in the failure.
    AllocationOrigin,
    /// The operation that initialized a value still tracked in raw storage.
    InitializationOrigin,
    /// The operation that invalidated the allocation before the reported access.
    DeallocationOrigin,
    /// The earlier dependency selection that conflicts with the primary selection.
    ConflictingDependency,
    /// A stored member that participates in the rejected representation layout.
    RepresentationMember,
    /// A stored member that prevents the containing type from being copied.
    NonCopyableMember,
    /// A declaration or member edge in an inline representation cycle.
    RepresentationCycleLocation,
    /// An earlier pattern that already covers the reported arm or alternative.
    CoveredByPattern,
    /// A source declaration that remains applicable to a failed semantic selection.
    SelectionCandidate,
}

impl DiagnosticRelatedLocationKind {
    /// Returns the stable machine key for this relationship.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FirstDeclaration => "first_declaration",
            Self::FirstDirective => "first_directive",
            Self::ConflictingDeclaration => "conflicting_declaration",
            Self::RequirementOrigin => "requirement_origin",
            Self::BorrowOrigin => "borrow_origin",
            Self::MoveOrigin => "move_origin",
            Self::AllocationOrigin => "allocation_origin",
            Self::InitializationOrigin => "initialization_origin",
            Self::DeallocationOrigin => "deallocation_origin",
            Self::ConflictingDependency => "conflicting_dependency",
            Self::RepresentationMember => "representation_member",
            Self::NonCopyableMember => "non_copyable_member",
            Self::RepresentationCycleLocation => "representation_cycle_location",
            Self::CoveredByPattern => "covered_by_pattern",
            Self::SelectionCandidate => "selection_candidate",
        }
    }
}
