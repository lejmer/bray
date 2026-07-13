use bray_source::{SourceId, TextRange};
use bray_syntax::{
    SourceSyntaxNode, SyntaxCast, SyntaxKind, SyntaxTree, SyntaxWalkControl, SyntaxWalkEvent,
    walk_syntax_node,
};

/// Stable source-backed reference to a syntax node used by later compiler phases.
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
    /// Creates a stable source-backed reference to one typed syntax node.
    pub fn from_node(node: &impl SourceSyntaxNode) -> Self {
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

    /// Resolves the first descendant of `T` within this exact anchored syntax node.
    pub fn find_descendant<T: SyntaxCast>(self, syntax: &SyntaxTree) -> Option<T> {
        let root = syntax.find_node(
            self.source_id,
            self.syntax_kind,
            self.full_range,
            self.is_recovered,
        )?;

        let mut result = None;

        walk_syntax_node(&root, |event| {
            if let SyntaxWalkEvent::EnterNode(node) = event
                && node.kind() == T::KIND
            {
                result = node.cast();

                return SyntaxWalkControl::Stop;
            }

            SyntaxWalkControl::Continue
        });

        result
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
    runtime_default: Option<SyntaxAnchor>,
    overload_arms: Box<[SyntaxAnchor]>,
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
            runtime_default: None,
            overload_arms: Box::new([]),
        }
    }

    pub(crate) fn with_runtime_default(mut self, runtime_default: Option<SyntaxAnchor>) -> Self {
        self.runtime_default = runtime_default;
        self
    }

    pub(crate) fn with_overload_arms(
        mut self,
        overload_arms: impl IntoIterator<Item = SyntaxAnchor>,
    ) -> Self {
        self.overload_arms = overload_arms.into_iter().collect();
        self
    }

    pub(crate) fn empty() -> Self {
        Self::new(None, [], [], [], [])
    }

    /// Returns the declaration visibility token kind when one was present.
    pub const fn visibility(&self) -> Option<SyntaxKind> {
        self.visibility
    }

    /// Returns whether the declaration has explicit internal visibility.
    pub const fn is_internal(&self) -> bool {
        matches!(self.visibility, Some(SyntaxKind::InternalKeyword))
    }

    /// Returns non-visibility modifier token kinds in source order.
    pub fn modifiers(&self) -> &[SyntaxKind] {
        &self.modifiers
    }

    /// Returns whether the declaration carries the `static` modifier.
    pub fn has_static_modifier(&self) -> bool {
        self.modifiers.contains(&SyntaxKind::StaticKeyword)
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

    /// Returns the default expression anchor, or its recovered owner anchor, when present.
    pub const fn runtime_default(&self) -> Option<SyntaxAnchor> {
        self.runtime_default
    }

    /// Returns overload arm path anchors in source order.
    pub fn overload_arms(&self) -> &[SyntaxAnchor] {
        &self.overload_arms
    }
}

#[cfg(test)]
mod tests {
    use bray_syntax::{CallableBodyBlockExpressionSyntax, SyntaxTree};
    use bray_testing::{test_source_at, test_source_store};

    use super::SyntaxAnchor;

    #[test]
    fn syntax_anchors_resolve_typed_descendants_only_within_their_node() {
        let sources = test_source_store([concat!(
            "module app;\n",
            "func first()\n",
            "{\n",
            "}\n",
            "extern func second();\n",
        )]);

        let parsed = bray_parser::parse_source_unit(test_source_at(&sources, 0));
        let source_unit = parsed.source_unit().clone();
        let functions = source_unit.function_declarations().collect::<Vec<_>>();

        let [first, second] = functions.as_slice() else {
            panic!("test source must contain two functions");
        };

        let first = SyntaxAnchor::from_node(first);
        let second = SyntaxAnchor::from_node(second);
        let syntax = SyntaxTree::compilation_unit([source_unit]);

        assert!(
            first
                .find_descendant::<CallableBodyBlockExpressionSyntax>(&syntax)
                .is_some()
        );
        assert!(
            second
                .find_descendant::<CallableBodyBlockExpressionSyntax>(&syntax)
                .is_none()
        );
    }
}
