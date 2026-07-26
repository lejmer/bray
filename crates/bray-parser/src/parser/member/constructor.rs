use bray_syntax::{
    ConstructorMemberModifiersSyntax, SyntaxKind, TypeConstructorMemberDeclarationSyntax,
};

use crate::parser::member::body::{MEMBER_ITEM_RECOVERY_KINDS, MEMBER_KEYWORD_RECOVERY_KINDS};
use crate::parser::state::Parser;

const TYPE_CONSTRUCTOR_MEMBER_START_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::TrustedKeyword,
];

const CONSTRUCTOR_CONTRACT_TAIL_BOUNDARY_KINDS: [SyntaxKind; 4] = [
    SyntaxKind::OpenBraceToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const CONSTRUCTOR_BODY_MISSING_BOUNDARY_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

impl Parser {
    pub(super) fn parse_type_constructor_member_declaration(
        &mut self,
    ) -> TypeConstructorMemberDeclarationSyntax {
        let start = self.peek().full_range().start();

        let mut builder =
            TypeConstructorMemberDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_constructor_member_modifiers(self.parse_constructor_member_modifiers());
        builder.push_construct_keyword(self.expect(SyntaxKind::ConstructKeyword));

        if self.at(SyntaxKind::IdentifierToken) {
            builder.push_identifier_token(self.parse_identifier());
        }

        builder.push_parameter_list(self.parse_parameter_list());

        builder.push_callable_result_clause(
            self.parse_callable_result_clause_until(Parser::at_constructor_result_type_boundary),
        );

        self.parse_callable_contract_clauses(
            &mut builder,
            Parser::at_constructor_contract_boundary,
        );

        builder.push_callable_body_block_expression(
            self.parse_callable_body_block_expression_until(
                Parser::at_constructor_body_missing_boundary,
            ),
        );

        builder.build()
    }

    fn parse_constructor_member_modifiers(&mut self) -> ConstructorMemberModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ConstructorMemberModifiersSyntax::builder(self.syntax_source(), start);

        while self.at_constructor_member_modifier() {
            if self.at_visibility_modifier() {
                builder.push_visibility_token(self.parse_visibility_modifier());
                continue;
            }

            builder.push_trusted_token(self.expect(SyntaxKind::TrustedKeyword));
        }

        builder.build()
    }

    fn at_constructor_result_type_boundary(&mut self) -> bool {
        self.at_constructor_contract_boundary()
    }

    fn at_constructor_contract_boundary(&mut self) -> bool {
        self.at_callable_contract_clause_start()
            || self.at_any(&CONSTRUCTOR_CONTRACT_TAIL_BOUNDARY_KINDS)
            || self.at_any(&MEMBER_KEYWORD_RECOVERY_KINDS)
    }

    fn at_constructor_body_missing_boundary(&mut self) -> bool {
        self.at_any(&CONSTRUCTOR_BODY_MISSING_BOUNDARY_KINDS)
            || self.at_any(&MEMBER_ITEM_RECOVERY_KINDS)
    }

    pub(super) fn should_parse_type_constructor_member_declaration(&mut self) -> bool {
        if !self.at(SyntaxKind::ConstructKeyword)
            && !self.at_any(&TYPE_CONSTRUCTOR_MEMBER_START_KINDS)
        {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.consume_constructor_member_modifiers_for_scan();

            scan.at(SyntaxKind::ConstructKeyword)
        })
    }

    fn at_constructor_member_modifier(&mut self) -> bool {
        self.at_visibility_modifier() || self.at(SyntaxKind::TrustedKeyword)
    }

    fn consume_constructor_member_modifiers_for_scan(&mut self) {
        while self.at_constructor_member_modifier() {
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
    use crate::test_support::{parse_diagnostic_kinds, source};

    use super::super::super::state::Parser;

    #[test]
    fn parser_parses_type_constructor_members() {
        let source = concat!(
            "module main; ",
            "struct Point { public trusted construct origin() -> Self {} }"
        );

        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.struct_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one struct declaration: {declarations:?}");
        };

        let constructors = declaration
            .struct_body()
            .type_constructor_member_declarations()
            .collect::<Vec<_>>();

        let [constructor] = constructors.as_slice() else {
            panic!("expected one constructor declaration: {constructors:?}");
        };

        assert_eq!(source_unit.full_text(), source);

        assert_eq!(
            constructor.full_text(),
            "public trusted construct origin() -> Self {} "
        );

        assert_eq!(
            constructor
                .constructor_member_modifiers()
                .visibility_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(
            constructor
                .constructor_member_modifiers()
                .trusted_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::TrustedKeyword)
        );

        assert_eq!(
            constructor.identifier_token().map(|token| token.kind()),
            Some(SyntaxKind::IdentifierToken)
        );

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_scan_ahead_recognizes_constructor_members_without_consuming_tokens() {
        let sources = source_store(["public trusted construct() -> Self {}"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);

        assert!(parser.should_parse_type_constructor_member_declaration());
        assert_eq!(parser.peek().kind(), SyntaxKind::PublicKeyword);

        let diagnostics = parser.finish();

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_constructor_contract_recovery_preserves_following_members() {
        let source =
            "module main; struct Point { construct() -> Self requires(valid; const Id: Int = 1; }";

        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.struct_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one struct declaration: {declarations:?}");
        };

        let body = declaration.struct_body();

        let constructors = body
            .type_constructor_member_declarations()
            .collect::<Vec<_>>();

        let constants = body.constant_declarations().collect::<Vec<_>>();

        let [constructor] = constructors.as_slice() else {
            panic!("expected one constructor declaration: {constructors:?}");
        };

        let [constant] = constants.as_slice() else {
            panic!("expected one constant declaration: {constants:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(constructor.requires_clauses().count(), 1);
        assert_eq!(constant.full_text(), "const Id: Int = 1; ");

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [
                DiagnosticKind::SyntaxExpectedToken,
                DiagnosticKind::SyntaxExpectedToken,
                DiagnosticKind::SyntaxExpectedToken,
            ]
        );
    }
}
