use bray_diagnostics::DiagnosticBag;
use bray_source::{SourceId, SourceSnapshot, SourceStore};
use bray_syntax::{
    CallableContractDeclarationSyntax, CallableOverloadDeclarationSyntax,
    ConstantDeclarationSyntax, DestructorMemberDeclarationSyntax, FinalizerMemberDeclarationSyntax,
    FunctionDeclarationSyntax, ImplementationOverloadDeclarationSyntax,
    ImplementationTypeMemberBindingSyntax, InherentImplementationDeclarationSyntax,
    NamedTraitImplementationDeclarationSyntax, PredicateDeclarationSyntax,
    ScopeEnterMemberDeclarationSyntax, ScopeExitMemberDeclarationSyntax, SourceUnitSyntax,
    StructDeclarationSyntax, StructFieldDeclarationSyntax, SyntaxTree,
    TraitCallableMemberDeclarationSyntax, TraitConstantMemberDeclarationSyntax,
    TraitDeclarationSyntax, TraitDestructorRequirementDeclarationSyntax,
    TraitFinalizerRequirementDeclarationSyntax, TraitPredicateMemberDeclarationSyntax,
    TraitScopeEnterRequirementDeclarationSyntax, TraitScopeExitRequirementDeclarationSyntax,
    TraitTypeMemberDeclarationSyntax, TypeCallableMemberDeclarationSyntax,
    TypeConstructorMemberDeclarationSyntax, TypeExpressionSyntax, UnionDeclarationSyntax,
    UnionPayloadFieldSyntax, UnionVariantDeclarationSyntax,
    UnnamedTraitImplementationDeclarationSyntax,
};

use super::state::Parser;

/// Syntax tree plus diagnostics for a source store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxTreeResult {
    syntax_tree: SyntaxTree,
    diagnostics: DiagnosticBag,
}

impl SyntaxTreeResult {
    pub(crate) fn new(syntax_tree: SyntaxTree, diagnostics: DiagnosticBag) -> Self {
        Self {
            syntax_tree,
            diagnostics,
        }
    }

    /// Builds a syntax tree result from source-unit syntax results.
    ///
    /// The iterator order becomes the source-unit order in the syntax tree.
    pub fn from_source_unit_results(
        results: impl IntoIterator<Item = SourceUnitSyntaxResult>,
    ) -> Self {
        let mut source_units = Vec::new();
        let mut diagnostic_bags = Vec::new();

        for result in results {
            let (_source_id, source_unit, diagnostics) = result.into_parts();

            source_units.push(source_unit);
            diagnostic_bags.push(diagnostics);
        }

        Self::new(
            SyntaxTree::compilation_unit(source_units),
            DiagnosticBag::merged_all(&diagnostic_bags),
        )
    }

    /// Returns the immutable syntax tree.
    pub const fn syntax_tree(&self) -> &SyntaxTree {
        &self.syntax_tree
    }

    /// Returns diagnostics produced while building syntax.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Consumes the result and returns its tree plus diagnostics.
    pub fn into_parts(self) -> (SyntaxTree, DiagnosticBag) {
        (self.syntax_tree, self.diagnostics)
    }
}

/// Parses all source snapshots in source ID order into one compilation unit.
pub fn parse_compilation_unit(sources: &SourceStore) -> SyntaxTreeResult {
    SyntaxTreeResult::from_source_unit_results(sources.iter().map(parse_source_unit))
}

/// Source-unit syntax plus diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceUnitSyntaxResult {
    source_id: SourceId,
    source_unit: SourceUnitSyntax,
    diagnostics: DiagnosticBag,
}

impl SourceUnitSyntaxResult {
    pub(crate) const fn new(
        source_id: SourceId,
        source_unit: SourceUnitSyntax,
        diagnostics: DiagnosticBag,
    ) -> Self {
        Self {
            source_id,
            source_unit,
            diagnostics,
        }
    }

    /// Returns the source ID for this syntax result.
    pub const fn source_id(&self) -> SourceId {
        self.source_id
    }

    /// Returns the source unit syntax node.
    pub const fn source_unit(&self) -> &SourceUnitSyntax {
        &self.source_unit
    }

    /// Returns diagnostics produced while building this source unit.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Consumes the result and returns its source ID, syntax node, and diagnostics.
    pub fn into_parts(self) -> (SourceId, SourceUnitSyntax, DiagnosticBag) {
        (self.source_id, self.source_unit, self.diagnostics)
    }
}

