use bray_syntax::{
    FieldModifiersSyntax, StructFieldDeclarationSyntax, StructFieldDeclarationSyntaxBuilder,
    SyntaxKind,
};

use crate::parser::state::Parser;

const STRUCT_FIELD_TYPE_BOUNDARY_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::EqualsToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
    SyntaxKind::FuncKeyword,
];

const STRUCT_FIELD_DEFAULT_BOUNDARY_KINDS: [SyntaxKind; 4] = [
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
    SyntaxKind::FuncKeyword,
];

impl Parser {
    pub(super) fn parse_struct_field_declaration(&mut self) -> StructFieldDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = StructFieldDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_field_modifiers(self.parse_field_modifiers());
        builder.push_identifier_token(self.parse_identifier());
        builder.push_colon_token(self.expect(SyntaxKind::ColonToken));

        // TODO(parser): Parse field type expressions once expression parsing is implemented.
        self.recover_struct_field_type_expression(&mut builder);

        if self.at(SyntaxKind::EqualsToken) {
            builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));
            // TODO(parser): Parse field default expressions once expression parsing is implemented.
            self.recover_struct_field_default_expression(&mut builder);
        }

        self.recover_until_struct_field_declaration_end(&mut builder);
        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));

        builder.build()
    }

    fn parse_field_modifiers(&mut self) -> FieldModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder = FieldModifiersSyntax::builder(self.syntax_source(), start);

        if self.at_visibility_modifier() {
            builder.push_visibility_token(self.parse_visibility_modifier());
        }

        if self.at(SyntaxKind::MutKeyword) {
            builder.push_mut_token(self.expect(SyntaxKind::MutKeyword));
        }

        builder.build()
    }

    fn recover_struct_field_type_expression(
        &mut self,
        builder: &mut StructFieldDeclarationSyntaxBuilder,
    ) {
        if self.at_struct_field_type_boundary() {
            return;
        }

        self.recover_current_and_until_predicate(builder, Parser::at_struct_field_type_boundary);
    }

    fn recover_struct_field_default_expression(
        &mut self,
        builder: &mut StructFieldDeclarationSyntaxBuilder,
    ) {
        if self.at_struct_field_default_boundary() {
            return;
        }

        self.recover_current_and_until_predicate(builder, Parser::at_struct_field_default_boundary);
    }

    fn recover_until_struct_field_declaration_end(
        &mut self,
        builder: &mut StructFieldDeclarationSyntaxBuilder,
    ) {
        self.recover_until_predicate(builder, Parser::at_struct_field_declaration_end);
    }

    fn at_struct_field_declaration_end(&mut self) -> bool {
        self.at(SyntaxKind::SemicolonToken)
            || self.at(SyntaxKind::CloseBraceToken)
            || self.at(SyntaxKind::EndOfFileToken)
            || self.at_probable_struct_field_start()
            || self.at_following_type_member_start()
    }

    fn at_struct_field_type_boundary(&mut self) -> bool {
        self.at_any(&STRUCT_FIELD_TYPE_BOUNDARY_KINDS)
            || self.at_probable_struct_field_start()
            || self.at_following_type_member_start()
    }

    fn at_struct_field_default_boundary(&mut self) -> bool {
        self.at_any(&STRUCT_FIELD_DEFAULT_BOUNDARY_KINDS)
            || self.at_probable_struct_field_start()
            || self.at_following_type_member_start()
    }

    fn at_following_type_member_start(&mut self) -> bool {
        self.should_parse_type_callable_member_declaration()
            || self.should_parse_type_constructor_member_declaration()
            || self.should_parse_type_lifecycle_member_declaration()
            || self.should_parse_constant_declaration()
    }

    pub(super) fn should_parse_struct_field_declaration(&mut self) -> bool {
        self.scan_ahead(|scan| {
            scan.consume_field_modifiers_for_scan();

            scan.at(SyntaxKind::IdentifierToken)
        })
    }

    fn at_probable_struct_field_start(&mut self) -> bool {
        self.scan_ahead(|scan| {
            scan.consume_field_modifiers_for_scan();

            scan.at(SyntaxKind::IdentifierToken)
                && scan.lookahead(1).kind() == SyntaxKind::ColonToken
        })
    }

    fn consume_field_modifiers_for_scan(&mut self) {
        if self.at_visibility_modifier() {
            self.consume();
        }

        if self.at(SyntaxKind::MutKeyword) {
            self.consume();
        }
    }
}
