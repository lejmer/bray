use bray_syntax::{
    GenericParameterListSyntax, StructBodySyntax, StructDeclarationSyntax,
    StructDeclarationSyntaxBuilder, SyntaxKind, SyntaxToken, TypeDirectivesSyntax,
    TypeDirectivesSyntaxBuilder, TypeModifiersSyntax, UnionBodySyntax, UnionDeclarationSyntax,
    UnionDeclarationSyntaxBuilder,
};

use super::contract::{BRACED_DECLARATION_CONSTRAINT_BOUNDARY_KINDS, WithClauseSyntaxSink};
use super::directive::{COPY_DIRECTIVE_NAME, DirectiveScanKind, LAYOUT_DIRECTIVE_NAME};
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

const TYPE_DIRECTIVE_ARGUMENT_RECOVERY_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::AtToken,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::StructKeyword,
    SyntaxKind::UnionKeyword,
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

        if self.at(SyntaxKind::SemicolonToken) {
            builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));
        } else {
            builder.push_struct_body(self.parse_struct_body());
        }

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
        builder.push_type_modifiers(self.parse_type_modifiers());
        builder.push_declaration_keyword(self.expect(keyword_kind));
        builder.push_identifier_token(self.parse_identifier());

        if self.at(SyntaxKind::LessToken) {
            builder.push_generic_parameter_list(self.parse_generic_parameter_list());
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
        self.parse_with_clauses(builder, Parser::at_type_constraint_boundary);
    }

    fn at_type_constraint_boundary(&mut self) -> bool {
        self.at_any(&BRACED_DECLARATION_CONSTRAINT_BOUNDARY_KINDS)
            || self.at_any(&MODULE_ITEM_START_KINDS)
    }

    fn parse_struct_body(&mut self) -> StructBodySyntax {
        let start = self.peek().full_range().start();
        let mut builder = StructBodySyntax::builder(self.syntax_source(), start);

        self.parse_braced_body_contents(
            &mut builder,
            Parser::at_type_body_missing_boundary,
            Parser::parse_struct_body_items,
        );

        builder.build()
    }

    fn parse_union_body(&mut self) -> UnionBodySyntax {
        let start = self.peek().full_range().start();
        let mut builder = UnionBodySyntax::builder(self.syntax_source(), start);

        self.parse_braced_body_contents(
            &mut builder,
            Parser::at_type_body_missing_boundary,
            Parser::parse_union_body_items,
        );

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
        self.consume_directives_for_scan(&MODULE_ITEM_START_KINDS, |directive_name| {
            match directive_name {
                LAYOUT_DIRECTIVE_NAME => DirectiveScanKind::ArgumentList,
                COPY_DIRECTIVE_NAME => DirectiveScanKind::Bare,
                _ => DirectiveScanKind::Unknown,
            }
        });
    }

    fn consume_type_modifiers_for_scan(&mut self) {
        if self.at_visibility_modifier() {
            self.consume();
        }
    }
}

trait TypeDeclarationSyntaxSink: RecoverySyntaxSink + WithClauseSyntaxSink {
    fn push_type_directives(&mut self, directives: TypeDirectivesSyntax);

    fn push_type_modifiers(&mut self, modifiers: TypeModifiersSyntax);

    fn push_declaration_keyword(&mut self, token: SyntaxToken);

    fn push_identifier_token(&mut self, token: SyntaxToken);

    fn push_generic_parameter_list(&mut self, list: GenericParameterListSyntax);
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

    fn push_generic_parameter_list(&mut self, list: GenericParameterListSyntax) {
        StructDeclarationSyntaxBuilder::push_generic_parameter_list(self, list);
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

    fn push_generic_parameter_list(&mut self, list: GenericParameterListSyntax) {
        UnionDeclarationSyntaxBuilder::push_generic_parameter_list(self, list);
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

        assert_eq!(
            declaration
                .struct_body()
                .unwrap_or_else(|| panic!("test struct must have a body"))
                .full_text(),
            "{}"
        );

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
            layout
                .directive_argument_list()
                .directive_arguments()
                .count(),
            1
        );

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_parses_type_generics_and_constraints() {
        let source = "module main; struct Box<T> with(copyable) { value: T; }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.struct_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one struct declaration: {declarations:?}");
        };

        let body = declaration
            .struct_body()
            .unwrap_or_else(|| panic!("test struct must have a body"));

        let fields = body.struct_field_declarations().collect::<Vec<_>>();

        let [field] = fields.as_slice() else {
            panic!("expected one struct field declaration: {fields:?}");
        };

        assert_eq!(source_unit.full_text(), source);

        let generic_parameter_list = match declaration.generic_parameter_list() {
            Some(list) => list,
            None => panic!("expected generic parameter list"),
        };

        assert_eq!(generic_parameter_list.full_text(), "<T> ");
        assert_eq!(generic_parameter_list.generic_type_parameters().count(), 1);
        assert_eq!(declaration.with_clauses().count(), 1);
        assert_eq!(declaration.skipped_syntax().count(), 0);
        assert_eq!(field.type_expression().full_text(), "T");
        assert_eq!(field.full_text(), "value: T; ");
        assert_eq!(field.identifier_token().kind(), SyntaxKind::IdentifierToken);

