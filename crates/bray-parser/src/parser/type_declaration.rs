use bray_syntax::{
    StructBodySyntax, StructDeclarationSyntax, StructDeclarationSyntaxBuilder, SyntaxKind,
    SyntaxToken, TypeDirectivesSyntax, TypeDirectivesSyntaxBuilder, TypeModifiersSyntax,
    UnionBodySyntax, UnionDeclarationSyntax, UnionDeclarationSyntaxBuilder,
};

use super::directive::{COPY_DIRECTIVE_NAME, LAYOUT_DIRECTIVE_NAME};
use super::module::MODULE_ITEM_START_KINDS;
use super::recovery::RecoverySyntaxSink;
use super::state::Parser;

const TYPE_DECLARATION_START_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::AtToken,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::StructKeyword,
    SyntaxKind::UnionKeyword,
];

const TYPE_HEADER_START_KINDS: [SyntaxKind; 4] = [
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::StructKeyword,
    SyntaxKind::UnionKeyword,
];

const TYPE_DIRECTIVE_ARGUMENT_RECOVERY_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::AtToken,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::StructKeyword,
    SyntaxKind::UnionKeyword,
];

const TYPE_AFTER_NAME_BOUNDARY_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::WithKeyword,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const TYPE_CONSTRAINT_BOUNDARY_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::WithKeyword,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const TYPE_BODY_MISSING_BOUNDARY_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

impl Parser {
    pub(super) fn parse_struct_declaration(&mut self) -> StructDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = StructDeclarationSyntax::builder(self.syntax_source(), start);

        self.parse_type_declaration_header(&mut builder, SyntaxKind::StructKeyword);
        builder.push_struct_body(self.parse_struct_body());

        builder.build()
    }

    pub(super) fn parse_union_declaration(&mut self) -> UnionDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = UnionDeclarationSyntax::builder(self.syntax_source(), start);

        self.parse_type_declaration_header(&mut builder, SyntaxKind::UnionKeyword);
        builder.push_union_body(self.parse_union_body());

        builder.build()
    }

    fn parse_type_declaration_header(
        &mut self,
        builder: &mut impl TypeDeclarationSyntaxSink,
        keyword_kind: SyntaxKind,
    ) {
        builder.push_type_directives(self.parse_type_directives());

        self.recover_until(builder, &TYPE_HEADER_START_KINDS);

        builder.push_type_modifiers(self.parse_type_modifiers());
        builder.push_declaration_keyword(self.expect(keyword_kind));
        builder.push_identifier_token(self.parse_identifier());

        if self.at(SyntaxKind::LessToken) {
            // TODO(parser): Parse generic parameter lists once generic syntax is implemented.
            self.recover_current_and_until_predicate(builder, Parser::at_type_after_name_boundary);
        }

        self.parse_type_constraints(builder);
    }

    fn parse_type_directives(&mut self) -> TypeDirectivesSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TypeDirectivesSyntax::builder(self.syntax_source(), start);

        while self.at(SyntaxKind::AtToken) {
            if self.at_directive_name(LAYOUT_DIRECTIVE_NAME) {
                builder.push_layout_directive(
                    self.parse_layout_directive(&TYPE_DIRECTIVE_ARGUMENT_RECOVERY_KINDS),
                );
                continue;
            }

            if self.at_directive_name(COPY_DIRECTIVE_NAME) {
                builder.push_copy_directive(self.parse_copy_directive());
                continue;
            }

            self.recover_unknown_type_directive(&mut builder);
        }

        builder.build()
    }

    fn recover_unknown_type_directive(&mut self, builder: &mut TypeDirectivesSyntaxBuilder) {
        self.recover_current_and_until(builder, &TYPE_DECLARATION_START_KINDS);
    }

    fn parse_type_modifiers(&mut self) -> TypeModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TypeModifiersSyntax::builder(self.syntax_source(), start);

        if self.at_visibility_modifier() {
            builder.push_visibility_token(self.parse_visibility_modifier());
        }

        builder.build()
    }

    fn parse_type_constraints(&mut self, builder: &mut impl TypeDeclarationSyntaxSink) {
        while self.at(SyntaxKind::WithKeyword) {
            // TODO(parser): Parse type constraint clauses once expressions are implemented.
            self.recover_current_and_until_predicate(builder, Parser::at_type_constraint_boundary);
        }
    }

    fn at_type_after_name_boundary(&mut self) -> bool {
        self.at_any(&TYPE_AFTER_NAME_BOUNDARY_KINDS) || self.at_any(&MODULE_ITEM_START_KINDS)
    }

    fn at_type_constraint_boundary(&mut self) -> bool {
        self.at_any(&TYPE_CONSTRAINT_BOUNDARY_KINDS) || self.at_any(&MODULE_ITEM_START_KINDS)
    }

    fn parse_struct_body(&mut self) -> StructBodySyntax {
        let start = self.peek().full_range().start();
        let mut builder = StructBodySyntax::builder(self.syntax_source(), start);

        // TODO(parser): Parse type body items once fields, variants, and members are implemented.
        self.parse_skipped_braced_body_tokens(&mut builder, Parser::at_type_body_missing_boundary);

        builder.build()
    }

    fn parse_union_body(&mut self) -> UnionBodySyntax {
        let start = self.peek().full_range().start();
        let mut builder = UnionBodySyntax::builder(self.syntax_source(), start);

        // TODO(parser): Parse type body items once fields, variants, and members are implemented.
        self.parse_skipped_braced_body_tokens(&mut builder, Parser::at_type_body_missing_boundary);

        builder.build()
    }

    fn at_type_body_missing_boundary(&mut self) -> bool {
        self.at_any(&TYPE_BODY_MISSING_BOUNDARY_KINDS) || self.at_any(&MODULE_ITEM_START_KINDS)
    }

    pub(super) fn should_parse_struct_declaration(&mut self) -> bool {
        self.should_parse_type_declaration(SyntaxKind::StructKeyword)
    }

    pub(super) fn should_parse_union_declaration(&mut self) -> bool {
        self.should_parse_type_declaration(SyntaxKind::UnionKeyword)
    }

    fn should_parse_type_declaration(&mut self, keyword_kind: SyntaxKind) -> bool {
        if !self.at_any(&TYPE_DECLARATION_START_KINDS) {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.consume_type_directives_for_scan();
            scan.consume_type_modifiers_for_scan();

            scan.at(keyword_kind)
        })
    }

    fn consume_type_directives_for_scan(&mut self) {
        while self.at(SyntaxKind::AtToken) {
            self.consume();

            if !self.at(SyntaxKind::IdentifierToken) {
                continue;
            }

            let name_token = self.consume();
            let directive_name = self.token_text(&name_token);

            if directive_name == Some(LAYOUT_DIRECTIVE_NAME) {
                self.consume_directive_argument_list_for_scan(&TYPE_DECLARATION_START_KINDS);
                continue;
            }

            if directive_name != Some(COPY_DIRECTIVE_NAME) {
                self.skip_unknown_directive_for_scan(&TYPE_DECLARATION_START_KINDS);
            }
        }
    }

    fn consume_type_modifiers_for_scan(&mut self) {
        if self.at_visibility_modifier() {
            self.consume();
        }
    }
}

