use bray_syntax::{
    CallableResultClauseSyntax, GenericParameterListSyntax, ParameterListSyntax, SyntaxKind,
    SyntaxToken, TraitCallableMemberDeclarationSyntax, TraitCallableMemberDeclarationSyntaxBuilder,
    TraitCallableMemberModifiersSyntax, TypeCallableMemberDeclarationSyntax,
    TypeCallableMemberDeclarationSyntaxBuilder, TypeCallableMemberModifiersSyntax,
};

use crate::cursor::RecoverySet;
use crate::parser::member::body::{MEMBER_ITEM_RECOVERY_KINDS, MEMBER_KEYWORD_RECOVERY_KINDS};
use crate::parser::recovery::RecoverySyntaxSink;
use crate::parser::state::Parser;

const TYPE_CALLABLE_MEMBER_START_KINDS: [SyntaxKind; 9] = [
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::ConstKeyword,
    SyntaxKind::StaticKeyword,
    SyntaxKind::ConsumeKeyword,
    SyntaxKind::MutKeyword,
    SyntaxKind::FuncKeyword,
];

const TRAIT_CALLABLE_MEMBER_START_KINDS: [SyntaxKind; 7] = [
    SyntaxKind::AsyncKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::ConstKeyword,
    SyntaxKind::StaticKeyword,
    SyntaxKind::ConsumeKeyword,
    SyntaxKind::MutKeyword,
    SyntaxKind::FuncKeyword,
];

const CALLABLE_MEMBER_CONTRACT_BOUNDARY_KINDS: [SyntaxKind; 13] = [
    SyntaxKind::RequiresKeyword,
    SyntaxKind::EnsuresKeyword,
    SyntaxKind::WithKeyword,
    SyntaxKind::UsesKeyword,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::StaticKeyword,
    SyntaxKind::ConsumeKeyword,
    SyntaxKind::FuncKeyword,
];

const CALLABLE_MEMBER_BODY_MISSING_BOUNDARY_KINDS: [SyntaxKind; 17] = [
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
    SyntaxKind::AtToken,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::ConstKeyword,
    SyntaxKind::StaticKeyword,
    SyntaxKind::ConsumeKeyword,
    SyntaxKind::MutKeyword,
    SyntaxKind::FuncKeyword,
    SyntaxKind::IdentifierToken,
    SyntaxKind::ConstructKeyword,
    SyntaxKind::FinalizeKeyword,
    SyntaxKind::DestructKeyword,
];

const TRAIT_CALLABLE_MEMBER_DECLARATION_END_KINDS: [SyntaxKind; 9] = [
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::ConstKeyword,
    SyntaxKind::StaticKeyword,
    SyntaxKind::ConsumeKeyword,
    SyntaxKind::FuncKeyword,
];

impl Parser {
    pub(super) fn parse_type_callable_member_declaration(
        &mut self,
    ) -> TypeCallableMemberDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TypeCallableMemberDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_type_callable_member_modifiers(self.parse_type_callable_member_modifiers());
        self.parse_callable_member_header(&mut builder);

        builder.push_callable_body_block_expression(
            self.parse_callable_body_block_expression_until(
                Parser::at_callable_member_body_missing_boundary,
            ),
        );

