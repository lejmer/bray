use bray_syntax::{
    CallableContractDeclarationSyntaxBuilder, DestructorMemberDeclarationSyntaxBuilder,
    EnsuresClauseSyntax, EnsuresClauseSyntaxBuilder, ExpressionSyntax,
    FinalizerMemberDeclarationSyntaxBuilder, FunctionDeclarationSyntaxBuilder,
    InherentImplementationDeclarationSyntaxBuilder, LambdaExpressionSyntaxBuilder,
    NamedTraitImplementationDeclarationSyntaxBuilder, PathSyntax, RequiresClauseSyntax,
    RequiresClauseSyntaxBuilder, ScopeEnterMemberDeclarationSyntaxBuilder,
    ScopeExitMemberDeclarationSyntaxBuilder, StructDeclarationSyntaxBuilder, SyntaxKind,
    SyntaxToken, TraitCallableMemberDeclarationSyntaxBuilder, TraitDeclarationSyntaxBuilder,
    TraitDestructorRequirementDeclarationSyntaxBuilder,
    TraitFinalizerRequirementDeclarationSyntaxBuilder,
    TraitScopeEnterRequirementDeclarationSyntaxBuilder,
    TraitScopeExitRequirementDeclarationSyntaxBuilder, TypeCallableMemberDeclarationSyntaxBuilder,
    TypeConstructorMemberDeclarationSyntaxBuilder, TypeExpressionSyntaxBuilder,
    UnionDeclarationSyntaxBuilder, UnnamedTraitImplementationDeclarationSyntaxBuilder,
    UsesClauseSyntax, UsesClauseSyntaxBuilder, WithClauseSyntax, WithClauseSyntaxBuilder,
};

use super::expression::EXPRESSION_START_KINDS;
use super::recovery::RecoverySyntaxSink;
use super::separated::{SeparatedListSpec, SeparatedListSyntaxSink, separated_list_recovery_kinds};
use super::state::Parser;

type ContractBoundary = fn(&mut Parser) -> bool;

const CALLABLE_CONTRACT_CLAUSE_START_KINDS: [SyntaxKind; 4] = [
    SyntaxKind::RequiresKeyword,
    SyntaxKind::EnsuresKeyword,
    SyntaxKind::WithKeyword,
    SyntaxKind::UsesKeyword,
];

pub(super) const BRACED_DECLARATION_CONSTRAINT_BOUNDARY_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::WithKeyword,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const PATH_START_KINDS: [SyntaxKind; 1] = [SyntaxKind::IdentifierToken];

const CONTRACT_CLAUSE_RECOVERY_KINDS: [SyntaxKind; 36] = [
    SyntaxKind::CloseParenToken,
    SyntaxKind::RequiresKeyword,
    SyntaxKind::EnsuresKeyword,
    SyntaxKind::WithKeyword,
    SyntaxKind::UsesKeyword,
    SyntaxKind::EqualsToken,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
    SyntaxKind::AtToken,
    SyntaxKind::ModuleKeyword,
    SyntaxKind::UsingKeyword,
    SyntaxKind::ExportKeyword,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::ExternKeyword,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::ConstKeyword,
    SyntaxKind::StaticKeyword,
    SyntaxKind::ConsumeKeyword,
    SyntaxKind::MutKeyword,
    SyntaxKind::FuncKeyword,
    SyntaxKind::CallableKeyword,
    SyntaxKind::StructKeyword,
    SyntaxKind::UnionKeyword,
    SyntaxKind::TraitKeyword,
    SyntaxKind::ImplKeyword,
    SyntaxKind::OverloadKeyword,
    SyntaxKind::PredicateKeyword,
    SyntaxKind::TypeKeyword,
    SyntaxKind::ConstructKeyword,
    SyntaxKind::FinalizeKeyword,
    SyntaxKind::DestructKeyword,
    SyntaxKind::EnterKeyword,
    SyntaxKind::ExitKeyword,
];

