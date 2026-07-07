use bray_syntax::{
    ImplementationBodySyntax, ImplementationSubjectSyntax, ImplementationSubjectSyntaxBuilder,
    InherentImplementationDeclarationSyntax, NamedTraitImplementationDeclarationSyntax, SyntaxKind,
    TraitApplicationSyntax, UnnamedTraitImplementationDeclarationSyntax,
};

use super::module::MODULE_ITEM_START_KINDS;
use super::recovery::RecoverySyntaxSink;
use super::state::Parser;

const IMPLEMENTATION_AFTER_SUBJECT_BOUNDARY_KINDS: [SyntaxKind; 6] = [
    SyntaxKind::OpenParenToken,
    SyntaxKind::WithKeyword,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const TRAIT_APPLICATION_BOUNDARY_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::CloseParenToken,
    SyntaxKind::WithKeyword,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const IMPLEMENTATION_CONSTRAINT_BOUNDARY_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::WithKeyword,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const IMPLEMENTATION_BODY_MISSING_BOUNDARY_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BorrowPrefixPolicy {
    Allow,
    Disallow,
}

impl Parser {
    pub(super) fn parse_inherent_implementation_declaration(
        &mut self,
    ) -> InherentImplementationDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder =
            InherentImplementationDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_impl_keyword(self.expect(SyntaxKind::ImplKeyword));

        builder.push_implementation_subject(
            self.parse_implementation_subject(BorrowPrefixPolicy::Disallow),
        );

        self.parse_implementation_constraints(&mut builder);

        builder.push_implementation_body(self.parse_implementation_body());

        builder.build()
    }

    pub(super) fn parse_unnamed_trait_implementation_declaration(
        &mut self,
    ) -> UnnamedTraitImplementationDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder =
            UnnamedTraitImplementationDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_impl_keyword(self.expect(SyntaxKind::ImplKeyword));

        builder.push_implementation_subject(
            self.parse_implementation_subject(BorrowPrefixPolicy::Allow),
        );

        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));
        builder.push_trait_application(self.parse_trait_application());
        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        self.parse_implementation_constraints(&mut builder);

        builder.push_implementation_body(self.parse_implementation_body());

        builder.build()
    }

    pub(super) fn parse_named_trait_implementation_declaration(
        &mut self,
    ) -> NamedTraitImplementationDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder =
            NamedTraitImplementationDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_impl_keyword(self.expect(SyntaxKind::ImplKeyword));

        builder.push_identifier_token(self.parse_identifier());
        builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));
        builder.push_implementation_subject(
            self.parse_implementation_subject(BorrowPrefixPolicy::Allow),
        );

        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));
        builder.push_trait_application(self.parse_trait_application());
        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        self.parse_implementation_constraints(&mut builder);

        builder.push_implementation_body(self.parse_implementation_body());

        builder.build()
    }

    fn parse_implementation_subject(
        &mut self,
        borrow_prefix_policy: BorrowPrefixPolicy,
    ) -> ImplementationSubjectSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ImplementationSubjectSyntax::builder(self.syntax_source(), start);

        match borrow_prefix_policy {
            BorrowPrefixPolicy::Allow => self.parse_implementation_borrow_prefix(&mut builder),
            BorrowPrefixPolicy::Disallow => {
                if self.at(SyntaxKind::AmpersandToken) {
                    self.recover_current_and_until_predicate(
                        &mut builder,
                        Parser::at_implementation_named_subject_or_boundary,
                    );
                }
            }
        }

        builder.push_path(self.parse_path());

        if self.at(SyntaxKind::LessToken) {
            // TODO(parser): Parse generic argument lists once generic syntax is implemented.
            self.recover_current_and_until_predicate(
                &mut builder,
                Parser::at_implementation_subject_boundary,
            );
        }

        builder.build()
    }

    fn parse_implementation_borrow_prefix(
        &mut self,
        builder: &mut ImplementationSubjectSyntaxBuilder,
    ) {
        if !self.at(SyntaxKind::AmpersandToken) {
            return;
        }

        builder.push_ampersand_token(self.expect(SyntaxKind::AmpersandToken));

        if self.at(SyntaxKind::MutKeyword) {
            builder.push_mut_token(self.expect(SyntaxKind::MutKeyword));
        }
    }

    fn parse_trait_application(&mut self) -> TraitApplicationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TraitApplicationSyntax::builder(self.syntax_source(), start);

        builder.push_path(self.parse_path());

        if self.at(SyntaxKind::LessToken) {
            // TODO(parser): Parse generic argument lists once generic syntax is implemented.
            self.recover_current_and_until_predicate(
                &mut builder,
                Parser::at_trait_application_boundary,
            );
        }

        builder.build()
    }

    fn parse_implementation_constraints(&mut self, builder: &mut impl RecoverySyntaxSink) {
        while self.at(SyntaxKind::WithKeyword) {
            // TODO(parser): Parse implementation constraint clauses once expressions are implemented.
            self.recover_current_and_until_predicate(
                builder,
                Parser::at_implementation_constraint_boundary,
            );
        }
    }

    fn parse_implementation_body(&mut self) -> ImplementationBodySyntax {
        let start = self.peek().full_range().start();
        let mut builder = ImplementationBodySyntax::builder(self.syntax_source(), start);

        self.parse_braced_body_contents(
            &mut builder,
            Parser::at_implementation_body_missing_boundary,
            Parser::parse_implementation_body_items,
        );

        builder.build()
    }

    fn at_implementation_named_subject_or_boundary(&mut self) -> bool {
        self.at(SyntaxKind::IdentifierToken) || self.at_implementation_subject_boundary()
    }

    fn at_implementation_subject_boundary(&mut self) -> bool {
        self.at_any(&IMPLEMENTATION_AFTER_SUBJECT_BOUNDARY_KINDS)
            || self.at_any(&MODULE_ITEM_START_KINDS)
    }

    fn at_trait_application_boundary(&mut self) -> bool {
        self.at_any(&TRAIT_APPLICATION_BOUNDARY_KINDS) || self.at_any(&MODULE_ITEM_START_KINDS)
    }

    fn at_implementation_constraint_boundary(&mut self) -> bool {
        self.at_any(&IMPLEMENTATION_CONSTRAINT_BOUNDARY_KINDS)
            || self.at_any(&MODULE_ITEM_START_KINDS)
    }

    fn at_implementation_body_missing_boundary(&mut self) -> bool {
        self.at_any(&IMPLEMENTATION_BODY_MISSING_BOUNDARY_KINDS)
            || self.at_any(&MODULE_ITEM_START_KINDS)
    }

    pub(super) fn should_parse_inherent_implementation_declaration(&mut self) -> bool {
        self.at(SyntaxKind::ImplKeyword)
    }

    pub(super) fn should_parse_unnamed_trait_implementation_declaration(&mut self) -> bool {
        if !self.at(SyntaxKind::ImplKeyword) {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.consume();

            if scan.at_named_trait_implementation_prefix() {
                return false;
            }

            if !scan.consume_implementation_subject_for_scan() {
                return false;
            }

            scan.at(SyntaxKind::OpenParenToken)
        })
    }

    pub(super) fn should_parse_named_trait_implementation_declaration(&mut self) -> bool {
        self.at(SyntaxKind::ImplKeyword) && self.at_named_trait_implementation_prefix()
    }

    fn at_named_trait_implementation_prefix(&mut self) -> bool {
        self.lookahead(1).kind() == SyntaxKind::IdentifierToken
            && self.lookahead(2).kind() == SyntaxKind::EqualsToken
    }

    fn consume_implementation_subject_for_scan(&mut self) -> bool {
        if self.at(SyntaxKind::AmpersandToken) {
            self.consume();

            if self.at(SyntaxKind::MutKeyword) {
                self.consume();
            }
        }

        if !self.consume_path_for_scan() {
            return false;
        }

        self.consume_optional_generic_arguments_for_scan();

        true
    }

    fn consume_optional_generic_arguments_for_scan(&mut self) {
        if !self.at(SyntaxKind::LessToken) {
            return;
        }

        let mut depth = 0usize;

        while !self.at(SyntaxKind::EndOfFileToken) {
            let kind = self.consume().kind();

            match kind {
                SyntaxKind::LessToken => depth += 1,
                SyntaxKind::GreaterToken => {
                    depth = depth.saturating_sub(1);

                    if depth == 0 {
                        return;
                    }
                }
                _ => {}
            }
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
    fn parser_parses_inherent_implementation_declarations_after_source_unit_modules() {
        let source = "module main; impl Point {}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declarations = source_unit
            .inherent_implementation_declarations()
            .collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one inherent implementation declaration: {declarations:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(declaration.full_text(), "impl Point {}");

        assert_eq!(
            declaration.implementation_subject().path().full_text(),
            "Point "
        );

        assert_eq!(declaration.implementation_body().full_text(), "{}");
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_parses_unnamed_trait_implementation_declarations_inside_block_modules() {
        let source = "module main { impl Point(Equatable) {} }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let modules = source_unit.block_module_declarations().collect::<Vec<_>>();

        let [module] = modules.as_slice() else {
            panic!("expected one block module declaration: {modules:?}");
        };

        let declarations = module
            .module_body()
            .unnamed_trait_implementation_declarations()
            .collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one unnamed trait implementation declaration: {declarations:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(declaration.full_text(), "impl Point(Equatable) {} ");

        assert_eq!(
            declaration.implementation_subject().path().full_text(),
            "Point"
        );

        assert_eq!(
            declaration.trait_application().path().full_text(),
            "Equatable"
        );

        assert_eq!(declaration.implementation_body().full_text(), "{} ");
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_parses_named_trait_implementation_declarations() {
        let source = "module main; impl PointEq = Point(Equatable) {}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit
            .named_trait_implementation_declarations()
            .collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one named trait implementation declaration: {declarations:?}");
        };

        assert_eq!(source_unit.full_text(), source);

        assert_eq!(
            declaration.full_text(),
            "impl PointEq = Point(Equatable) {}"
        );

        assert_eq!(
            declaration.identifier_token().kind(),
            SyntaxKind::IdentifierToken
        );

        assert_eq!(
            declaration.implementation_subject().path().full_text(),
            "Point"
        );

        assert_eq!(
            declaration.trait_application().path().full_text(),
            "Equatable"
        );

        assert!(result.diagnostics().is_empty());
    }

    // TODO(parser): Update this when implementation generics and constraints are parsed.
    #[test]
    fn parser_parses_trait_implementation_callable_members_after_skipped_header_forms() {
        let source = "module main; impl Buffer<T>(Reader<Bytes>) with(T: Copy) { func read() {} }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit
            .unnamed_trait_implementation_declarations()
            .collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one unnamed trait implementation declaration: {declarations:?}");
        };

        let subject_skipped = declaration
            .implementation_subject()
            .skipped_syntax()
            .collect::<Vec<_>>();

        let trait_skipped = declaration
            .trait_application()
            .skipped_syntax()
            .collect::<Vec<_>>();

        let declaration_skipped = declaration.skipped_syntax().collect::<Vec<_>>();

        let body = declaration.implementation_body();
        let members = body.type_callable_member_declarations().collect::<Vec<_>>();

        let [subject_generics] = subject_skipped.as_slice() else {
            panic!("expected subject generics as skipped syntax: {subject_skipped:?}");
        };

        let [trait_generics] = trait_skipped.as_slice() else {
            panic!("expected trait generics as skipped syntax: {trait_skipped:?}");
        };

        let [
            declaration_subject_generics,
            declaration_trait_generics,
            constraint,
        ] = declaration_skipped.as_slice()
        else {
            panic!(
                "expected generic arguments and constraint as skipped syntax: {declaration_skipped:?}"
            );
        };

        let [member] = members.as_slice() else {
            panic!("expected one trait implementation callable member: {members:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(subject_generics.full_text(), "<T>");
        assert_eq!(trait_generics.full_text(), "<Bytes>");
        assert_eq!(declaration_subject_generics.full_text(), "<T>");
        assert_eq!(declaration_trait_generics.full_text(), "<Bytes>");
        assert_eq!(constraint.full_text(), "with(T: Copy) ");
        assert_eq!(member.full_text(), "func read() {} ");
        assert_eq!(body.skipped_syntax().count(), 0);

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
    fn parser_parses_inherent_implementation_callable_members_without_losing_later_items() {
        let source = "module main; impl Point { func run() {} }\nusing core;";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit
            .inherent_implementation_declarations()
            .collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one inherent implementation declaration: {declarations:?}");
        };

        let body = declaration.implementation_body();
        let members = body.type_callable_member_declarations().collect::<Vec<_>>();

        let [member] = members.as_slice() else {
            panic!("expected one inherent implementation callable member: {members:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(body.full_text(), "{ func run() {} }\n");
        assert_eq!(member.full_text(), "func run() {} ");
        assert_eq!(body.skipped_syntax().count(), 0);
        assert_eq!(source_unit.using_declarations().count(), 1);

        assert!(result.diagnostics().is_empty());
    }

    // TODO(parser): Update this when constant type and value expressions are parsed.
    #[test]
    fn parser_parses_constant_members_in_implementation_bodies() {
        let source = concat!(
            "module main; ",
            "impl Point { const Origin: Point = zero; } ",
            "impl Point(Shape) { const Sides: Int = 4; }"
        );

        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let inherent_declarations = source_unit
            .inherent_implementation_declarations()
            .collect::<Vec<_>>();

        let trait_declarations = source_unit
            .unnamed_trait_implementation_declarations()
            .collect::<Vec<_>>();

        let [inherent] = inherent_declarations.as_slice() else {
            panic!("expected one inherent implementation declaration: {inherent_declarations:?}");
        };

        let [trait_implementation] = trait_declarations.as_slice() else {
            panic!("expected one trait implementation declaration: {trait_declarations:?}");
        };

        let inherent_constants = inherent
            .implementation_body()
            .constant_declarations()
            .collect::<Vec<_>>();

        let trait_constants = trait_implementation
            .implementation_body()
            .constant_declarations()
            .collect::<Vec<_>>();

        let [inherent_constant] = inherent_constants.as_slice() else {
            panic!("expected one inherent constant declaration: {inherent_constants:?}");
        };

        let [trait_constant] = trait_constants.as_slice() else {
            panic!("expected one trait implementation constant declaration: {trait_constants:?}");
        };

        assert_eq!(source_unit.full_text(), source);

        assert_eq!(
            inherent_constant.full_text(),
            "const Origin: Point = zero; "
        );

        assert_eq!(trait_constant.full_text(), "const Sides: Int = 4; ");

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [
                DiagnosticKind::SyntaxSkippedSyntax,
                DiagnosticKind::SyntaxSkippedSyntax,
                DiagnosticKind::SyntaxSkippedSyntax,
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );
    }

    // TODO(parser): Update this when type and predicate expressions are parsed.
    #[test]
    fn parser_parses_shared_members_in_implementation_bodies() {
        let source = concat!(
            "module main; ",
            "impl Point { type Item = Element; predicate valid(value: Int); } ",
            "impl Point(Shape) { type Area = Float; predicate convex(); }"
        );

        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let inherent_declarations = source_unit
            .inherent_implementation_declarations()
            .collect::<Vec<_>>();

        let trait_declarations = source_unit
            .unnamed_trait_implementation_declarations()
            .collect::<Vec<_>>();

        let [inherent] = inherent_declarations.as_slice() else {
            panic!("expected one inherent implementation declaration: {inherent_declarations:?}");
        };

        let [trait_implementation] = trait_declarations.as_slice() else {
            panic!("expected one trait implementation declaration: {trait_declarations:?}");
        };

        let inherent_body = inherent.implementation_body();
        let trait_body = trait_implementation.implementation_body();

        assert_eq!(source_unit.full_text(), source);

        assert_eq!(
            inherent_body.implementation_type_member_bindings().count(),
            1
        );

        assert_eq!(inherent_body.predicate_declarations().count(), 1);
        assert_eq!(trait_body.implementation_type_member_bindings().count(), 1);
        assert_eq!(trait_body.predicate_declarations().count(), 1);

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
    fn parser_implementation_body_missing_open_brace_does_not_consume_following_item() {
        let source = "module main; impl Point\nusing core;";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit
            .inherent_implementation_declarations()
            .collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one inherent implementation declaration: {declarations:?}");
        };

        let insertion = marker_offset(source, "using");
        let body = declaration.implementation_body();

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
    fn parser_scan_ahead_recognizes_implementation_forms_without_consuming_tokens() {
        let sources = source_store([
            "impl Point {}",
            "impl Point(Equatable) {}",
            "impl PointEq = Point(Equatable) {}",
        ]);

        let mut inherent = Parser::new(source(&sources, 0));
        let mut unnamed = Parser::new(source(&sources, 1));
        let mut named = Parser::new(source(&sources, 2));

        assert!(inherent.should_parse_inherent_implementation_declaration());
        assert!(!inherent.should_parse_unnamed_trait_implementation_declaration());
        assert!(!inherent.should_parse_named_trait_implementation_declaration());
        assert_eq!(inherent.peek().kind(), SyntaxKind::ImplKeyword);

        assert!(unnamed.should_parse_unnamed_trait_implementation_declaration());
        assert!(!unnamed.should_parse_named_trait_implementation_declaration());
        assert_eq!(unnamed.peek().kind(), SyntaxKind::ImplKeyword);

        assert!(named.should_parse_named_trait_implementation_declaration());
        assert_eq!(named.peek().kind(), SyntaxKind::ImplKeyword);
    }
}