        builder.build()
    }

    fn parse_type_callable_member_modifiers(&mut self) -> TypeCallableMemberModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TypeCallableMemberModifiersSyntax::builder(self.syntax_source(), start);

        while self.at_type_callable_member_modifier() {
            if self.at_visibility_modifier() {
                builder.push_visibility_token(self.parse_visibility_modifier());
                continue;
            }

            if self.at(SyntaxKind::AsyncKeyword) {
                builder.push_async_token(self.expect(SyntaxKind::AsyncKeyword));
                continue;
            }

            if self.at(SyntaxKind::TrustedKeyword) {
                builder.push_trusted_token(self.expect(SyntaxKind::TrustedKeyword));
                continue;
            }

            if self.at(SyntaxKind::ConstKeyword) {
                builder.push_const_token(self.expect(SyntaxKind::ConstKeyword));
                continue;
            }

            if self.at(SyntaxKind::StaticKeyword) {
                builder.push_static_token(self.expect(SyntaxKind::StaticKeyword));
                continue;
            }

            if self.at(SyntaxKind::ConsumeKeyword) {
                builder.push_consume_token(self.expect(SyntaxKind::ConsumeKeyword));
                continue;
            }

            builder.push_mut_token(self.expect(SyntaxKind::MutKeyword));
        }

        builder.build()
    }

    pub(super) fn parse_trait_callable_member_declaration(
        &mut self,
    ) -> TraitCallableMemberDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder =
            TraitCallableMemberDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_trait_callable_member_modifiers(self.parse_trait_callable_member_modifiers());
        self.parse_callable_member_header(&mut builder);
        self.parse_trait_callable_member_tail(&mut builder);

        builder.build()
    }

    fn parse_trait_callable_member_modifiers(&mut self) -> TraitCallableMemberModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TraitCallableMemberModifiersSyntax::builder(self.syntax_source(), start);

        while self.at_trait_callable_member_modifier() {
            if self.at(SyntaxKind::AsyncKeyword) {
                builder.push_async_token(self.expect(SyntaxKind::AsyncKeyword));
                continue;
            }

            if self.at(SyntaxKind::TrustedKeyword) {
                builder.push_trusted_token(self.expect(SyntaxKind::TrustedKeyword));
                continue;
            }

            if self.at(SyntaxKind::ConstKeyword) {
                builder.push_const_token(self.expect(SyntaxKind::ConstKeyword));
                continue;
            }

            if self.at(SyntaxKind::StaticKeyword) {
                builder.push_static_token(self.expect(SyntaxKind::StaticKeyword));
                continue;
            }

            if self.at(SyntaxKind::ConsumeKeyword) {
                builder.push_consume_token(self.expect(SyntaxKind::ConsumeKeyword));
                continue;
            }

            builder.push_mut_token(self.expect(SyntaxKind::MutKeyword));
        }

        builder.build()
    }

    fn parse_callable_member_header(
        &mut self,
        builder: &mut impl CallableMemberDeclarationSyntaxSink,
    ) {
        builder.push_func_keyword(self.expect(SyntaxKind::FuncKeyword));
        builder.push_identifier_token(self.parse_identifier());

        if self.at(SyntaxKind::LessToken) {
            builder.push_generic_parameter_list(self.parse_generic_parameter_list());
        }

        builder.push_parameter_list(self.parse_parameter_list());

        if self.at(SyntaxKind::ArrowToken) {
            builder.push_callable_result_clause(self.parse_callable_result_clause());
        }

        self.parse_callable_contract_clauses(builder, Parser::at_callable_member_contract_boundary);
    }

    fn parse_trait_callable_member_tail(
        &mut self,
        builder: &mut TraitCallableMemberDeclarationSyntaxBuilder,
    ) {
        if self.at(SyntaxKind::OpenBraceToken) {
            builder.push_callable_body_block_expression(
                self.parse_callable_body_block_expression_until(
                    Parser::at_callable_member_body_missing_boundary,
                ),
            );
            return;
        }

        self.recover_until_trait_callable_member_declaration_end(builder);
        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));
    }

    fn recover_until_trait_callable_member_declaration_end(
        &mut self,
        builder: &mut TraitCallableMemberDeclarationSyntaxBuilder,
    ) {
        self.recover_until_set(
            builder,
            RecoverySet::new(&TRAIT_CALLABLE_MEMBER_DECLARATION_END_KINDS),
        );
    }

    fn at_callable_member_contract_boundary(&mut self) -> bool {
        self.at_any(&CALLABLE_MEMBER_CONTRACT_BOUNDARY_KINDS)
            || self.at_any(&MEMBER_KEYWORD_RECOVERY_KINDS)
    }

    fn at_callable_member_body_missing_boundary(&mut self) -> bool {
        self.at_any(&CALLABLE_MEMBER_BODY_MISSING_BOUNDARY_KINDS)
            || self.at_any(&MEMBER_ITEM_RECOVERY_KINDS)
    }

    pub(super) fn should_parse_type_callable_member_declaration(&mut self) -> bool {
        if !self.at_any(&TYPE_CALLABLE_MEMBER_START_KINDS) {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.consume_type_callable_member_modifiers_for_scan();

            scan.at(SyntaxKind::FuncKeyword)
        })
    }

    pub(super) fn should_parse_trait_callable_member_declaration(&mut self) -> bool {
        if !self.at_any(&TRAIT_CALLABLE_MEMBER_START_KINDS) {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.consume_trait_callable_member_modifiers_for_scan();

            scan.at(SyntaxKind::FuncKeyword)
        })
    }

    fn consume_type_callable_member_modifiers_for_scan(&mut self) {
        while self.at_type_callable_member_modifier() {
            self.consume();
        }
    }

    fn consume_trait_callable_member_modifiers_for_scan(&mut self) {
        while self.at_trait_callable_member_modifier() {
            self.consume();
        }
    }

    fn at_type_callable_member_modifier(&mut self) -> bool {
        self.at_visibility_modifier()
            || self.at(SyntaxKind::AsyncKeyword)
            || self.at(SyntaxKind::TrustedKeyword)
            || self.at(SyntaxKind::ConstKeyword)
            || self.at(SyntaxKind::StaticKeyword)
            || self.at(SyntaxKind::ConsumeKeyword)
            || self.at(SyntaxKind::MutKeyword)
    }

    fn at_trait_callable_member_modifier(&mut self) -> bool {
        self.at(SyntaxKind::AsyncKeyword)
            || self.at(SyntaxKind::TrustedKeyword)
            || self.at(SyntaxKind::ConstKeyword)
            || self.at(SyntaxKind::StaticKeyword)
            || self.at(SyntaxKind::ConsumeKeyword)
            || self.at(SyntaxKind::MutKeyword)
    }
}

