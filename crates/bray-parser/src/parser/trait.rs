use bray_syntax::{
    SyntaxKind, TraitBodySyntax, TraitDeclarationSyntax, TraitDeclarationSyntaxBuilder,
    TraitModifiersSyntax,
};

use super::module::MODULE_ITEM_START_KINDS;
use super::state::Parser;

const TRAIT_DECLARATION_START_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::TraitKeyword,
];

const TRAIT_CONSTRAINT_BOUNDARY_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::WithKeyword,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const TRAIT_BODY_MISSING_BOUNDARY_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

impl Parser {
    pub(super) fn parse_trait_declaration(&mut self) -> TraitDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TraitDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_trait_modifiers(self.parse_trait_modifiers());
        builder.push_trait_keyword(self.expect(SyntaxKind::TraitKeyword));
        builder.push_identifier_token(self.parse_identifier());

        if self.at(SyntaxKind::LessToken) {
            builder.push_generic_parameter_list(self.parse_generic_parameter_list());
        }

        self.parse_trait_constraints(&mut builder);
        builder.push_trait_body(self.parse_trait_body());

        builder.build()
    }

    fn parse_trait_modifiers(&mut self) -> TraitModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TraitModifiersSyntax::builder(self.syntax_source(), start);

        if self.at_visibility_modifier() {
            builder.push_visibility_token(self.parse_visibility_modifier());
        }

        builder.build()
    }

    fn parse_trait_constraints(&mut self, builder: &mut TraitDeclarationSyntaxBuilder) {
        while self.at(SyntaxKind::WithKeyword) {
            // TODO(parser): Parse trait constraint clauses once constraint syntax is implemented.
            self.recover_current_and_until_predicate(builder, Parser::at_trait_constraint_boundary);
        }
    }

    fn at_trait_constraint_boundary(&mut self) -> bool {
        self.at_any(&TRAIT_CONSTRAINT_BOUNDARY_KINDS) || self.at_any(&MODULE_ITEM_START_KINDS)
    }

    fn parse_trait_body(&mut self) -> TraitBodySyntax {
        let start = self.peek().full_range().start();
        let mut builder = TraitBodySyntax::builder(self.syntax_source(), start);

        self.parse_braced_body_contents(
            &mut builder,
            Parser::at_trait_body_missing_boundary,
            Parser::parse_trait_body_items,
        );

        builder.build()
    }

    fn at_trait_body_missing_boundary(&mut self) -> bool {
        self.at_any(&TRAIT_BODY_MISSING_BOUNDARY_KINDS) || self.at_any(&MODULE_ITEM_START_KINDS)
    }

    pub(super) fn should_parse_trait_declaration(&mut self) -> bool {
        if !self.at_any(&TRAIT_DECLARATION_START_KINDS) {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.consume_trait_modifiers_for_scan();

            scan.at(SyntaxKind::TraitKeyword)
        })
    }

    fn consume_trait_modifiers_for_scan(&mut self) {
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
    use crate::test_support::{marker_offset, parse_diagnostic_kinds, source};

    use super::super::state::Parser;

    #[test]
    fn parser_parses_trait_declarations_after_source_unit_modules() {
        let source = "module main; public trait Display {}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.trait_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one trait declaration: {declarations:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(declaration.full_text(), "public trait Display {}");

        assert_eq!(
            declaration
                .trait_modifiers()
                .visibility_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(
            declaration.identifier_token().kind(),
            SyntaxKind::IdentifierToken
        );

        assert_eq!(declaration.trait_body().full_text(), "{}");
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_parses_trait_declarations_inside_block_modules() {
        let source = "module main { trait Display {} }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.block_module_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one block module declaration: {declarations:?}");
        };

        let body = declaration.module_body();
        let traits = body.trait_declarations().collect::<Vec<_>>();

        let [trait_declaration] = traits.as_slice() else {
            panic!("expected one trait declaration: {traits:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(body.full_text(), "{ trait Display {} }");
        assert_eq!(trait_declaration.full_text(), "trait Display {} ");
        assert!(result.diagnostics().is_empty());
    }

    // TODO(parser): Update this when trait constraints are parsed.
    #[test]
    fn parser_parses_trait_generics_and_skips_constraints_for_now() {
        let source = "module main; trait Iterable<T> with(T: Item) {}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.trait_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one trait declaration: {declarations:?}");
        };

        let skipped_syntax = declaration.skipped_syntax().collect::<Vec<_>>();

        let [constraint] = skipped_syntax.as_slice() else {
            panic!("expected constraint as skipped syntax: {skipped_syntax:?}");
        };

        assert_eq!(source_unit.full_text(), source);

        let generic_parameter_list = match declaration.generic_parameter_list() {
            Some(list) => list,
            None => panic!("expected generic parameter list"),
        };

        assert_eq!(generic_parameter_list.full_text(), "<T> ");
        assert_eq!(generic_parameter_list.generic_type_parameters().count(), 1);
        assert_eq!(constraint.full_text(), "with(T: Item) ");
        assert_eq!(declaration.trait_body().full_text(), "{}");

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [DiagnosticKind::SyntaxSkippedSyntax]
        );
    }

    #[test]
    fn parser_parses_trait_callable_members_without_losing_later_items() {
        let source = "module main; trait Display { func show<T>(); }\nusing core;";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.trait_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one trait declaration: {declarations:?}");
        };

        let body = declaration.trait_body();

        let members = body
            .trait_callable_member_declarations()
            .collect::<Vec<_>>();

        let [member] = members.as_slice() else {
            panic!("expected one trait callable member: {members:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(body.full_text(), "{ func show<T>(); }\n");
        assert_eq!(member.full_text(), "func show<T>(); ");
        assert!(member.semicolon_token().is_some());

        let generic_parameter_list = match member.generic_parameter_list() {
            Some(list) => list,
            None => panic!("expected generic parameter list"),
        };

        assert_eq!(generic_parameter_list.full_text(), "<T>");
        assert_eq!(generic_parameter_list.generic_type_parameters().count(), 1);
        assert_eq!(body.skipped_syntax().count(), 0);
        assert_eq!(source_unit.using_declarations().count(), 1);

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_parses_trait_constant_members() {
        let source = "module main; trait Config { const Size: Int; const Name: String = \"bray\"; func read(); }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.trait_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one trait declaration: {declarations:?}");
        };

        let body = declaration.trait_body();

        let constants = body
            .trait_constant_member_declarations()
            .collect::<Vec<_>>();

        let callables = body
            .trait_callable_member_declarations()
            .collect::<Vec<_>>();

        let [required_constant, defaulted_constant] = constants.as_slice() else {
            panic!("expected two trait constant members: {constants:?}");
        };

        let [callable] = callables.as_slice() else {
            panic!("expected one trait callable member: {callables:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(required_constant.full_text(), "const Size: Int; ");
        assert!(required_constant.equals_token().is_none());

        assert_eq!(
            defaulted_constant.full_text(),
            "const Name: String = \"bray\"; "
        );

        assert!(defaulted_constant.equals_token().is_some());

        assert_eq!(
            defaulted_constant
                .expression()
                .map(|expression| expression.full_text()),
            Some(String::from("\"bray\""))
        );

        assert_eq!(callable.full_text(), "func read(); ");

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_scan_ahead_recognizes_trait_declarations_without_consuming_tokens() {
        let sources = source_store(["public trait Display {}"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);

        assert!(parser.should_parse_trait_declaration());
        assert_eq!(parser.peek().kind(), SyntaxKind::PublicKeyword);
    }

    #[test]
    fn parser_trait_body_missing_open_brace_does_not_consume_following_item() {
        let source = "module main; trait Display\nusing core;";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.trait_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one trait declaration: {declarations:?}");
        };

        let insertion = marker_offset(source, "using");

        let body = declaration.trait_body();

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
    fn parser_trait_scan_rejects_visibility_on_other_declarations() {
        let sources = source_store(["public func main() {}"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);

        assert!(!parser.should_parse_trait_declaration());

        let diagnostics = parser.finish();

        assert!(diagnostics.is_empty());
    }
}