impl Parser {
    pub(super) fn parse_callable_contract_clauses(
        &mut self,
        builder: &mut impl CallableContractClauseSyntaxSink,
        at_boundary: ContractBoundary,
    ) {
        while self.at_callable_contract_clause_start() {
            match self.peek().kind() {
                SyntaxKind::RequiresKeyword => {
                    builder.push_requires_clause(self.parse_requires_clause(at_boundary));
                }
                SyntaxKind::EnsuresKeyword => {
                    builder.push_ensures_clause(self.parse_ensures_clause(at_boundary));
                }
                SyntaxKind::WithKeyword => {
                    builder.push_with_clause(self.parse_with_clause(at_boundary));
                }
                SyntaxKind::UsesKeyword => {
                    builder.push_uses_clause(self.parse_uses_clause(at_boundary));
                }
                _ => unreachable!("contract clause loop checked clause start kinds"),
            }
        }
    }

    pub(super) fn parse_with_clauses(
        &mut self,
        builder: &mut impl WithClauseSyntaxSink,
        at_boundary: ContractBoundary,
    ) {
        while self.at(SyntaxKind::WithKeyword) {
            builder.push_with_clause(self.parse_with_clause(at_boundary));
        }
    }

    pub(super) fn at_callable_contract_clause_start(&mut self) -> bool {
        self.at_any(&CALLABLE_CONTRACT_CLAUSE_START_KINDS)
    }

    fn parse_requires_clause(&mut self, at_boundary: ContractBoundary) -> RequiresClauseSyntax {
        let start = self.peek().full_range().start();
        let mut builder = RequiresClauseSyntax::builder(self.syntax_source(), start);

        builder.push_requires_keyword(self.expect(SyntaxKind::RequiresKeyword));
        self.parse_expression_contract_clause_arguments(&mut builder, at_boundary);

        builder.build()
    }

    fn parse_ensures_clause(&mut self, at_boundary: ContractBoundary) -> EnsuresClauseSyntax {
        let start = self.peek().full_range().start();
        let mut builder = EnsuresClauseSyntax::builder(self.syntax_source(), start);

        builder.push_ensures_keyword(self.expect(SyntaxKind::EnsuresKeyword));
        self.parse_expression_contract_clause_arguments(&mut builder, at_boundary);

        builder.build()
    }

    fn parse_with_clause(&mut self, at_boundary: ContractBoundary) -> WithClauseSyntax {
        let start = self.peek().full_range().start();
        let mut builder = WithClauseSyntax::builder(self.syntax_source(), start);

        builder.push_with_keyword(self.expect(SyntaxKind::WithKeyword));
        self.parse_static_predicate_clause_arguments(&mut builder, at_boundary);

        builder.build()
    }

    fn parse_uses_clause(&mut self, at_boundary: ContractBoundary) -> UsesClauseSyntax {
        let start = self.peek().full_range().start();
        let mut builder = UsesClauseSyntax::builder(self.syntax_source(), start);

        builder.push_uses_keyword(self.expect(SyntaxKind::UsesKeyword));
        self.parse_path_contract_clause_arguments(&mut builder, at_boundary);

        builder.build()
    }

    fn parse_expression_contract_clause_arguments(
        &mut self,
        builder: &mut impl ParenthesizedContractClauseSyntaxSink<ExpressionSyntax>,
        at_boundary: ContractBoundary,
    ) {
        self.parse_parenthesized_contract_clause_arguments(
            builder,
            at_boundary,
            &EXPRESSION_START_KINDS,
            Parser::parse_contract_clause_expression,
        );
    }

    fn parse_static_predicate_clause_arguments(
        &mut self,
        builder: &mut impl ParenthesizedContractClauseSyntaxSink<ExpressionSyntax>,
        at_boundary: ContractBoundary,
    ) {
        self.parse_parenthesized_contract_clause_arguments(
            builder,
            at_boundary,
            &EXPRESSION_START_KINDS,
            Parser::parse_static_predicate_expression,
        );
    }