trait CallableMemberDeclarationSyntaxSink: RecoverySyntaxSink {
    fn push_func_keyword(&mut self, token: SyntaxToken);

    fn push_identifier_token(&mut self, token: SyntaxToken);

    fn push_generic_parameter_list(&mut self, list: GenericParameterListSyntax);

    fn push_parameter_list(&mut self, parameter_list: ParameterListSyntax);

    fn push_callable_result_clause(&mut self, clause: CallableResultClauseSyntax);
}

impl CallableMemberDeclarationSyntaxSink for TypeCallableMemberDeclarationSyntaxBuilder {
    fn push_func_keyword(&mut self, token: SyntaxToken) {
        TypeCallableMemberDeclarationSyntaxBuilder::push_func_keyword(self, token);
    }

    fn push_identifier_token(&mut self, token: SyntaxToken) {
        TypeCallableMemberDeclarationSyntaxBuilder::push_identifier_token(self, token);
    }

    fn push_generic_parameter_list(&mut self, list: GenericParameterListSyntax) {
        TypeCallableMemberDeclarationSyntaxBuilder::push_generic_parameter_list(self, list);
    }

    fn push_parameter_list(&mut self, parameter_list: ParameterListSyntax) {
        TypeCallableMemberDeclarationSyntaxBuilder::push_parameter_list(self, parameter_list);
    }

    fn push_callable_result_clause(&mut self, clause: CallableResultClauseSyntax) {
        TypeCallableMemberDeclarationSyntaxBuilder::push_callable_result_clause(self, clause);
    }
}

impl CallableMemberDeclarationSyntaxSink for TraitCallableMemberDeclarationSyntaxBuilder {
    fn push_func_keyword(&mut self, token: SyntaxToken) {
        TraitCallableMemberDeclarationSyntaxBuilder::push_func_keyword(self, token);
    }

    fn push_identifier_token(&mut self, token: SyntaxToken) {
        TraitCallableMemberDeclarationSyntaxBuilder::push_identifier_token(self, token);
    }

    fn push_generic_parameter_list(&mut self, list: GenericParameterListSyntax) {
        TraitCallableMemberDeclarationSyntaxBuilder::push_generic_parameter_list(self, list);
    }

    fn push_parameter_list(&mut self, parameter_list: ParameterListSyntax) {
        TraitCallableMemberDeclarationSyntaxBuilder::push_parameter_list(self, parameter_list);
    }

    fn push_callable_result_clause(&mut self, clause: CallableResultClauseSyntax) {
        TraitCallableMemberDeclarationSyntaxBuilder::push_callable_result_clause(self, clause);
    }
}
