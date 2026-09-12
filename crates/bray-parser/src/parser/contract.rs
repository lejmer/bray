use bray_syntax::{
    CallableContractDeclarationSyntaxBuilder, DestructorMemberDeclarationSyntaxBuilder,
    EnsuresClauseSyntax, EnsuresClauseSyntaxBuilder, ExecutesClauseSyntax,
    ExecutesClauseSyntaxBuilder, ExecutionPropertySyntax, ExpressionSyntax,
    FinalizerMemberDeclarationSyntaxBuilder, FunctionDeclarationSyntaxBuilder,
    InherentImplementationDeclarationSyntaxBuilder, LambdaExpressionSyntaxBuilder,
    NamedTraitImplementationDeclarationSyntaxBuilder, PathSyntax, RequiresClauseSyntax,
    RequiresClauseSyntaxBuilder, ScopeEnterMemberDeclarationSyntaxBuilder,
    ScopeExitMemberDeclarationSyntaxBuilder, StaticDeclarationSyntaxBuilder,
    StructDeclarationSyntaxBuilder, SyntaxKind, SyntaxToken,
    TraitCallableMemberDeclarationSyntaxBuilder, TraitDeclarationSyntaxBuilder,
    TraitDestructorRequirementDeclarationSyntaxBuilder,
    TraitFinalizerRequirementDeclarationSyntaxBuilder,
    TraitScopeEnterRequirementDeclarationSyntaxBuilder,
    TraitScopeExitRequirementDeclarationSyntaxBuilder, TypeCallableMemberDeclarationSyntaxBuilder,
    TypeConstructorMemberDeclarationSyntaxBuilder, TypeExpressionSyntaxBuilder,
    UnionDeclarationSyntaxBuilder, UnnamedTraitImplementationDeclarationSyntaxBuilder,
    UsesClauseSyntax, UsesClauseSyntaxBuilder, WhenClauseSyntax, WithClauseSyntax,
    WithClauseSyntaxBuilder,
};

use super::delimiter::DelimiterDepth;
use super::expression::EXPRESSION_START_KINDS;
use super::member::MEMBER_KEYWORD_RECOVERY_KINDS;
use super::module::MODULE_ITEM_START_KINDS;
use super::recovery::RecoverySyntaxSink;
use super::separated::{SeparatedListSpec, SeparatedListSyntaxSink, separated_list_recovery_kinds};
use super::state::Parser;

type ContractBoundary = fn(&mut Parser) -> bool;

