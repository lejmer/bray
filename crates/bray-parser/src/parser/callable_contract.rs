use bray_syntax::{
    CallableContractDeclarationSyntax, CallableContractDeclarationSyntaxBuilder,
    CallableContractModifiersSyntax, SyntaxKind,
};

use super::module::MODULE_ITEM_START_KINDS;
use super::state::Parser;

const CALLABLE_CONTRACT_DECLARATION_START_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::CallableKeyword,
];

const CALLABLE_CONTRACT_AFTER_NAME_BOUNDARY_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::WithKeyword,
    SyntaxKind::EqualsToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const CALLABLE_CONTRACT_CONSTRAINT_BOUNDARY_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::WithKeyword,
    SyntaxKind::EqualsToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const CALLABLE_CONTRACT_TYPE_BOUNDARY_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

impl Parser {
    pub(super) fn parse_callable_contract_declaration(
        &mut self,
    ) -> CallableContractDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = CallableContractDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_callable_contract_modifiers(self.parse_callable_contract_modifiers());
        builder.push_callable_keyword(self.expect(SyntaxKind::CallableKeyword));
        builder.push_identifier_token(self.parse_identifier());

        if self.at(SyntaxKind::LessToken) {
            // TODO(parser): Parse callable contract generic parameter lists once generic syntax is implemented.
            self.recover_current_and_until_predicate(
                &mut builder,
                Parser::at_callable_contract_after_name_boundary,
            );
        }

        self.parse_callable_contract_constraints(&mut builder);
        builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));

        let mut at_type_boundary = Parser::at_callable_contract_type_recovery_boundary;

        builder.push_type_expression(self.parse_type_expression_until(&mut at_type_boundary));
        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));

        builder.build()
    }

    fn parse_callable_contract_modifiers(&mut self) -> CallableContractModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder = CallableContractModifiersSyntax::builder(self.syntax_source(), start);

        if self.at_visibility_modifier() {
            builder.push_visibility_token(self.parse_visibility_modifier());
        }

        builder.build()
    }

    fn parse_callable_contract_constraints(
        &mut self,
        builder: &mut CallableContractDeclarationSyntaxBuilder,
    ) {
        while self.at(SyntaxKind::WithKeyword) {
            // TODO(parser): Parse callable contract constraints once constraint syntax is implemented.
            self.recover_current_and_until_predicate(
                builder,
                Parser::at_callable_contract_constraint_boundary,
            );
        }
    }

    fn at_callable_contract_after_name_boundary(&mut self) -> bool {
        self.at_any(&CALLABLE_CONTRACT_AFTER_NAME_BOUNDARY_KINDS)
            || self.at_any(&MODULE_ITEM_START_KINDS)
    }

    fn at_callable_contract_constraint_boundary(&mut self) -> bool {
        self.at_any(&CALLABLE_CONTRACT_CONSTRAINT_BOUNDARY_KINDS)
            || self.at_any(&MODULE_ITEM_START_KINDS)
    }

    fn at_callable_contract_type_boundary(&mut self) -> bool {
        self.at_any(&CALLABLE_CONTRACT_TYPE_BOUNDARY_KINDS)
    }

    fn at_callable_contract_type_recovery_boundary(&mut self) -> bool {
        self.at_callable_contract_type_boundary() || self.at_any(&MODULE_ITEM_START_KINDS)
    }

    pub(super) fn should_parse_callable_contract_declaration(&mut self) -> bool {
        if !self.at_any(&CALLABLE_CONTRACT_DECLARATION_START_KINDS) {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.consume_callable_contract_modifiers_for_scan();

            scan.at(SyntaxKind::CallableKeyword)
        })
    }

    fn consume_callable_contract_modifiers_for_scan(&mut self) {
        if self.at_visibility_modifier() {
            self.consume();
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::parser::parse_compilation_unit;
    use crate::test_support::parse_diagnostic_kinds;

    #[test]
    fn parser_parses_callable_contract_declarations_after_source_unit_modules() {
        let source = "module main; public callable Mapper = func(value: Int) -> Bool;";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declarations = source_unit
            .callable_contract_declarations()
            .collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one callable contract declaration: {declarations:?}");
        };

        assert_eq!(source_unit.full_text(), source);

        assert_eq!(
            declaration.full_text(),
            "public callable Mapper = func(value: Int) -> Bool;"
        );

        assert_eq!(
            declaration
                .callable_contract_modifiers()
                .visibility_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(
            declaration.type_expression().full_text(),
            "func(value: Int) -> Bool"
        );
        assert_eq!(declaration.skipped_syntax().count(), 0);

        assert!(result.diagnostics().is_empty());
    }

    // TODO(parser): Update this when callable contract generics and constraints are parsed.
    #[test]
    fn parser_skips_callable_contract_generics_and_constraints_for_now() {
        let source = "module main; callable Mapper<T> with(T: Copy) = func(value: T) -> Bool;";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declarations = source_unit
            .callable_contract_declarations()
            .collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one callable contract declaration: {declarations:?}");
        };

        let skipped_syntax = declaration.skipped_syntax().collect::<Vec<_>>();

        let [generics, constraint] = skipped_syntax.as_slice() else {
            panic!(
                "expected generic parameters and constraint as skipped syntax: {skipped_syntax:?}"
            );
        };

        assert_eq!(source_unit.full_text(), source);

        assert_eq!(
            declaration.full_text(),
            "callable Mapper<T> with(T: Copy) = func(value: T) -> Bool;"
        );

        assert_eq!(generics.full_text(), "<T> ");
        assert_eq!(constraint.full_text(), "with(T: Copy) ");

        assert_eq!(
            declaration.type_expression().full_text(),
            "func(value: T) -> Bool"
        );

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [
                DiagnosticKind::SyntaxSkippedSyntax,
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );
    }

    #[test]
    fn parser_parses_callable_contract_declarations_inside_block_modules() {
        let source = "module main { callable Mapper = func(value: Int); }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let modules = source_unit.block_module_declarations().collect::<Vec<_>>();

        let [module] = modules.as_slice() else {
            panic!("expected one block module declaration: {modules:?}");
        };

        let declarations = module
            .module_body()
            .callable_contract_declarations()
            .collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one callable contract declaration: {declarations:?}");
        };

        assert_eq!(source_unit.full_text(), source);

        assert_eq!(
            declaration.full_text(),
            "callable Mapper = func(value: Int); "
        );

        assert_eq!(
            declaration.type_expression().full_text(),
            "func(value: Int)"
        );
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_reports_missing_callable_contract_semicolon_before_following_declaration_start() {
        let source = "module main; callable Mapper = func(value: Int)\nfunc main() {}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];

        let contracts = source_unit
            .callable_contract_declarations()
            .collect::<Vec<_>>();

        let functions = source_unit.function_declarations().collect::<Vec<_>>();

        let [contract] = contracts.as_slice() else {
            panic!("expected one callable contract declaration: {contracts:?}");
        };

        let [function] = functions.as_slice() else {
            panic!("expected one function declaration: {functions:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(contract.full_text(), "callable Mapper = func(value: Int)\n");
        assert_eq!(contract.type_expression().full_text(), "func(value: Int)\n");
        assert!(contract.semicolon_token().is_missing());
        assert_eq!(function.full_text(), "func main() {}");

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [DiagnosticKind::SyntaxExpectedToken]
        );
    }
}
