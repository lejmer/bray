use bray_syntax::{
    FunctionDeclarationSyntax, FunctionDeclarationSyntaxBuilder, FunctionDirectivesSyntax,
    FunctionDirectivesSyntaxBuilder, FunctionModifiersSyntax, SyntaxKind,
};

use super::directive::{
    ABI_DIRECTIVE_NAME, ENTRYPOINT_DIRECTIVE_NAME, LINK_DIRECTIVE_NAME, SYMBOL_DIRECTIVE_NAME,
    TEST_DIRECTIVE_NAME,
};
use super::module::MODULE_ITEM_START_KINDS;
use super::state::Parser;

const FUNCTION_DECLARATION_START_KINDS: [SyntaxKind; 8] = [
    SyntaxKind::AtToken,
    SyntaxKind::ExternKeyword,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::ConstKeyword,
    SyntaxKind::FuncKeyword,
];

const FUNCTION_HEADER_START_KINDS: [SyntaxKind; 7] = [
    SyntaxKind::ExternKeyword,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::ConstKeyword,
    SyntaxKind::FuncKeyword,
];

const FUNCTION_DIRECTIVE_ARGUMENT_RECOVERY_KINDS: [SyntaxKind; 8] = [
    SyntaxKind::AtToken,
    SyntaxKind::ExternKeyword,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::ConstKeyword,
    SyntaxKind::FuncKeyword,
];

const FUNCTION_AFTER_NAME_RECOVERY_KINDS: [SyntaxKind; 8] = [
    SyntaxKind::OpenParenToken,
    SyntaxKind::ArrowToken,
    SyntaxKind::RequiresKeyword,
    SyntaxKind::EnsuresKeyword,
    SyntaxKind::WithKeyword,
    SyntaxKind::UsesKeyword,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::SemicolonToken,
];

const CALLABLE_CONTRACT_CLAUSE_START_KINDS: [SyntaxKind; 4] = [
    SyntaxKind::RequiresKeyword,
    SyntaxKind::EnsuresKeyword,
    SyntaxKind::WithKeyword,
    SyntaxKind::UsesKeyword,
];