    fn parse_path_contract_clause_arguments(
        &mut self,
        builder: &mut impl ParenthesizedContractClauseSyntaxSink<PathSyntax>,
        at_boundary: ContractBoundary,
    ) {
        self.parse_parenthesized_contract_clause_arguments(
            builder,
            at_boundary,
            &PATH_START_KINDS,
            Parser::parse_contract_clause_path,
        );
    }

    fn parse_parenthesized_contract_clause_arguments<Item>(
        &mut self,
        builder: &mut impl ParenthesizedContractClauseSyntaxSink<Item>,
        at_boundary: ContractBoundary,
        item_start_kinds: &[SyntaxKind],
        mut parse_item: impl FnMut(&mut Parser, ContractBoundary) -> Item,
    ) {
        let recovery_kinds = separated_list_recovery_kinds(
            item_start_kinds,
            SyntaxKind::CommaToken,
            &CONTRACT_CLAUSE_RECOVERY_KINDS,
        );

        let spec = SeparatedListSpec {
            separator_kind: SyntaxKind::CommaToken,
            terminators: &CONTRACT_CLAUSE_RECOVERY_KINDS,
            recovery_kinds: &recovery_kinds,
            allow_trailing_separator: true,
        };

        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));

        if self.at_contract_clause_sequence_end(at_boundary, item_start_kinds) {
            builder.push_item(parse_item(self, at_boundary));
        } else {
            self.parse_separated_list_until(
                builder,
                spec,
                |parser| parser.at_contract_clause_sequence_end(at_boundary, item_start_kinds),
                |parser| parse_item(parser, at_boundary),
            );
        }

        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));
    }

    fn parse_contract_clause_expression(
        &mut self,
        at_boundary: ContractBoundary,
    ) -> ExpressionSyntax {
        let mut at_expression_boundary = |parser: &mut Parser| {
            parser.at(SyntaxKind::CommaToken)
                || parser.at_contract_clause_sequence_end(at_boundary, &EXPRESSION_START_KINDS)
        };

        self.parse_expression_until(&mut at_expression_boundary)
    }

    fn parse_static_predicate_expression(
        &mut self,
        at_boundary: ContractBoundary,
    ) -> ExpressionSyntax {
        if !self.scan_ahead(|scan| {
            let mut at_subject_boundary = |parser: &mut Parser| {
                parser.at(SyntaxKind::ColonToken)
                    || parser.at(SyntaxKind::CommaToken)
                    || parser.at_contract_clause_sequence_end(at_boundary, &EXPRESSION_START_KINDS)
            };

            scan.parse_type_expression_until(&mut at_subject_boundary);

            scan.at(SyntaxKind::ColonToken)
        }) {
            return self.parse_contract_clause_expression(at_boundary);
        }

        let start = self.peek().full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        let mut at_subject_boundary = |parser: &mut Parser| {
            parser.at(SyntaxKind::ColonToken)
                || parser.at(SyntaxKind::CommaToken)
                || parser.at_contract_clause_sequence_end(at_boundary, &EXPRESSION_START_KINDS)
        };

        builder.push_type_expression(self.parse_type_expression_until(&mut at_subject_boundary));

        builder.push_operator_token(self.expect(SyntaxKind::ColonToken));
        builder.push_trait_application(self.parse_trait_application());

        builder.build()
    }

    fn parse_contract_clause_path(&mut self, _at_boundary: ContractBoundary) -> PathSyntax {
        self.parse_path()
    }

    fn at_contract_clause_sequence_end(
        &mut self,
        at_boundary: ContractBoundary,
        item_start_kinds: &[SyntaxKind],
    ) -> bool {
        if self.at(SyntaxKind::CloseParenToken) || self.at(SyntaxKind::EndOfFileToken) {
            return true;
        }

        if self.at_any(item_start_kinds) {
            return false;
        }

        self.at_callable_contract_clause_start() || at_boundary(self)
    }
}

