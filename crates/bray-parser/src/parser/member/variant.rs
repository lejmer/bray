use bray_syntax::{
    PayloadFieldModifiersSyntax, SyntaxKind, SyntaxToken, UnionPayloadFieldSyntax,
    UnionVariantDeclarationSyntax, UnionVariantDeclarationSyntaxBuilder, UnionVariantPayloadSyntax,
    UnionVariantPayloadSyntaxBuilder, VariantDirectivesSyntax, VariantDirectivesSyntaxBuilder,
};

use crate::cursor::RecoverySet;
use crate::parser::directive::{DirectiveScanKind, TAG_DIRECTIVE_NAME};
use crate::parser::member::body::MEMBER_ITEM_RECOVERY_KINDS;
use crate::parser::separated::{
    SeparatedListSpec, SeparatedListSyntaxSink, separated_list_recovery_kinds,
};
use crate::parser::state::Parser;

const UNION_VARIANT_DECLARATION_START_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::AtToken,
    SyntaxKind::IdentifierToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const VARIANT_DIRECTIVE_ARGUMENT_RECOVERY_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::AtToken,
    SyntaxKind::IdentifierToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const UNION_VARIANT_AFTER_NAME_BOUNDARY_KINDS: [SyntaxKind; 4] = [
    SyntaxKind::OpenParenToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const UNION_VARIANT_PAYLOAD_TERMINATORS: [SyntaxKind; 4] = [
    SyntaxKind::CloseParenToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const UNION_PAYLOAD_FIELD_START_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::PosKeyword,
    SyntaxKind::MutKeyword,
    SyntaxKind::IdentifierToken,
];

const UNION_PAYLOAD_FIELD_TYPE_BOUNDARY_KINDS: [SyntaxKind; 6] = [
    SyntaxKind::EqualsToken,
    SyntaxKind::CommaToken,
    SyntaxKind::CloseParenToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const UNION_PAYLOAD_FIELD_DEFAULT_BOUNDARY_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::CommaToken,
    SyntaxKind::CloseParenToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

impl Parser {
    pub(super) fn parse_union_variant_declaration(&mut self) -> UnionVariantDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = UnionVariantDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_variant_directives(self.parse_variant_directives());
        builder.push_identifier_token(self.parse_identifier());

        if self.at(SyntaxKind::LessToken) {
            self.recover_current_and_until(&mut builder, &UNION_VARIANT_AFTER_NAME_BOUNDARY_KINDS);
        }

        if self.at(SyntaxKind::OpenParenToken) {
            builder.push_union_variant_payload(self.parse_union_variant_payload());
        }

        self.recover_until_union_variant_declaration_end(&mut builder);
        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));

        builder.build()
    }

    fn parse_variant_directives(&mut self) -> VariantDirectivesSyntax {
        let start = self.peek().full_range().start();
        let mut builder = VariantDirectivesSyntax::builder(self.syntax_source(), start);

        while self.at(SyntaxKind::AtToken) {
            if self.at_directive_name(TAG_DIRECTIVE_NAME) {
                builder.push_tag_directive(
                    self.parse_tag_directive(&VARIANT_DIRECTIVE_ARGUMENT_RECOVERY_KINDS),
                );
                continue;
            }

            self.recover_unknown_variant_directive(&mut builder);
        }

        builder.build()
    }

    fn recover_unknown_variant_directive(&mut self, builder: &mut VariantDirectivesSyntaxBuilder) {
        self.recover_current_and_until(builder, &UNION_VARIANT_DECLARATION_START_KINDS);
    }

    fn parse_union_variant_payload(&mut self) -> UnionVariantPayloadSyntax {
        let start = self.peek().full_range().start();
        let recovery_kinds = separated_list_recovery_kinds(
            &UNION_PAYLOAD_FIELD_START_KINDS,
            SyntaxKind::CommaToken,
            &UNION_VARIANT_PAYLOAD_TERMINATORS,
        );

        let spec = SeparatedListSpec {
            separator_kind: SyntaxKind::CommaToken,
            terminators: &UNION_VARIANT_PAYLOAD_TERMINATORS,
            recovery_kinds: &recovery_kinds,
            allow_trailing_separator: true,
        };

        let mut builder = UnionVariantPayloadSyntax::builder(self.syntax_source(), start);

        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));
        self.parse_separated_list(&mut builder, spec, Parser::parse_union_payload_field);
        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        builder.build()
    }

    pub(in crate::parser) fn parse_union_payload_field(&mut self) -> UnionPayloadFieldSyntax {
        let start = self.peek().full_range().start();
        let mut builder = UnionPayloadFieldSyntax::builder(self.syntax_source(), start);

        builder.push_payload_field_modifiers(self.parse_payload_field_modifiers());

        let mut at_type_boundary = Parser::at_union_payload_field_type_boundary;

        builder.push_typed_identifier(self.parse_typed_identifier_until(&mut at_type_boundary));

        if self.at(SyntaxKind::EqualsToken) {
            builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));
            let mut at_default_boundary = Parser::at_union_payload_field_default_boundary;
            builder.push_expression(self.parse_expression_until(&mut at_default_boundary));
        }

        builder.build()
    }

    fn parse_payload_field_modifiers(&mut self) -> PayloadFieldModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder = PayloadFieldModifiersSyntax::builder(self.syntax_source(), start);

        if self.at(SyntaxKind::PosKeyword) {
            builder.push_pos_token(self.expect(SyntaxKind::PosKeyword));
        }

        if self.at(SyntaxKind::MutKeyword) {
            builder.push_mut_token(self.expect(SyntaxKind::MutKeyword));
        }

        builder.build()
    }

    fn at_union_payload_field_type_boundary(&mut self) -> bool {
        self.at_any(&UNION_PAYLOAD_FIELD_TYPE_BOUNDARY_KINDS)
            || self.at_probable_union_payload_field_start()
    }

    fn at_union_payload_field_default_boundary(&mut self) -> bool {
        self.at_any(&UNION_PAYLOAD_FIELD_DEFAULT_BOUNDARY_KINDS)
            || self.at_probable_union_payload_field_start()
    }

    fn at_probable_union_payload_field_start(&mut self) -> bool {
        self.scan_ahead(|scan| {
            scan.consume_payload_field_modifiers_for_scan();

            scan.at(SyntaxKind::IdentifierToken)
                && scan.lookahead(1).kind() == SyntaxKind::ColonToken
        })
    }

    fn consume_payload_field_modifiers_for_scan(&mut self) {
        if self.at(SyntaxKind::PosKeyword) {
            self.consume();
        }

        if self.at(SyntaxKind::MutKeyword) {
            self.consume();
        }
    }

    fn recover_until_union_variant_declaration_end(
        &mut self,
        builder: &mut UnionVariantDeclarationSyntaxBuilder,
    ) {
        let recovery_set = RecoverySet::new(&[
            SyntaxKind::SemicolonToken,
            SyntaxKind::CloseBraceToken,
            SyntaxKind::EndOfFileToken,
        ])
        .with_additional(&MEMBER_ITEM_RECOVERY_KINDS);

        self.recover_until_set(builder, recovery_set);
    }

    pub(super) fn should_parse_union_variant_declaration(&mut self) -> bool {
        if !self.at(SyntaxKind::AtToken) && !self.at(SyntaxKind::IdentifierToken) {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.consume_variant_directives_for_scan();

            scan.at(SyntaxKind::IdentifierToken)
        })
    }

    fn consume_variant_directives_for_scan(&mut self) {
        self.consume_directives_for_scan(
            &UNION_VARIANT_DECLARATION_START_KINDS,
            |directive_name| match directive_name {
                TAG_DIRECTIVE_NAME => DirectiveScanKind::ArgumentList,
                _ => DirectiveScanKind::Unknown,
            },
        );
    }
}

impl SeparatedListSyntaxSink<UnionPayloadFieldSyntax> for UnionVariantPayloadSyntaxBuilder {
    fn push_item(&mut self, item: UnionPayloadFieldSyntax) {
        UnionVariantPayloadSyntaxBuilder::push_union_payload_field(self, item);
    }

    fn push_separator(&mut self, separator: SyntaxToken) {
        UnionVariantPayloadSyntaxBuilder::push_separator_token(self, separator);
    }
}