const CALLABLE_CONTRACT_BOUNDARY_KINDS: [SyntaxKind; 8] = [
    SyntaxKind::RequiresKeyword,
    SyntaxKind::EnsuresKeyword,
    SyntaxKind::WithKeyword,
    SyntaxKind::UsesKeyword,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

impl Parser {
    pub(super) fn parse_function_declaration(&mut self) -> FunctionDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = FunctionDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_function_directives(self.parse_function_directives());
        self.recover_until(&mut builder, &FUNCTION_HEADER_START_KINDS);

        builder.push_function_modifiers(self.parse_function_modifiers());
        builder.push_func_keyword(self.expect(SyntaxKind::FuncKeyword));
        builder.push_identifier_token(self.parse_identifier());

        if self.at(SyntaxKind::LessToken) {
            // TODO(parser): Parse generic parameter lists once generic syntax is implemented.
            self.recover_current_and_until(&mut builder, &FUNCTION_AFTER_NAME_RECOVERY_KINDS);
        }

        builder.push_parameter_list(self.parse_parameter_list());

        if self.at(SyntaxKind::ArrowToken) {
            builder.push_callable_result_clause(self.parse_callable_result_clause());
        }

        self.parse_callable_contract_clauses(&mut builder);
        self.parse_function_declaration_tail(&mut builder);

        builder.build()
    }

    fn parse_function_directives(&mut self) -> FunctionDirectivesSyntax {
        let start = self.peek().full_range().start();
        let mut builder = FunctionDirectivesSyntax::builder(self.syntax_source(), start);

        while self.at(SyntaxKind::AtToken) {
            if self.at_directive_name(ABI_DIRECTIVE_NAME) {
                builder.push_abi_directive(
                    self.parse_abi_directive(&FUNCTION_DIRECTIVE_ARGUMENT_RECOVERY_KINDS),
                );
                continue;
            }

            if self.at_directive_name(LINK_DIRECTIVE_NAME) {
                builder.push_link_directive(
                    self.parse_link_directive(&FUNCTION_DIRECTIVE_ARGUMENT_RECOVERY_KINDS),
                );
                continue;
            }

            if self.at_directive_name(SYMBOL_DIRECTIVE_NAME) {
                builder.push_symbol_directive(
                    self.parse_symbol_directive(&FUNCTION_DIRECTIVE_ARGUMENT_RECOVERY_KINDS),
                );
                continue;
            }

            if self.at_directive_name(ENTRYPOINT_DIRECTIVE_NAME) {
                builder.push_entrypoint_directive(self.parse_entrypoint_directive());
                continue;
            }

            if self.at_directive_name(TEST_DIRECTIVE_NAME) {
                builder.push_test_directive(self.parse_test_directive());
                continue;
            }

            self.recover_unknown_function_directive(&mut builder);
        }

        builder.build()
    }

    fn recover_unknown_function_directive(
        &mut self,
        builder: &mut FunctionDirectivesSyntaxBuilder,
    ) {
        self.recover_current_and_until(builder, &FUNCTION_DECLARATION_START_KINDS);
    }

    fn parse_function_modifiers(&mut self) -> FunctionModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder = FunctionModifiersSyntax::builder(self.syntax_source(), start);

        while self.at_function_modifier() {
            if self.at(SyntaxKind::ExternKeyword) {
                builder.push_extern_token(self.expect(SyntaxKind::ExternKeyword));
                continue;
            }

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

            builder.push_const_token(self.expect(SyntaxKind::ConstKeyword));
        }

        builder.build()
    }

    fn at_function_modifier(&mut self) -> bool {
        self.at(SyntaxKind::ExternKeyword)
            || self.at_visibility_modifier()
            || self.at(SyntaxKind::AsyncKeyword)
            || self.at(SyntaxKind::TrustedKeyword)
            || self.at(SyntaxKind::ConstKeyword)
    }

    fn parse_callable_contract_clauses(&mut self, builder: &mut FunctionDeclarationSyntaxBuilder) {
        while self.at_any(&CALLABLE_CONTRACT_CLAUSE_START_KINDS) {
            // TODO(parser): Parse callable contract clauses once expressions are implemented.
            self.recover_current_and_until_predicate(
                builder,
                Parser::at_callable_contract_boundary,
            );
        }
    }

    fn at_callable_contract_boundary(&mut self) -> bool {
        self.at_any(&CALLABLE_CONTRACT_BOUNDARY_KINDS) || self.at_any(&MODULE_ITEM_START_KINDS)
    }

    fn parse_function_declaration_tail(&mut self, builder: &mut FunctionDeclarationSyntaxBuilder) {
        if self.at(SyntaxKind::OpenBraceToken) {
            builder
                .push_callable_body_block_expression(self.parse_callable_body_block_expression());
            return;
        }

        self.recover_until_module_item_declaration_end(builder);
        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));
    }

    pub(super) fn should_parse_function_declaration(&mut self) -> bool {
        if !self.at_any(&FUNCTION_DECLARATION_START_KINDS) {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.consume_function_directives_for_scan();
            scan.consume_function_modifiers_for_scan();

            scan.at(SyntaxKind::FuncKeyword)
        })
    }

    fn consume_function_directives_for_scan(&mut self) {
        while self.at(SyntaxKind::AtToken) {
            self.consume();

            if !self.at(SyntaxKind::IdentifierToken) {
                continue;
            }

            let name_token = self.consume();
            let directive_name = self.token_text(&name_token);

            if directive_name == Some(ABI_DIRECTIVE_NAME)
                || directive_name == Some(LINK_DIRECTIVE_NAME)
                || directive_name == Some(SYMBOL_DIRECTIVE_NAME)
            {
                self.consume_directive_argument_list_for_scan(&FUNCTION_DECLARATION_START_KINDS);
                continue;
            }

            if directive_name != Some(ENTRYPOINT_DIRECTIVE_NAME)
                && directive_name != Some(TEST_DIRECTIVE_NAME)
            {
                self.skip_unknown_function_directive_for_scan();
            }
        }
    }

    fn skip_unknown_function_directive_for_scan(&mut self) {
        if self.consume_if(SyntaxKind::OpenParenToken).is_some() {
            self.scan_until_balanced_close_paren(&FUNCTION_DECLARATION_START_KINDS);
            self.consume_if(SyntaxKind::CloseParenToken);

            return;
        }

        while !self.at_any(&FUNCTION_DECLARATION_START_KINDS)
            && !self.at(SyntaxKind::EndOfFileToken)
        {
            self.consume();
        }
    }

    fn consume_function_modifiers_for_scan(&mut self) {
        while self.at_function_modifier() {
            self.consume();
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_source::{TextRange, TextSize};
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::parser::parse_compilation_unit;
    use crate::test_support::{assert_missing_semicolon_diagnostic, parse_diagnostic_kinds};

    #[test]
    fn parser_parses_function_declarations_after_source_unit_modules() {
        let source = concat!(
            "module main; ",
            "@test @abi(\"C\") public async func main(pos value: Int = 1, mut tail: Bool,) ",
            "-> Unit {}"
        );

        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.function_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one function declaration: {declarations:?}");
        };

        let directives = declaration.function_directives();
        let modifiers = declaration.function_modifiers();
        let parameter_list = declaration.parameter_list();
        let parameters = parameter_list.parameters().collect::<Vec<_>>();

        let [first, second] = parameters.as_slice() else {
            panic!("expected two function parameters: {parameters:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(
            declaration.full_text(),
            "@test @abi(\"C\") public async func main(pos value: Int = 1, mut tail: Bool,) -> Unit {}"
        );

        assert_eq!(directives.test_directives().count(), 1);
        assert_eq!(directives.abi_directives().count(), 1);

        assert_eq!(
            modifiers.visibility_token().map(|token| token.kind()),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(
            modifiers.async_token().map(|token| token.kind()),
            Some(SyntaxKind::AsyncKeyword)
        );

        assert_eq!(parameter_list.separator_tokens().count(), 2);
        assert_eq!(
            first
                .parameter_modifiers()
                .pos_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::PosKeyword)
        );
        assert!(first.equals_token().is_some());
        assert_eq!(
            second
                .parameter_modifiers()
                .mut_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::MutKeyword)
        );
        assert!(declaration.callable_result_clause().is_some());
        assert!(declaration.callable_body_block_expression().is_some());

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [
                DiagnosticKind::SyntaxSkippedSyntax,
                DiagnosticKind::SyntaxSkippedSyntax,
                DiagnosticKind::SyntaxSkippedSyntax,
                DiagnosticKind::SyntaxSkippedSyntax,
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );
    }

    #[test]
    fn parser_represents_missing_parameter_separators_in_function_declarations() {
        let source = "module main; func main(first: Int mut second: Bool) {}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.function_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one function declaration: {declarations:?}");
        };

        let separators = declaration
            .parameter_list()
            .separator_tokens()
            .collect::<Vec<_>>();

        let [separator] = separators.as_slice() else {
            panic!("expected one separator token: {separators:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(declaration.parameter_list().parameters().count(), 2);
        assert!(separator.is_missing());
        assert_eq!(separator.kind(), SyntaxKind::CommaToken);

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [
                DiagnosticKind::SyntaxSkippedSyntax,
                DiagnosticKind::SyntaxExpectedToken,
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );
    }

    #[test]
    fn parser_reports_missing_function_semicolon_before_following_item() {
        let source = "module main; extern func main()\nusing core;";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.function_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one function declaration: {declarations:?}");
        };

        let insertion = TextSize::new(
            source
                .find("using")
                .expect("test source should contain using keyword")
                .try_into()
                .expect("test source offset should fit in TextSize"),
        );

        let semicolon_token = match declaration.semicolon_token() {
            Some(token) => token,
            None => panic!("expected function semicolon token"),
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(source_unit.using_declarations().count(), 1);
        assert!(semicolon_token.is_missing());
        assert_eq!(semicolon_token.range(), TextRange::empty(insertion));

        assert_missing_semicolon_diagnostic(
            &result,
            insertion,
            SyntaxKind::UsingKeyword,
            "using",
            &[DiagnosticKind::SyntaxExpectedToken],
        );
    }

    #[test]
    fn parser_recovers_function_bodies_without_losing_later_items() {
        let source = "module main; func main() { return; }\nusing core;";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.function_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one function declaration: {declarations:?}");
        };

        let body = match declaration.callable_body_block_expression() {
            Some(body) => body,
            None => panic!("expected function body"),
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(body.full_text(), "{ return; }\n");
        assert_eq!(body.skipped_syntax().count(), 1);
        assert_eq!(source_unit.using_declarations().count(), 1);

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [DiagnosticKind::SyntaxSkippedSyntax]
        );
    }
}