pub(super) trait CallableContractClauseSyntaxSink: RecoverySyntaxSink {
    fn push_requires_clause(&mut self, clause: RequiresClauseSyntax);

    fn push_ensures_clause(&mut self, clause: EnsuresClauseSyntax);

    fn push_with_clause(&mut self, clause: WithClauseSyntax);

    fn push_uses_clause(&mut self, clause: UsesClauseSyntax);
}

pub(super) trait WithClauseSyntaxSink: RecoverySyntaxSink {
    fn push_with_clause(&mut self, clause: WithClauseSyntax);
}

trait ParenthesizedContractClauseSyntaxSink<Item>: SeparatedListSyntaxSink<Item> {
    fn push_open_paren_token(&mut self, token: SyntaxToken);

    fn push_close_paren_token(&mut self, token: SyntaxToken);
}

impl SeparatedListSyntaxSink<ExpressionSyntax> for RequiresClauseSyntaxBuilder {
    fn push_item(&mut self, item: ExpressionSyntax) {
        RequiresClauseSyntaxBuilder::push_expression(self, item);
    }

    fn push_separator(&mut self, separator: SyntaxToken) {
        RequiresClauseSyntaxBuilder::push_separator_token(self, separator);
    }
}

impl ParenthesizedContractClauseSyntaxSink<ExpressionSyntax> for RequiresClauseSyntaxBuilder {
    fn push_open_paren_token(&mut self, token: SyntaxToken) {
        RequiresClauseSyntaxBuilder::push_open_paren_token(self, token);
    }

    fn push_close_paren_token(&mut self, token: SyntaxToken) {
        RequiresClauseSyntaxBuilder::push_close_paren_token(self, token);
    }
}

impl SeparatedListSyntaxSink<ExpressionSyntax> for EnsuresClauseSyntaxBuilder {
    fn push_item(&mut self, item: ExpressionSyntax) {
        EnsuresClauseSyntaxBuilder::push_expression(self, item);
    }

    fn push_separator(&mut self, separator: SyntaxToken) {
        EnsuresClauseSyntaxBuilder::push_separator_token(self, separator);
    }
}

impl ParenthesizedContractClauseSyntaxSink<ExpressionSyntax> for EnsuresClauseSyntaxBuilder {
    fn push_open_paren_token(&mut self, token: SyntaxToken) {
        EnsuresClauseSyntaxBuilder::push_open_paren_token(self, token);
    }

    fn push_close_paren_token(&mut self, token: SyntaxToken) {
        EnsuresClauseSyntaxBuilder::push_close_paren_token(self, token);
    }
}

impl SeparatedListSyntaxSink<ExpressionSyntax> for WithClauseSyntaxBuilder {
    fn push_item(&mut self, item: ExpressionSyntax) {
        WithClauseSyntaxBuilder::push_expression(self, item);
    }

    fn push_separator(&mut self, separator: SyntaxToken) {
        WithClauseSyntaxBuilder::push_separator_token(self, separator);
    }
}

impl ParenthesizedContractClauseSyntaxSink<ExpressionSyntax> for WithClauseSyntaxBuilder {
    fn push_open_paren_token(&mut self, token: SyntaxToken) {
        WithClauseSyntaxBuilder::push_open_paren_token(self, token);
    }

    fn push_close_paren_token(&mut self, token: SyntaxToken) {
        WithClauseSyntaxBuilder::push_close_paren_token(self, token);
    }
}

impl SeparatedListSyntaxSink<PathSyntax> for UsesClauseSyntaxBuilder {
    fn push_item(&mut self, item: PathSyntax) {
        UsesClauseSyntaxBuilder::push_path(self, item);
    }

    fn push_separator(&mut self, separator: SyntaxToken) {
        UsesClauseSyntaxBuilder::push_separator_token(self, separator);
    }
}

impl ParenthesizedContractClauseSyntaxSink<PathSyntax> for UsesClauseSyntaxBuilder {
    fn push_open_paren_token(&mut self, token: SyntaxToken) {
        UsesClauseSyntaxBuilder::push_open_paren_token(self, token);
    }

