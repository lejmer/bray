use bray_source::{SourceId, TextRange};
use bray_syntax::{SourceSyntaxNode, SyntaxKind};

/// Stable source-backed reference to a syntax node discovered as declaration surface.
///
/// Declaration discovery uses anchors instead of storing typed syntax nodes in
/// the declaration table. Later phases can use the source ID, syntax kind, and
/// range to correlate records with parsed syntax without depending on green tree
/// internals.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SyntaxAnchor {
    source_id: SourceId,
    syntax_kind: SyntaxKind,
    full_range: TextRange,
    is_recovered: bool,
}

impl SyntaxAnchor {
    pub(crate) fn from_node(node: &impl SourceSyntaxNode) -> Self {
        Self {
            source_id: node.source().source_id(),
            syntax_kind: node.kind(),
            full_range: node.full_range(),
            is_recovered: node.is_recovered(),
        }
    }

    /// Returns the source snapshot that contains the anchored syntax node.
    pub const fn source_id(self) -> SourceId {
        self.source_id
    }

    /// Returns the concrete syntax node kind this anchor refers to.
    pub const fn syntax_kind(self) -> SyntaxKind {
        self.syntax_kind
    }

    /// Returns the full source range covered by the anchored syntax node.
    pub const fn full_range(self) -> TextRange {
        self.full_range
    }

    /// Returns whether the anchored syntax node contains parser recovery.
    pub const fn is_recovered(self) -> bool {
        self.is_recovered
    }
}

/// Syntax-backed declaration metadata needed before symbol construction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarationSurface {
    visibility: Option<SyntaxKind>,
    modifiers: Box<[SyntaxKind]>,
    directives: Box<[SyntaxAnchor]>,
    constraints: Box<[SyntaxAnchor]>,
    contract_clauses: Box<[SyntaxAnchor]>,
}

impl DeclarationSurface {
    pub(crate) fn new(
        visibility: Option<SyntaxKind>,
        modifiers: impl IntoIterator<Item = SyntaxKind>,
        directives: impl IntoIterator<Item = SyntaxAnchor>,
        constraints: impl IntoIterator<Item = SyntaxAnchor>,
        contract_clauses: impl IntoIterator<Item = SyntaxAnchor>,
    ) -> Self {
        Self {
            visibility,
            modifiers: modifiers.into_iter().collect(),
            directives: directives.into_iter().collect(),
            constraints: constraints.into_iter().collect(),
            contract_clauses: contract_clauses.into_iter().collect(),
        }
    }

    pub(crate) fn empty() -> Self {
        Self::new(None, [], [], [], [])
    }

    /// Returns the declaration visibility token kind when one was present.
    pub const fn visibility(&self) -> Option<SyntaxKind> {
        self.visibility
    }

    /// Returns non-visibility modifier token kinds in source order.
    pub fn modifiers(&self) -> &[SyntaxKind] {
        &self.modifiers
    }

    /// Returns directive syntax anchors in source order.
    pub fn directives(&self) -> &[SyntaxAnchor] {
        &self.directives
    }

    /// Returns syntax anchors for declaration constraint clauses in source order.
    pub fn constraints(&self) -> &[SyntaxAnchor] {
        &self.constraints
    }

    /// Returns syntax anchors for callable contract clauses in source order.
    pub fn contract_clauses(&self) -> &[SyntaxAnchor] {
        &self.contract_clauses
    }
}