/// Parses one source snapshot into one source-unit syntax node.
pub fn parse_source_unit(snapshot: &SourceSnapshot) -> SourceUnitSyntaxResult {
    let source_id = snapshot.source_id();

    // Parser owns a snapshot handle; cloning shares immutable source text.
    let mut parser = Parser::new(snapshot.clone());

    let source_unit = parser.parse_source_unit();
    let diagnostics = parser.finish();

    SourceUnitSyntaxResult::new(source_id, source_unit, diagnostics)
}

/// Selects the declaration grammar used for an exact fragment snapshot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DeclarationFragmentContext {
    /// Module-level declaration grammar.
    Module,
    /// Struct member declaration grammar.
    Struct,
    /// Union member declaration grammar.
    Union,
    /// Union variant payload-field grammar.
    UnionVariant,
    /// Trait member declaration grammar.
    Trait,
    /// Implementation member declaration grammar.
    Implementation,
}

/// One declaration parsed outside a source unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeclarationFragmentSyntax {
    /// Constant declaration.
    Constant(ConstantDeclarationSyntax),
    /// Function declaration.
    Function(FunctionDeclarationSyntax),
    /// Predicate declaration.
    Predicate(PredicateDeclarationSyntax),
    /// Callable contract declaration.
    CallableContract(CallableContractDeclarationSyntax),
    /// Callable overload declaration.
    CallableOverload(CallableOverloadDeclarationSyntax),
    /// Implementation overload declaration.
    ImplementationOverload(ImplementationOverloadDeclarationSyntax),
    /// Struct declaration.
    Struct(StructDeclarationSyntax),
    /// Union declaration.
    Union(UnionDeclarationSyntax),
    /// Trait declaration.
    Trait(TraitDeclarationSyntax),
    /// Inherent implementation declaration.
    InherentImplementation(InherentImplementationDeclarationSyntax),
    /// Unnamed trait implementation declaration.
    UnnamedTraitImplementation(UnnamedTraitImplementationDeclarationSyntax),
    /// Named trait implementation declaration.
    NamedTraitImplementation(NamedTraitImplementationDeclarationSyntax),
    /// Struct field declaration.
    StructField(StructFieldDeclarationSyntax),
    /// Union variant declaration.
    UnionVariant(UnionVariantDeclarationSyntax),
    /// Union payload field.
    UnionPayloadField(UnionPayloadFieldSyntax),
    /// Type constructor member declaration.
    TypeConstructorMember(TypeConstructorMemberDeclarationSyntax),
    /// Type callable member declaration.
    TypeCallableMember(TypeCallableMemberDeclarationSyntax),
    /// Finalizer member declaration.
    FinalizerMember(FinalizerMemberDeclarationSyntax),
    /// Destructor member declaration.
    DestructorMember(DestructorMemberDeclarationSyntax),
    /// Scope-enter member declaration.
    ScopeEnterMember(ScopeEnterMemberDeclarationSyntax),
    /// Scope-exit member declaration.
    ScopeExitMember(ScopeExitMemberDeclarationSyntax),
    /// Trait constant member declaration.
    TraitConstantMember(TraitConstantMemberDeclarationSyntax),
    /// Trait type-valued member declaration.
    TraitTypeMember(TraitTypeMemberDeclarationSyntax),
    /// Trait predicate member declaration.
    TraitPredicateMember(TraitPredicateMemberDeclarationSyntax),
    /// Trait callable member declaration.
    TraitCallableMember(TraitCallableMemberDeclarationSyntax),
    /// Trait finalizer requirement declaration.
    TraitFinalizerRequirement(TraitFinalizerRequirementDeclarationSyntax),
    /// Trait destructor requirement declaration.
    TraitDestructorRequirement(TraitDestructorRequirementDeclarationSyntax),
    /// Trait scope-enter requirement declaration.
    TraitScopeEnterRequirement(TraitScopeEnterRequirementDeclarationSyntax),
    /// Trait scope-exit requirement declaration.
    TraitScopeExitRequirement(TraitScopeExitRequirementDeclarationSyntax),
    /// Implementation type-valued member binding.
    ImplementationTypeMemberBinding(ImplementationTypeMemberBindingSyntax),
}