trait TypeDeclarationSyntaxSink: RecoverySyntaxSink {
    fn push_type_directives(&mut self, directives: TypeDirectivesSyntax);

    fn push_type_modifiers(&mut self, modifiers: TypeModifiersSyntax);

    fn push_declaration_keyword(&mut self, token: SyntaxToken);

    fn push_identifier_token(&mut self, token: SyntaxToken);
}

impl TypeDeclarationSyntaxSink for StructDeclarationSyntaxBuilder {
    fn push_type_directives(&mut self, directives: TypeDirectivesSyntax) {
        StructDeclarationSyntaxBuilder::push_type_directives(self, directives);
    }

    fn push_type_modifiers(&mut self, modifiers: TypeModifiersSyntax) {
        StructDeclarationSyntaxBuilder::push_type_modifiers(self, modifiers);
    }

    fn push_declaration_keyword(&mut self, token: SyntaxToken) {
        StructDeclarationSyntaxBuilder::push_struct_keyword(self, token);
    }

    fn push_identifier_token(&mut self, token: SyntaxToken) {
        StructDeclarationSyntaxBuilder::push_identifier_token(self, token);
    }
}

impl TypeDeclarationSyntaxSink for UnionDeclarationSyntaxBuilder {
    fn push_type_directives(&mut self, directives: TypeDirectivesSyntax) {
        UnionDeclarationSyntaxBuilder::push_type_directives(self, directives);
    }

    fn push_type_modifiers(&mut self, modifiers: TypeModifiersSyntax) {
        UnionDeclarationSyntaxBuilder::push_type_modifiers(self, modifiers);
    }

    fn push_declaration_keyword(&mut self, token: SyntaxToken) {
        UnionDeclarationSyntaxBuilder::push_union_keyword(self, token);
    }

