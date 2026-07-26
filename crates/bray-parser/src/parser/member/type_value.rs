use bray_syntax::{
    ImplementationTypeMemberBindingSyntax, ImplementationTypeMemberBindingSyntaxBuilder,
    SyntaxKind, TraitTypeMemberDeclarationSyntax, TraitTypeMemberDeclarationSyntaxBuilder,
};

use crate::parser::member::MEMBER_KEYWORD_RECOVERY_KINDS;
use crate::parser::module::MODULE_ITEM_START_KINDS;
use crate::parser::state::Parser;

const IMPLEMENTATION_TYPE_MEMBER_TYPE_BOUNDARY_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const TRAIT_TYPE_MEMBER_DECLARATION_END_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

impl Parser {
    pub(super) fn parse_implementation_type_member_binding(
        &mut self,
    ) -> ImplementationTypeMemberBindingSyntax {
        let start = self.peek().full_range().start();

        let mut builder =
            ImplementationTypeMemberBindingSyntax::builder(self.syntax_source(), start);

        builder.push_type_keyword(self.expect(SyntaxKind::TypeKeyword));
        builder.push_identifier_token(self.parse_identifier());

        let equals_token = self.expect(SyntaxKind::EqualsToken);
        let equals_missing = equals_token.is_missing();

        builder.push_equals_token(equals_token);

        let mut at_type_boundary = Parser::at_implementation_type_member_type_boundary;

        if equals_missing && self.at_implementation_type_member_type_boundary() {
            builder.push_type_expression(self.missing_type_expression());
        } else {
            builder.push_type_expression(self.parse_type_expression_until(&mut at_type_boundary));
        }

        self.recover_until_implementation_type_member_binding_end(&mut builder);
        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));

        builder.build()
    }

    pub(super) fn parse_trait_type_member_declaration(
        &mut self,
    ) -> TraitTypeMemberDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TraitTypeMemberDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_type_keyword(self.expect(SyntaxKind::TypeKeyword));
        builder.push_identifier_token(self.parse_identifier());
        self.recover_until_trait_type_member_declaration_end(&mut builder);
        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));

        builder.build()
    }

    fn at_implementation_type_member_type_boundary(&mut self) -> bool {
        self.at_any(&IMPLEMENTATION_TYPE_MEMBER_TYPE_BOUNDARY_KINDS)
            || self.at_any(&MEMBER_KEYWORD_RECOVERY_KINDS)
            || self.at_any(&MODULE_ITEM_START_KINDS)
    }

    fn recover_until_implementation_type_member_binding_end(
        &mut self,
        builder: &mut ImplementationTypeMemberBindingSyntaxBuilder,
    ) {
        self.recover_until_predicate(builder, Parser::at_implementation_type_member_type_boundary);
    }

    fn recover_until_trait_type_member_declaration_end(
        &mut self,
        builder: &mut TraitTypeMemberDeclarationSyntaxBuilder,
    ) {
        self.recover_until_predicate(builder, Parser::at_trait_type_member_declaration_end);
    }

    fn at_trait_type_member_declaration_end(&mut self) -> bool {
        self.at_any(&TRAIT_TYPE_MEMBER_DECLARATION_END_KINDS)
            || self.at_any(&MEMBER_KEYWORD_RECOVERY_KINDS)
    }

    pub(super) fn should_parse_trait_type_member_declaration(&mut self) -> bool {
        self.at(SyntaxKind::TypeKeyword)
    }

    pub(super) fn should_parse_implementation_type_member_binding(&mut self) -> bool {
        self.at(SyntaxKind::TypeKeyword)
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_source::TextRange;
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::parser::parse_compilation_unit;
    use crate::test_support::{
        assert_missing_semicolon_diagnostic, marker_offset, parse_diagnostic_kinds,
    };

    #[test]
    fn parser_parses_implementation_type_member_bindings() {
        let source = "module main; impl Point { type Item = Element; }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declarations = source_unit
            .inherent_implementation_declarations()
            .collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one inherent implementation declaration: {declarations:?}");
        };

        let type_members = declaration
            .implementation_body()
            .implementation_type_member_bindings()
            .collect::<Vec<_>>();

        let [type_member] = type_members.as_slice() else {
            panic!("expected one implementation type member binding: {type_members:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(type_member.full_text(), "type Item = Element; ");
        assert_eq!(type_member.type_keyword().kind(), SyntaxKind::TypeKeyword);
        assert_eq!(type_member.equals_token().kind(), SyntaxKind::EqualsToken);
        assert_eq!(type_member.type_expression().full_text(), "Element");

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_reports_missing_implementation_type_member_equals() {
        let source = "module main; impl Point { type Item; }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declarations = source_unit
            .inherent_implementation_declarations()
            .collect::<Vec<_>>();

        let insertion = marker_offset(source, "; }");

        let [declaration] = declarations.as_slice() else {
            panic!("expected one inherent implementation declaration: {declarations:?}");
        };

        let type_members = declaration
            .implementation_body()
            .implementation_type_member_bindings()
            .collect::<Vec<_>>();

        let [type_member] = type_members.as_slice() else {
            panic!("expected one implementation type member binding: {type_members:?}");
        };

        let equals_token = type_member.equals_token();

        assert_eq!(source_unit.full_text(), source);
        assert!(equals_token.is_missing());
        assert_eq!(equals_token.range(), TextRange::empty(insertion));

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [DiagnosticKind::SyntaxExpectedToken]
        );
    }

    #[test]
    fn parser_parses_trait_type_member_declarations() {
        let source = "module main; trait Iterator { type Item; }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.trait_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one trait declaration: {declarations:?}");
        };

        let type_members = declaration
            .trait_body()
            .trait_type_member_declarations()
            .collect::<Vec<_>>();

        let [type_member] = type_members.as_slice() else {
            panic!("expected one trait type member declaration: {type_members:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(type_member.full_text(), "type Item; ");
        assert_eq!(type_member.type_keyword().kind(), SyntaxKind::TypeKeyword);
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_reports_missing_trait_type_member_semicolon_before_following_member() {
        let source = "module main; trait Iterator { type Item\nconst Count: Int; }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.trait_declarations().collect::<Vec<_>>();
        let insertion = marker_offset(source, "const");

        let [declaration] = declarations.as_slice() else {
            panic!("expected one trait declaration: {declarations:?}");
        };

        let type_members = declaration
            .trait_body()
            .trait_type_member_declarations()
            .collect::<Vec<_>>();

        let [type_member] = type_members.as_slice() else {
            panic!("expected one trait type member declaration: {type_members:?}");
        };

        let semicolon_token = type_member.semicolon_token();

        assert_eq!(source_unit.full_text(), source);
        assert!(semicolon_token.is_missing());
        assert_eq!(semicolon_token.range(), TextRange::empty(insertion));

        assert_missing_semicolon_diagnostic(
            &result,
            insertion,
            SyntaxKind::ConstKeyword,
            "const",
            &[DiagnosticKind::SyntaxExpectedToken],
        );
    }

    #[test]
    fn parser_recovers_disallowed_trait_type_member_defaults_as_skipped_syntax() {
        let source = "module main; trait Iterator { type Item = Value; }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.trait_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one trait declaration: {declarations:?}");
        };

        let type_members = declaration
            .trait_body()
            .trait_type_member_declarations()
            .collect::<Vec<_>>();

        let [type_member] = type_members.as_slice() else {
            panic!("expected one trait type member declaration: {type_members:?}");
        };

        let skipped_syntax = type_member.skipped_syntax().collect::<Vec<_>>();

        let [default] = skipped_syntax.as_slice() else {
            panic!("expected skipped type member default: {skipped_syntax:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(default.full_text(), "= Value");

        assert_eq!(parse_diagnostic_kinds(&result), []);
    }
}