impl DeclarationFragmentSyntax {
    /// Returns whether the declaration contains missing or skipped syntax.
    pub fn is_recovered(&self) -> bool {
        match self {
            Self::Constant(syntax) => syntax.is_recovered(),
            Self::Function(syntax) => syntax.is_recovered(),
            Self::Predicate(syntax) => syntax.is_recovered(),
            Self::CallableContract(syntax) => syntax.is_recovered(),
            Self::CallableOverload(syntax) => syntax.is_recovered(),
            Self::ImplementationOverload(syntax) => syntax.is_recovered(),
            Self::Struct(syntax) => syntax.is_recovered(),
            Self::Union(syntax) => syntax.is_recovered(),
            Self::Trait(syntax) => syntax.is_recovered(),
            Self::InherentImplementation(syntax) => syntax.is_recovered(),
            Self::UnnamedTraitImplementation(syntax) => syntax.is_recovered(),
            Self::NamedTraitImplementation(syntax) => syntax.is_recovered(),
            Self::StructField(syntax) => syntax.is_recovered(),
            Self::UnionVariant(syntax) => syntax.is_recovered(),
            Self::UnionPayloadField(syntax) => syntax.is_recovered(),
            Self::TypeConstructorMember(syntax) => syntax.is_recovered(),
            Self::TypeCallableMember(syntax) => syntax.is_recovered(),
            Self::FinalizerMember(syntax) => syntax.is_recovered(),
            Self::DestructorMember(syntax) => syntax.is_recovered(),
            Self::ScopeEnterMember(syntax) => syntax.is_recovered(),
            Self::ScopeExitMember(syntax) => syntax.is_recovered(),
            Self::TraitConstantMember(syntax) => syntax.is_recovered(),
            Self::TraitTypeMember(syntax) => syntax.is_recovered(),
            Self::TraitPredicateMember(syntax) => syntax.is_recovered(),
            Self::TraitCallableMember(syntax) => syntax.is_recovered(),
            Self::TraitFinalizerRequirement(syntax) => syntax.is_recovered(),
            Self::TraitDestructorRequirement(syntax) => syntax.is_recovered(),
            Self::TraitScopeEnterRequirement(syntax) => syntax.is_recovered(),
            Self::TraitScopeExitRequirement(syntax) => syntax.is_recovered(),
            Self::ImplementationTypeMemberBinding(syntax) => syntax.is_recovered(),
        }
    }
}

/// Declaration-fragment syntax plus ordinary parser diagnostics and recovery state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarationFragmentSyntaxResult {
    declaration: Option<DeclarationFragmentSyntax>,
    diagnostics: DiagnosticBag,
    is_recovered: bool,
}

impl DeclarationFragmentSyntaxResult {
    pub(crate) const fn new(
        declaration: Option<DeclarationFragmentSyntax>,
        diagnostics: DiagnosticBag,
        is_recovered: bool,
    ) -> Self {
        Self {
            declaration,
            diagnostics,
            is_recovered,
        }
    }

    /// Returns the parsed declaration, or `None` when the fragment has no declaration in context.
    pub const fn declaration(&self) -> Option<&DeclarationFragmentSyntax> {
        self.declaration.as_ref()
    }

    /// Returns diagnostics produced while parsing the fragment.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Returns whether parsing inserted missing syntax, skipped syntax, or found extra syntax.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }

    /// Consumes the result and returns its declaration, diagnostics, and recovery state.
    pub fn into_parts(self) -> (Option<DeclarationFragmentSyntax>, DiagnosticBag, bool) {
        (self.declaration, self.diagnostics, self.is_recovered)
    }
}

/// Parses one exact source snapshot as a declaration fragment in `context`.
pub fn parse_declaration_fragment(
    snapshot: &SourceSnapshot,
    context: DeclarationFragmentContext,
) -> DeclarationFragmentSyntaxResult {
    // Parser owns a snapshot handle; cloning shares immutable source text.
    let mut parser = Parser::new(snapshot.clone());

    let (declaration, is_recovered) = parser.parse_declaration_fragment(context);

    let diagnostics = parser.finish();

    DeclarationFragmentSyntaxResult::new(declaration, diagnostics, is_recovered)
}

/// Type-expression syntax plus ordinary parser diagnostics and recovery state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeExpressionFragmentSyntaxResult {
    type_expression: TypeExpressionSyntax,
    diagnostics: DiagnosticBag,
    is_recovered: bool,
}

impl TypeExpressionFragmentSyntaxResult {
    pub(crate) const fn new(
        type_expression: TypeExpressionSyntax,
        diagnostics: DiagnosticBag,
        is_recovered: bool,
    ) -> Self {
        Self {
            type_expression,
            diagnostics,
            is_recovered,
        }
    }