    fn push_close_paren_token(&mut self, token: SyntaxToken) {
        UsesClauseSyntaxBuilder::push_close_paren_token(self, token);
    }
}

macro_rules! impl_callable_contract_clause_sink {
    ($builder:ty) => {
        impl CallableContractClauseSyntaxSink for $builder {
            fn push_requires_clause(&mut self, clause: RequiresClauseSyntax) {
                <$builder>::push_requires_clause(self, clause);
            }

            fn push_ensures_clause(&mut self, clause: EnsuresClauseSyntax) {
                <$builder>::push_ensures_clause(self, clause);
            }

            fn push_with_clause(&mut self, clause: WithClauseSyntax) {
                <$builder>::push_with_clause(self, clause);
            }

            fn push_uses_clause(&mut self, clause: UsesClauseSyntax) {
                <$builder>::push_uses_clause(self, clause);
            }
        }
    };
}

macro_rules! impl_with_clause_sink {
    ($builder:ty) => {
        impl WithClauseSyntaxSink for $builder {
            fn push_with_clause(&mut self, clause: WithClauseSyntax) {
                <$builder>::push_with_clause(self, clause);
            }
        }
    };
}

impl_callable_contract_clause_sink!(FunctionDeclarationSyntaxBuilder);
impl_callable_contract_clause_sink!(TypeExpressionSyntaxBuilder);
impl_callable_contract_clause_sink!(LambdaExpressionSyntaxBuilder);
impl_callable_contract_clause_sink!(TypeCallableMemberDeclarationSyntaxBuilder);
impl_callable_contract_clause_sink!(TraitCallableMemberDeclarationSyntaxBuilder);
impl_callable_contract_clause_sink!(TypeConstructorMemberDeclarationSyntaxBuilder);
impl_callable_contract_clause_sink!(FinalizerMemberDeclarationSyntaxBuilder);
impl_callable_contract_clause_sink!(DestructorMemberDeclarationSyntaxBuilder);
impl_callable_contract_clause_sink!(ScopeEnterMemberDeclarationSyntaxBuilder);
impl_callable_contract_clause_sink!(ScopeExitMemberDeclarationSyntaxBuilder);
impl_callable_contract_clause_sink!(TraitFinalizerRequirementDeclarationSyntaxBuilder);
impl_callable_contract_clause_sink!(TraitDestructorRequirementDeclarationSyntaxBuilder);
impl_callable_contract_clause_sink!(TraitScopeEnterRequirementDeclarationSyntaxBuilder);
impl_callable_contract_clause_sink!(TraitScopeExitRequirementDeclarationSyntaxBuilder);

