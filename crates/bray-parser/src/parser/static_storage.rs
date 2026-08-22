use bray_syntax::{
    StaticDeclarationModifiersSyntax, StaticDeclarationSyntax, StaticDirectivesSyntax, SyntaxKind,
};

use super::directive::{
    DirectiveScanKind, LINK_DIRECTIVE_NAME, SYMBOL_DIRECTIVE_NAME, THREAD_LOCAL_DIRECTIVE_NAME,
};
use super::module::MODULE_ITEM_START_KINDS;
use super::state::Parser;

const STATIC_DECLARATION_START_KINDS: [SyntaxKind; 6] = [
    SyntaxKind::AtToken,
    SyntaxKind::ExternKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::StaticKeyword,
];

const STATIC_TYPE_BOUNDARY_KINDS: [SyntaxKind; 4] = [
    SyntaxKind::WithKeyword,
    SyntaxKind::EqualsToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::EndOfFileToken,
];

const STATIC_INITIALIZER_BOUNDARY_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

impl Parser {
    pub(super) fn parse_static_declaration(&mut self) -> StaticDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = StaticDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_static_directives(self.parse_static_directives());
        builder.push_static_declaration_modifiers(self.parse_static_declaration_modifiers());
        builder.push_static_keyword(self.expect(SyntaxKind::StaticKeyword));

        if self.at(SyntaxKind::MutKeyword) {
            builder.push_mut_token(self.expect(SyntaxKind::MutKeyword));
        }

        builder.push_identifier_token(self.parse_identifier());

        if self.at(SyntaxKind::LessToken) {
            builder.push_generic_parameter_list(self.parse_generic_parameter_list());
        }

        builder.push_colon_token(self.expect(SyntaxKind::ColonToken));

        let mut at_type_boundary = Parser::at_static_type_boundary;

        builder.push_type_expression(self.parse_type_expression_until(&mut at_type_boundary));
        self.parse_with_clauses(&mut builder, Parser::at_static_constraint_boundary);

        if self.at(SyntaxKind::EqualsToken) {
            builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));

            if !self.at_static_initializer_boundary() {
                let mut at_initializer_boundary = Parser::at_static_initializer_boundary;

                builder.push_expression(
                    self.parse_non_assignment_expression_until(&mut at_initializer_boundary),
                );
            }
        }

        self.recover_until_predicate(&mut builder, Parser::at_static_declaration_end);
        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));

        builder.build()
    }

    fn parse_static_directives(&mut self) -> StaticDirectivesSyntax {
        let start = self.peek().full_range().start();
        let mut builder = StaticDirectivesSyntax::builder(self.syntax_source(), start);

        while self.at(SyntaxKind::AtToken) {
            if self.at_directive_name(THREAD_LOCAL_DIRECTIVE_NAME) {
                builder.push_thread_local_directive(self.parse_thread_local_directive());
                continue;
            }

            if self.at_directive_name(LINK_DIRECTIVE_NAME) {
                builder.push_link_directive(
                    self.parse_link_directive(&STATIC_DECLARATION_START_KINDS),
                );

                continue;
            }

            if self.at_directive_name(SYMBOL_DIRECTIVE_NAME) {
                builder.push_symbol_directive(
                    self.parse_symbol_directive(&STATIC_DECLARATION_START_KINDS),
                );

                continue;
            }

            self.recover_current_and_until(&mut builder, &STATIC_DECLARATION_START_KINDS);
        }

        builder.build()
    }

    fn parse_static_declaration_modifiers(&mut self) -> StaticDeclarationModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder = StaticDeclarationModifiersSyntax::builder(self.syntax_source(), start);

        while self.at_static_declaration_modifier() {
            if self.at(SyntaxKind::ExternKeyword) {
                builder.push_extern_token(self.expect(SyntaxKind::ExternKeyword));
                continue;
            }

            if self.at(SyntaxKind::TrustedKeyword) {
                builder.push_trusted_token(self.expect(SyntaxKind::TrustedKeyword));
                continue;
            }

            builder.push_visibility_token(self.parse_visibility_modifier());
        }

        builder.build()
    }

    fn at_static_declaration_modifier(&mut self) -> bool {
        self.at(SyntaxKind::ExternKeyword)
            || self.at(SyntaxKind::TrustedKeyword)
            || self.at_visibility_modifier()
    }

    fn at_static_type_boundary(&mut self) -> bool {
        self.at_any(&STATIC_TYPE_BOUNDARY_KINDS) || self.at_static_following_item_start()
    }

    fn at_static_constraint_boundary(&mut self) -> bool {
        self.at(SyntaxKind::EqualsToken) || self.at_static_declaration_end()
    }

    fn at_static_initializer_boundary(&mut self) -> bool {
        self.at_any(&STATIC_INITIALIZER_BOUNDARY_KINDS) || self.at_static_following_item_start()
    }

    fn at_static_declaration_end(&mut self) -> bool {
        self.at_any(&STATIC_INITIALIZER_BOUNDARY_KINDS) || self.at_static_following_item_start()
    }

    fn at_static_following_item_start(&mut self) -> bool {
        self.at_any(&MODULE_ITEM_START_KINDS)
    }

    pub(super) fn should_parse_static_declaration(&mut self) -> bool {
        if !self.at_any(&STATIC_DECLARATION_START_KINDS) {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.consume_directives_for_scan(&MODULE_ITEM_START_KINDS, |name| match name {
                THREAD_LOCAL_DIRECTIVE_NAME => DirectiveScanKind::Bare,
                LINK_DIRECTIVE_NAME | SYMBOL_DIRECTIVE_NAME => DirectiveScanKind::ArgumentList,
                _ => DirectiveScanKind::Unknown,
            });

            while scan.at_static_declaration_modifier() {
                scan.consume();
            }

            scan.at(SyntaxKind::StaticKeyword)
        })
    }
}