    /// Returns the parsed type expression.
    pub const fn type_expression(&self) -> &TypeExpressionSyntax {
        &self.type_expression
    }

    /// Returns diagnostics produced while parsing the fragment.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Returns whether parsing inserted missing syntax, skipped syntax, or found extra syntax.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }

    /// Consumes the result and returns its type expression, diagnostics, and recovery state.
    pub fn into_parts(self) -> (TypeExpressionSyntax, DiagnosticBag, bool) {
        (self.type_expression, self.diagnostics, self.is_recovered)
    }
}

/// Parses one exact source snapshot as a type-expression fragment.
pub fn parse_type_expression_fragment(
    snapshot: &SourceSnapshot,
) -> TypeExpressionFragmentSyntaxResult {
    // Parser owns a snapshot handle; cloning shares immutable source text.
    let mut parser = Parser::new(snapshot.clone());

    let (type_expression, is_recovered) = parser.parse_type_expression_fragment();

    let diagnostics = parser.finish();

    TypeExpressionFragmentSyntaxResult::new(type_expression, diagnostics, is_recovered)
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_source::{SourceId, TextRange, TextSize};
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use super::{parse_compilation_unit, parse_source_unit};
    use crate::test_support::{diagnostic_kinds, token_kinds};

    #[test]
    fn parser_builds_one_compilation_unit_root() {
        let sources = source_store(["module main;\n"]);
        let result = parse_compilation_unit(&sources);
        let root = result.syntax_tree().root();

        assert_eq!(root.kind(), SyntaxKind::CompilationUnit);
        assert_eq!(root.source_units().len(), 1);
        assert_eq!(root.full_range(), TextRange::EMPTY);
    }

    #[test]
    fn parser_creates_source_units_for_every_snapshot_in_source_id_order() {
        let sources = source_store(["module first;", "module second;", "module third;"]);
        let result = parse_compilation_unit(&sources);
        let source_units = result.syntax_tree().root().source_units();

        assert_eq!(source_units.len(), 3);
        assert_eq!(source_units[0].full_text(), "module first;");
        assert_eq!(source_units[1].full_text(), "module second;");
        assert_eq!(source_units[2].full_text(), "module third;");
    }

    #[test]
    fn parser_exposes_source_unit_syntax_results_as_the_parse_primitive() {
        let sources = source_store(["first", "$"]);

        let source = match sources.get(SourceId::new(1)) {
            Some(source) => source,
            None => panic!("second source should exist"),
        };

        let result = parse_source_unit(source);

        assert_eq!(result.source_id(), SourceId::new(1));
        assert_eq!(result.source_unit().full_text(), "$");

        assert_eq!(
            diagnostic_kinds(result.diagnostics()),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxUnexpectedEof,
                DiagnosticKind::SyntaxUnexpectedEof,
                DiagnosticKind::SyntaxUnexpectedEof
            ]
        );
    }

    #[test]
    fn source_unit_full_spans_cover_their_source_text() {
        let sources = source_store(["aé\n", ""]);
        let result = parse_compilation_unit(&sources);
        let source_units = result.syntax_tree().root().source_units();

        assert_eq!(
            source_units[0].full_range(),
            TextRange::new(TextSize::ZERO, TextSize::new(4))
        );

        assert_eq!(source_units[1].full_range(), TextRange::EMPTY);
    }

    #[test]
    fn source_unit_tokens_include_eof_and_remain_reachable() {
        let sources = source_store(["module main;"]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];

        assert_eq!(
            token_kinds(source_unit.tokens()),
            [
                SyntaxKind::ModuleKeyword,
                SyntaxKind::IdentifierToken,
                SyntaxKind::SemicolonToken,
                SyntaxKind::EndOfFileToken
            ]
        );

        assert_eq!(
            source_unit.tokens().last().map(|token| token.kind()),
            Some(SyntaxKind::EndOfFileToken)
        );
    }

    #[test]
    fn parser_preserves_exact_source_reconstruction_with_trivia() {
        let text = "  module // hi\r\n/** docs */\nmain;\n// final\n";
        let sources = source_store([text]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];

        assert_eq!(source_unit.full_text(), text);
        assert_eq!(result.syntax_tree().full_text(), text);
    }

    #[test]
    fn parser_returns_no_diagnostics_for_lexically_valid_sources() {
        let sources = source_store(["module main;", "module extra {}"]);
        let result = parse_compilation_unit(&sources);

        assert!(result.diagnostics().is_empty());
    }
}