const CALLABLE_CONTRACT_CLAUSE_START_KINDS: [SyntaxKind; 6] = [
    SyntaxKind::RequiresKeyword,
    SyntaxKind::EnsuresKeyword,
    SyntaxKind::ExecutesKeyword,
    SyntaxKind::WhenKeyword,
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

const CONTRACT_CLAUSE_RECOVERY_KINDS: [SyntaxKind; 38] = [
    SyntaxKind::CloseParenToken,
    SyntaxKind::RequiresKeyword,
    SyntaxKind::EnsuresKeyword,
    SyntaxKind::ExecutesKeyword,
    SyntaxKind::WhenKeyword,
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
                SyntaxKind::ExecutesKeyword => {
                    builder.push_executes_clause(self.parse_executes_clause(at_boundary));
                }
                SyntaxKind::WhenKeyword => {
                    builder.push_when_clause(self.parse_when_clause());
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

    fn parse_executes_clause(&mut self, at_boundary: ContractBoundary) -> ExecutesClauseSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ExecutesClauseSyntax::builder(self.syntax_source(), start);

        builder.push_executes_keyword(self.expect(SyntaxKind::ExecutesKeyword));

        self.parse_parenthesized_contract_clause_arguments(
            &mut builder,
            at_boundary,
            &PATH_START_KINDS,
            Parser::parse_execution_property,
        );

        builder.build()
    }

    fn parse_execution_property(
        &mut self,
        _at_boundary: ContractBoundary,
    ) -> ExecutionPropertySyntax {
        let start = self.peek().full_range().start();
        let mut builder = ExecutionPropertySyntax::builder(self.syntax_source(), start);

        builder.push_identifier_token(self.expect(SyntaxKind::IdentifierToken));

        builder.build()
    }

    fn parse_when_clause(&mut self) -> WhenClauseSyntax {
        let start = self.peek().full_range().start();
        let mut builder = WhenClauseSyntax::builder(self.syntax_source(), start);

        if !self.try_enter_syntax_nesting() {
            let mut depth = DelimiterDepth::default();
            let mut closed = false;

            self.recover_current_and_until_predicate(&mut builder, |parser| {
                if closed
                    || parser.at_any(&MODULE_ITEM_START_KINDS)
                    || parser.at_any(&MEMBER_KEYWORD_RECOVERY_KINDS)
                {
                    return true;
                }

                match parser.peek().kind() {
                    kind @ (SyntaxKind::OpenBraceToken | SyntaxKind::CloseBraceToken) => {
                        if kind == SyntaxKind::CloseBraceToken && depth.is_at_root() {
                            return true;
                        }

                        depth.observe_grouping(kind);
                        closed = kind == SyntaxKind::CloseBraceToken && depth.is_at_root();
                    }
                    _ => {}
                }

                false
            });

            return builder.build();
        }

        builder.push_when_keyword(self.expect(SyntaxKind::WhenKeyword));
        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));

        builder
            .push_condition(self.parse_contract_clause_expression(Parser::at_guarantee_boundary));

        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));
        builder.push_open_brace_token(self.expect(SyntaxKind::OpenBraceToken));

        while !self.at_guarantee_group_end() {
            match self.peek().kind() {
                SyntaxKind::EnsuresKeyword => {
                    builder.push_ensures_clause(
                        self.parse_ensures_clause(Parser::at_guarantee_boundary),
                    );
                }
                SyntaxKind::ExecutesKeyword => {
                    builder.push_executes_clause(
                        self.parse_executes_clause(Parser::at_guarantee_boundary),
                    );
                }
                SyntaxKind::WhenKeyword => builder.push_when_clause(self.parse_when_clause()),
                _ => {
                    let actual = self.peek();

                    let diagnostic = crate::diagnostic::expected_operator(
                        &self.syntax_source(),
                        SyntaxKind::CloseBraceToken,
                        &actual,
                    );

                    self.record_syntax_diagnostic(diagnostic);

                    self.recover_current_and_until_balanced_close_brace_or_recovery_set(
                        &mut builder,
                        crate::cursor::RecoverySet::new(&[
                            SyntaxKind::EnsuresKeyword,
                            SyntaxKind::ExecutesKeyword,
                            SyntaxKind::WhenKeyword,
                        ])
                        .with_additional(&CONTRACT_CLAUSE_RECOVERY_KINDS),
                    );
                }
            }
        }

        builder.push_close_brace_token(self.expect(SyntaxKind::CloseBraceToken));
        self.leave_syntax_nesting();

        builder.build()
    }

    fn at_guarantee_group_end(&mut self) -> bool {
        self.at_any(&[
            SyntaxKind::CloseBraceToken,
            SyntaxKind::OpenBraceToken,
            SyntaxKind::EndOfFileToken,
        ]) || self.at_any(&MODULE_ITEM_START_KINDS)
            || self.at_any(&MEMBER_KEYWORD_RECOVERY_KINDS)
    }

    fn at_guarantee_boundary(&mut self) -> bool {
        self.at_guarantee_group_end() || self.at(SyntaxKind::SemicolonToken)
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
        if self.scan_ahead(|scan| scan.scan_explicit_type_equality(at_boundary)) {
            return self.parse_explicit_type_equality(at_boundary);
        }

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

    fn scan_explicit_type_equality(&mut self, at_boundary: ContractBoundary) -> bool {
        let mut at_left_boundary = |parser: &mut Parser| {
            parser.at(SyntaxKind::EqualsEqualsToken)
                || parser.at(SyntaxKind::CommaToken)
                || parser.at_contract_clause_sequence_end(at_boundary, &EXPRESSION_START_KINDS)
        };

        let left = self.parse_type_expression_until(&mut at_left_boundary);

        self.at(SyntaxKind::EqualsEqualsToken) && has_explicit_type_syntax(&left)
    }

    fn parse_explicit_type_equality(&mut self, at_boundary: ContractBoundary) -> ExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        let mut at_left_boundary = |parser: &mut Parser| {
            parser.at(SyntaxKind::EqualsEqualsToken)
                || parser.at(SyntaxKind::CommaToken)
                || parser.at_contract_clause_sequence_end(at_boundary, &EXPRESSION_START_KINDS)
        };

        builder.push_type_expression(self.parse_type_expression_until(&mut at_left_boundary));
        builder.push_operator_token(self.expect(SyntaxKind::EqualsEqualsToken));

        let mut at_right_boundary = |parser: &mut Parser| {
            parser.at(SyntaxKind::CommaToken)
                || parser.at_contract_clause_sequence_end(at_boundary, &EXPRESSION_START_KINDS)
        };

        builder.push_type_expression(self.parse_type_expression_until(&mut at_right_boundary));

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

fn has_explicit_type_syntax(syntax: &bray_syntax::TypeExpressionSyntax) -> bool {
    syntax.generic_argument_lists().next().is_some()
        || syntax.trait_applications().next().is_some()
        || syntax.type_expressions().next().is_some()
        || syntax.func_keyword().is_some()
        || syntax.unit_keyword().is_some()
        || syntax.self_keyword().is_some()
        || syntax.ampersand_token().is_some()
        || syntax.box_keyword().is_some()
        || syntax.view_keyword().is_some()
        || syntax.open_bracket_token().is_some()
        || syntax.open_paren_token().is_some()
}

pub(super) trait CallableContractClauseSyntaxSink: RecoverySyntaxSink {
    fn push_requires_clause(&mut self, clause: RequiresClauseSyntax);

    fn push_ensures_clause(&mut self, clause: EnsuresClauseSyntax);

    fn push_executes_clause(&mut self, clause: ExecutesClauseSyntax);

    fn push_when_clause(&mut self, clause: WhenClauseSyntax);

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

impl SeparatedListSyntaxSink<ExecutionPropertySyntax> for ExecutesClauseSyntaxBuilder {
    fn push_item(&mut self, item: ExecutionPropertySyntax) {
        self.push_property(item);
    }

    fn push_separator(&mut self, separator: SyntaxToken) {
        self.push_separator_token(separator);
    }
}

impl ParenthesizedContractClauseSyntaxSink<ExecutionPropertySyntax>
    for ExecutesClauseSyntaxBuilder
{
    fn push_open_paren_token(&mut self, token: SyntaxToken) {
        ExecutesClauseSyntaxBuilder::push_open_paren_token(self, token);
    }

    fn push_close_paren_token(&mut self, token: SyntaxToken) {
        ExecutesClauseSyntaxBuilder::push_close_paren_token(self, token);
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

            fn push_executes_clause(&mut self, clause: ExecutesClauseSyntax) {
                <$builder>::push_executes_clause(self, clause);
            }

            fn push_when_clause(&mut self, clause: WhenClauseSyntax) {
                <$builder>::push_when_clause(self, clause);
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
impl_with_clause_sink!(StaticDeclarationSyntaxBuilder);
impl_with_clause_sink!(StructDeclarationSyntaxBuilder);
impl_with_clause_sink!(UnionDeclarationSyntaxBuilder);
impl_with_clause_sink!(TraitDeclarationSyntaxBuilder);
impl_with_clause_sink!(InherentImplementationDeclarationSyntaxBuilder);
impl_with_clause_sink!(UnnamedTraitImplementationDeclarationSyntaxBuilder);
impl_with_clause_sink!(NamedTraitImplementationDeclarationSyntaxBuilder);

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::SeparatedSyntaxNode;
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::parser::parse_compilation_unit;
    use crate::test_support::parse_diagnostic_kinds;

    #[test]
    fn guarded_guarantees_preserve_nested_clauses_and_the_ordinary_body() {
        let source = r#"
        module main;
        func check(pos ready: bool) -> unit requires(true) when(ready)
        {
            executes(pure, total,) ensures(result == unit) when(true)
            {
                executes(total)
            }
        }
        executes(total)
        {
            return unit;
        }
        "#;

        let result = parse_compilation_unit(&source_store([source]));
        let unit = &result.syntax_tree().root().source_units()[0];
        let declaration = unit.function_declarations().next().unwrap();
        let group = declaration.when_clauses().next().unwrap();

        assert!(
            result.diagnostics().is_empty(),
            "{:?}",
            result.diagnostics()
        );

        assert_eq!(unit.full_text(), source);
        assert_eq!(group.condition().full_text().trim(), "ready");

        assert_eq!(
            group
                .executes_clauses()
                .next()
                .unwrap()
                .properties()
                .count(),
            2
        );

        assert_eq!(group.ensures_clauses().count(), 1);
        assert_eq!(group.when_clauses().count(), 1);
        assert_eq!(declaration.executes_clauses().count(), 1);
        assert_eq!(declaration.ensures_clauses().count(), 0);
    }

    #[test]
    fn execution_guarantees_are_shared_by_every_callable_form() {
        let source = bray_testing::EXECUTION_GUARANTEES_SOURCE;

        let result = parse_compilation_unit(&source_store([source]));

        assert!(
            result.diagnostics().is_empty(),
            "{:?}",
            result.diagnostics()
        );

        assert_eq!(
            result.syntax_tree().root().source_units()[0].full_text(),
            source
        );
    }

    #[test]
    fn execution_property_lists_do_not_accept_expressions() {
        for properties in ["pure()", "pure.total", "{ true }", "pure && total", ""] {
            let source = format!(
                r#"
                module main;
                func check() executes({properties})
                {{
                }}
                func next()
                {{
                }}
                "#
            );

            let result = parse_compilation_unit(&source_store([source.as_str()]));

            assert!(!result.diagnostics().is_empty(), "accepted {properties}");

            assert_eq!(
                result.syntax_tree().root().source_units()[0]
                    .function_declarations()
                    .count(),
                2,
                "{properties}"
            );

            assert_eq!(
                result.syntax_tree().root().source_units()[0].full_text(),
                source
            );
        }
    }

    #[test]
    fn guarded_groups_do_not_accept_executable_code_or_requirements() {
        for contents in [
            "return unit;",
            "yield true;",
            "= true;",
            "let value = true;",
            "requires(true)",
            "uses(io)",
            "with(true)",
        ] {
            let source = format!(
                r#"
                module main;
                func check() when(true)
                {{
                    {contents}
                }}
                {{
                }}
                func next()
                {{
                }}
                "#
            );

            let result = parse_compilation_unit(&source_store([source.as_str()]));
            let unit = &result.syntax_tree().root().source_units()[0];

            let diagnostic = result.diagnostics().iter().next().unwrap();

            assert_eq!(diagnostic.kind(), DiagnosticKind::SyntaxExpectedToken);

            assert!(diagnostic.args().contains(
                &bray_diagnostics::DiagnosticArg::expected_syntax_kind(SyntaxKind::CloseBraceToken)
            ));

            assert_eq!(
                diagnostic.primary_span().unwrap().range().start().bytes(),
                u32::try_from(source.find(contents).unwrap()).unwrap()
            );

            assert_eq!(
                diagnostic.labels()[0].kind(),
                bray_diagnostics::DiagnosticLabelKind::InvalidOperatorOrPunctuation
            );

            assert_eq!(unit.full_text(), source);
            assert_eq!(unit.function_declarations().count(), 2);
        }
    }

    #[test]
    fn malformed_guarantee_groups_preserve_following_declarations() {
        for header in [
            "when(true) { executes(total)",
            "when(true) { let value = true;",
            "when(true) { when(true) { executes(total)",
            "when(true)",
            "when(true",
            "when(",
            "when(true) { ensures(",
        ] {
            let source = format!("module main; func broken() {header} func next() {{}}");
            let result = parse_compilation_unit(&source_store([source.as_str()]));
            let unit = &result.syntax_tree().root().source_units()[0];

            assert!(!result.diagnostics().is_empty(), "accepted {header}");
            assert_eq!(unit.full_text(), source);
            assert_eq!(unit.function_declarations().count(), 2, "{header}");
        }
    }

    #[test]
    fn unterminated_overdeep_guarantee_groups_preserve_following_declarations() {
        let source = format!(
            "module main; func broken() {}executes(total) func next() {{}}",
            "when(true) {".repeat(140),
        );

        let result = parse_compilation_unit(&source_store([source.as_str()]));
        let unit = &result.syntax_tree().root().source_units()[0];

        assert!(
            parse_diagnostic_kinds(&result).contains(&DiagnosticKind::SyntaxNestingLimitExceeded)
        );

        assert_eq!(unit.full_text(), source);
        assert_eq!(unit.function_declarations().count(), 2);
    }

    #[test]
    fn guarded_group_nesting_uses_the_parser_depth_limit() {
        let source = format!(
            "module main; func check() {}executes(total){} {{}} func next() {{}}",
            "when(true) {".repeat(140),
            "}".repeat(140)
        );

        let result = parse_compilation_unit(&source_store([source.as_str()]));

        assert!(
            parse_diagnostic_kinds(&result).contains(&DiagnosticKind::SyntaxNestingLimitExceeded)
        );

        assert_eq!(
            result.syntax_tree().root().source_units()[0]
                .function_declarations()
                .count(),
            2
        );

        assert_eq!(
            result.syntax_tree().root().source_units()[0].full_text(),
            source
        );
    }

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
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxExpectedExpression,
            ]
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