        assert_eq!(
            declaration
                .struct_body()
                .unwrap_or_else(|| panic!("test struct must have a body"))
                .full_text(),
            "{ value: T; }"
        );

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_parses_union_variants_with_payload_fields() {
        let source = "module main; union Maybe { @tag(1) Some(pos value: Int = fallback,); None; }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.union_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one union declaration: {declarations:?}");
        };

        let body = declaration.union_body();
        let variants = body.union_variant_declarations().collect::<Vec<_>>();

        let [some, none] = variants.as_slice() else {
            panic!("expected two union variants: {variants:?}");
        };

        let some_payload = match some.union_variant_payload() {
            Some(payload) => payload,
            None => panic!("expected payload on Some variant"),
        };

        let fields = some_payload.union_payload_fields().collect::<Vec<_>>();

        let [field] = fields.as_slice() else {
            panic!("expected one payload field: {fields:?}");
        };

        assert_eq!(source_unit.full_text(), source);

        assert_eq!(
            declaration.full_text(),
            "union Maybe { @tag(1) Some(pos value: Int = fallback,); None; }"
        );

        assert_eq!(some.variant_directives().tag_directives().count(), 1);

        let tag = match some.variant_directives().tag_directives().next() {
            Some(tag) => tag,
            None => panic!("expected tag directive"),
        };

        assert_eq!(
            tag.directive_argument_list().directive_arguments().count(),
            1
        );

        assert_eq!(some.identifier_token().text(source), Some("Some"));
        assert_eq!(some_payload.separator_tokens().count(), 1);

        assert_eq!(
            field
                .payload_field_modifiers()
                .pos_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::PosKeyword)
        );

        assert!(field.equals_token().is_some());
        assert_eq!(field.type_expression().full_text(), "Int ");

        assert_eq!(
            field.expression().map(|expression| expression.full_text()),
            Some(String::from("fallback"))
        );

        assert_eq!(none.full_text(), "None; ");

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_parses_type_callable_members_in_struct_bodies() {
        let source = "module main; struct Point { public static func make<T>() -> Point {} }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.struct_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one struct declaration: {declarations:?}");
        };

        let members = declaration
            .struct_body()
            .unwrap_or_else(|| panic!("test struct must have a body"))
            .type_callable_member_declarations()
            .collect::<Vec<_>>();

        let [member] = members.as_slice() else {
            panic!("expected one callable member: {members:?}");
        };

        assert_eq!(source_unit.full_text(), source);

        assert_eq!(
            member.full_text(),
            "public static func make<T>() -> Point {} "
        );

        assert_eq!(
            member
                .type_callable_member_modifiers()
                .visibility_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(
            member
                .type_callable_member_modifiers()
                .static_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::StaticKeyword)
        );

        assert!(member.callable_result_clause().is_some());
        assert!(member.callable_body_block_expression().is_some());

        let generic_parameter_list = match member.generic_parameter_list() {
            Some(list) => list,
            None => panic!("expected generic parameter list"),
        };

        assert_eq!(generic_parameter_list.full_text(), "<T>");
        assert_eq!(generic_parameter_list.generic_type_parameters().count(), 1);

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_parses_constant_declarations_in_type_bodies() {
        let source = concat!(
            "module main; ",
            "struct Config { public const Max: Int = 10; } ",
            "union Maybe { const Tag: Int = 1; Some; }"
        );

        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let structs = source_unit.struct_declarations().collect::<Vec<_>>();
        let unions = source_unit.union_declarations().collect::<Vec<_>>();

        let [struct_declaration] = structs.as_slice() else {
            panic!("expected one struct declaration: {structs:?}");
        };

        let [union_declaration] = unions.as_slice() else {
            panic!("expected one union declaration: {unions:?}");
        };

        let struct_constants = struct_declaration
            .struct_body()
            .unwrap_or_else(|| panic!("test struct must have a body"))
            .constant_declarations()
            .collect::<Vec<_>>();

        let union_body = union_declaration.union_body();
        let union_constants = union_body.constant_declarations().collect::<Vec<_>>();
        let variants = union_body.union_variant_declarations().collect::<Vec<_>>();

        let [struct_constant] = struct_constants.as_slice() else {
            panic!("expected one struct constant declaration: {struct_constants:?}");
        };

        let [union_constant] = union_constants.as_slice() else {
            panic!("expected one union constant declaration: {union_constants:?}");
        };

        let [variant] = variants.as_slice() else {
            panic!("expected one union variant declaration: {variants:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(struct_constant.full_text(), "public const Max: Int = 10; ");

        assert_eq!(
            struct_constant
                .constant_modifiers()
                .visibility_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(union_constant.full_text(), "const Tag: Int = 1; ");

        assert_eq!(
            struct_constant
                .expression()
                .map(|expression| expression.full_text()),
            Some(String::from("10"))
        );

        assert_eq!(
            union_constant
                .expression()
                .map(|expression| expression.full_text()),
            Some(String::from("1"))
        );

        assert_eq!(variant.full_text(), "Some; ");

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_preserves_bodyless_and_flexible_struct_forms() {
        let source = concat!(
            "module main; ",
            "struct FILE; ",
            "@layout(c) struct Packet { length: usize; bytes: [u8; ..]; }"
        );

        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let declarations = result.syntax_tree().root().source_units()[0]
            .struct_declarations()
            .collect::<Vec<_>>();

        let [incomplete, flexible] = declarations.as_slice() else {
            panic!("expected two struct declarations: {declarations:?}");
        };

        assert!(incomplete.semicolon_token().is_some());
        assert!(incomplete.struct_body().is_none());

        let body = flexible
            .struct_body()
            .unwrap_or_else(|| panic!("flexible struct must have a body"));

        let fields = body.struct_field_declarations().collect::<Vec<_>>();

        let [_, trailing] = fields.as_slice() else {
            panic!("expected two fields: {fields:?}");
        };

        assert!(trailing.type_expression().dot_dot_token().is_some());
        assert!(result.diagnostics().is_empty());
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

        let body = declaration
            .struct_body()
            .unwrap_or_else(|| panic!("test struct must have a body"));

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