#[cfg(test)]
mod tests {
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store;

    use crate::parser::parse_compilation_unit;

    #[test]
    fn parser_parses_static_declaration_surface() {
        let source =
            "module app; @thread_local internal static Cache<T>: T with(T: Copy) = default<T>();";

        let sources = test_source_store([source]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.static_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one static declaration: {declarations:?}");
        };

        assert_eq!(
            declaration.full_text(),
            source.strip_prefix("module app; ").unwrap()
        );

        assert_eq!(
            declaration
                .static_declaration_modifiers()
                .visibility_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::InternalKeyword)
        );

        assert_eq!(
            declaration
                .static_directives()
                .thread_local_directives()
                .count(),
            1
        );

        assert!(declaration.generic_parameter_list().is_some());
        assert_eq!(declaration.with_clauses().count(), 1);

        assert_eq!(
            declaration
                .expression()
                .map(|expression| expression.full_text()),
            Some("default<T>()".into())
        );

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_preserves_native_data_symbol_declarations() {
        let source = concat!(
            "module app; ",
            "@link(name = \"c\") ",
            "@symbol(name = \"errno\", presence = optional) ",
            "@thread_local extern trusted static mut errno: i32;"
        );

        let sources = test_source_store([source]);
        let result = parse_compilation_unit(&sources);

        let declarations = result.syntax_tree().root().source_units()[0]
            .static_declarations()
            .collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one static declaration: {declarations:?}");
        };

        let modifiers = declaration.static_declaration_modifiers();

        assert!(modifiers.extern_token().is_some());
        assert!(modifiers.trusted_token().is_some());
        assert!(declaration.mut_token().is_some());
        assert!(declaration.equals_token().is_none());
        assert!(declaration.expression().is_none());

        assert_eq!(declaration.static_directives().link_directives().count(), 1);
        assert_eq!(declaration.static_directives().symbol_directives().count(), 1);

        assert_eq!(
            declaration
                .static_directives()
                .thread_local_directives()
                .count(),
            1
        );

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_preserves_a_following_static_after_a_missing_initializer() {
        let source = "module app; static First: u32 = static Second: u32 = 2;";
        let sources = test_source_store([source]);
        let result = parse_compilation_unit(&sources);

        let declarations = result.syntax_tree().root().source_units()[0]
            .static_declarations()
            .collect::<Vec<_>>();

        assert_eq!(declarations.len(), 2);
        assert!(declarations[0].expression().is_none());
        assert_eq!(declarations[1].full_text(), "static Second: u32 = 2;");
        assert!(!result.diagnostics().is_empty());
    }

    #[test]
    fn function_scanning_preserves_a_thread_local_static() {
        let source = "module app; @thread_local static THREAD_VALUE: i32 = 42; func read() {}";
        let sources = test_source_store([source]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];

        assert_eq!(source_unit.static_declarations().count(), 1);
        assert_eq!(source_unit.function_declarations().count(), 1);
        assert!(result.diagnostics().is_empty());
    }
}