    fn push_identifier_token(&mut self, token: SyntaxToken) {
        UnionDeclarationSyntaxBuilder::push_identifier_token(self, token);
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::parser::parse_compilation_unit;
    use crate::test_support::{diagnostic_kinds, marker_offset, parse_diagnostic_kinds, source};

    use super::super::state::Parser;

    #[test]
    fn parser_parses_struct_declarations_after_source_unit_modules() {
        let source = "module main; @copy public struct Point {}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.struct_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one struct declaration: {declarations:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(declaration.full_text(), "@copy public struct Point {}");
        assert_eq!(declaration.type_directives().copy_directives().count(), 1);

        assert_eq!(
            declaration
                .type_modifiers()
                .visibility_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(declaration.struct_body().full_text(), "{}");
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_parses_union_declarations_inside_block_modules() {
        let source = "module main { internal union Maybe {} }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let modules = source_unit.block_module_declarations().collect::<Vec<_>>();

        let [module] = modules.as_slice() else {
            panic!("expected one block module declaration: {modules:?}");
        };

        let unions = module
            .module_body()
            .union_declarations()
            .collect::<Vec<_>>();

        let [declaration] = unions.as_slice() else {
            panic!("expected one union declaration: {unions:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(declaration.full_text(), "internal union Maybe {} ");

        assert_eq!(
            declaration
                .type_modifiers()
                .visibility_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::InternalKeyword)
        );

        assert_eq!(declaration.union_body().full_text(), "{} ");
        assert!(result.diagnostics().is_empty());
    }

    // TODO(parser): Update this when directive arguments are parsed.
    #[test]
    fn parser_parses_layout_type_directives() {
        let source = "module main; @layout(c) struct Point {}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.struct_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one struct declaration: {declarations:?}");
        };

        let directives = declaration.type_directives();
        let layouts = directives.layout_directives().collect::<Vec<_>>();

        let [layout] = layouts.as_slice() else {
            panic!("expected one layout directive: {layouts:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(directives.full_text(), "@layout(c) ");
        assert_eq!(layout.directive_argument_list().full_text(), "(c) ");

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [DiagnosticKind::SyntaxSkippedSyntax]
        );
    }

    // TODO(parser): Update this when type generics, constraints, and body items are parsed.
    #[test]
    fn parser_skips_type_generic_parameters_constraints_and_body_items_for_now() {
        let source = "module main; struct Box<T> with(T: Copy) { value: T; }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.struct_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one struct declaration: {declarations:?}");
        };

        let declaration_skipped = declaration.skipped_syntax().collect::<Vec<_>>();
        let body_skipped = declaration
            .struct_body()
            .skipped_syntax()
            .collect::<Vec<_>>();

        let [generics, constraint, field] = declaration_skipped.as_slice() else {
            panic!(
                "expected generic parameters, constraint, and field as skipped syntax: {declaration_skipped:?}"
            );
        };

        let [body_field] = body_skipped.as_slice() else {
            panic!("expected field as skipped body syntax: {body_skipped:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(generics.full_text(), "<T> ");
        assert_eq!(constraint.full_text(), "with(T: Copy) ");
        assert_eq!(field.full_text(), "value: T; ");
        assert_eq!(body_field.full_text(), "value: T; ");
        assert_eq!(declaration.struct_body().full_text(), "{ value: T; }");

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [
                DiagnosticKind::SyntaxSkippedSyntax,
                DiagnosticKind::SyntaxSkippedSyntax,
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );
    }

    #[test]
    fn parser_struct_body_missing_open_brace_does_not_consume_following_item() {
        let source = "module main; struct Point\nusing core;";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.struct_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one struct declaration: {declarations:?}");
        };

        let insertion = marker_offset(source, "using");
        let body = declaration.struct_body();

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(source_unit.using_declarations().count(), 1);

        assert!(body.open_brace_token().is_missing());
        assert!(body.close_brace_token().is_missing());

        assert_eq!(body.open_brace_token().range().start(), insertion);
        assert_eq!(body.close_brace_token().range().start(), insertion);

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [
                DiagnosticKind::SyntaxExpectedToken,
                DiagnosticKind::SyntaxExpectedToken
            ]
        );
    }

    #[test]
    fn parser_scan_ahead_recognizes_type_declarations_without_consuming_tokens() {
        let sources = source_store(["@copy public struct Point {}"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);

        assert!(parser.should_parse_struct_declaration());
        assert!(!parser.should_parse_union_declaration());
        assert_eq!(parser.peek().kind(), SyntaxKind::AtToken);

        let diagnostics = parser.finish();

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_scan_ahead_recognizes_union_declarations_without_consuming_tokens() {
        let sources = source_store(["@layout(c) union Maybe {}"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);

        assert!(parser.should_parse_union_declaration());
        assert_eq!(parser.peek().kind(), SyntaxKind::AtToken);

        let diagnostics = parser.finish();

        assert!(diagnostic_kinds(&diagnostics).is_empty());
    }
}