impl_with_clause_sink!(CallableContractDeclarationSyntaxBuilder);
impl_with_clause_sink!(StructDeclarationSyntaxBuilder);
impl_with_clause_sink!(UnionDeclarationSyntaxBuilder);
impl_with_clause_sink!(TraitDeclarationSyntaxBuilder);
impl_with_clause_sink!(InherentImplementationDeclarationSyntaxBuilder);
impl_with_clause_sink!(UnnamedTraitImplementationDeclarationSyntaxBuilder);
impl_with_clause_sink!(NamedTraitImplementationDeclarationSyntaxBuilder);

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::parser::parse_compilation_unit;
    use crate::test_support::parse_diagnostic_kinds;

    #[test]
    fn parser_parses_callable_contract_clauses() {
        let source = concat!(
            "module main; ",
            "func check() requires(valid, ready,) ensures(done) ",
            "with(static_ok) uses(core.io, system.cap,) {}"
        );

        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.function_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one function declaration: {declarations:?}");
        };

        let requires = declaration.requires_clauses().collect::<Vec<_>>();
        let ensures = declaration.ensures_clauses().collect::<Vec<_>>();
        let with = declaration.with_clauses().collect::<Vec<_>>();
        let uses = declaration.uses_clauses().collect::<Vec<_>>();

        let [requires_clause] = requires.as_slice() else {
            panic!("expected one requires clause: {requires:?}");
        };

        let [uses_clause] = uses.as_slice() else {
            panic!("expected one uses clause: {uses:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(requires_clause.expressions().count(), 2);
        assert_eq!(requires_clause.separator_tokens().count(), 2);
        assert_eq!(ensures.len(), 1);
        assert_eq!(with.len(), 1);
        assert_eq!(uses_clause.paths().count(), 2);
        assert_eq!(uses_clause.separator_tokens().count(), 2);

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn static_predicate_scan_stops_before_later_declaration_colons() {
        let source = "module main; struct Wrapper with(true) { value: bool; }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        assert_eq!(result.syntax_tree().full_text(), source);
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_contract_clauses_represent_missing_uses_separators() {
        let source = "module main; func check() uses(core std) {}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.function_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one function declaration: {declarations:?}");
        };

        let uses_clauses = declaration.uses_clauses().collect::<Vec<_>>();

        let [uses_clause] = uses_clauses.as_slice() else {
            panic!("expected one uses clause: {uses_clauses:?}");
        };

        let separators = uses_clause.separator_tokens().collect::<Vec<_>>();

        let [separator] = separators.as_slice() else {
            panic!("expected one separator token: {separators:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(uses_clause.paths().count(), 2);
        assert_eq!(separator.kind(), SyntaxKind::CommaToken);
        assert!(separator.is_missing());

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [DiagnosticKind::SyntaxExpectedToken]
        );
    }

    #[test]
    fn parser_contract_clauses_recover_bad_expression_items() {
        let source = "module main; func check() requires($, valid) {}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.function_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one function declaration: {declarations:?}");
        };

        let requires_clauses = declaration.requires_clauses().collect::<Vec<_>>();

        let [requires_clause] = requires_clauses.as_slice() else {
            panic!("expected one requires clause: {requires_clauses:?}");
        };

        let skipped_syntax = requires_clause.skipped_syntax().collect::<Vec<_>>();

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(requires_clause.full_text(), "requires($, valid) ");
        assert_eq!(requires_clause.expressions().count(), 2);
        assert_eq!(skipped.full_text(), "$");

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [DiagnosticKind::LexicalInvalidCharacter]
        );
    }

    #[test]
    fn parser_accepts_trust_boundaries_in_contract_expressions() {
        let source = concat!(
            "module main; ",
            "trusted predicate valid(value: i32); ",
            "trusted func check(value: i32) ",
            "requires(trusted valid(value)) {}",
        );

        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.function_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one function declaration: {declarations:?}");
        };

        let requires_clauses = declaration.requires_clauses().collect::<Vec<_>>();

        let [requires_clause] = requires_clauses.as_slice() else {
            panic!("expected one requires clause: {requires_clauses:?}");
        };

        assert_eq!(requires_clause.expressions().count(), 1);

        assert!(
            result.diagnostics().is_empty(),
            "{:#?}",
            result.diagnostics()
        );
    }

    #[test]
    fn parser_preserves_static_trait_satisfaction_constraints() {
        let source = concat!(
            "module main; ",
            "func copy<T>(value: T) -> T ",
            "with(T: Copyable) ",
            "{ return value; }",
        );

        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.function_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one function declaration: {declarations:?}");
        };

        let constraints = declaration.with_clauses().collect::<Vec<_>>();

        let [constraint] = constraints.as_slice() else {
            panic!("expected one static constraint clause: {constraints:?}");
        };

        let expressions = constraint.expressions().collect::<Vec<_>>();

        let [expression] = expressions.as_slice() else {
            panic!("expected one static predicate expression: {expressions:?}");
        };

        assert_eq!(expression.full_text(), "T: Copyable");
        assert_eq!(expression.type_expressions().count(), 1);
        assert_eq!(expression.trait_applications().count(), 1);

        assert!(
            result.diagnostics().is_empty(),
            "{:#?}",
            result.diagnostics()
        );
    }
}
