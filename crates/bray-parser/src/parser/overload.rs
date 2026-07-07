use bray_syntax::{
    CallableOverloadDeclarationSyntax, ImplementationOverloadDeclarationSyntax,
    ImplementationOverloadSubjectSyntax, OverloadArmListSyntax, OverloadArmListSyntaxBuilder,
    OverloadArmSyntax, OverloadModifiersSyntax, SyntaxKind, SyntaxToken,
};

use super::member::MEMBER_KEYWORD_RECOVERY_KINDS;
use super::module::MODULE_ITEM_START_KINDS;
use super::separated::{SeparatedListSpec, SeparatedListSyntaxSink, separated_list_recovery_kinds};
use super::state::Parser;

const OVERLOAD_ARM_START_KINDS: [SyntaxKind; 1] = [SyntaxKind::IdentifierToken];

impl Parser {
    pub(super) fn parse_callable_overload_declaration(
        &mut self,
    ) -> CallableOverloadDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = CallableOverloadDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_overload_modifiers(self.parse_overload_modifiers());
        builder.push_overload_keyword(self.expect(SyntaxKind::OverloadKeyword));
        builder.push_identifier_token(self.parse_identifier());
        builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));
        builder.push_overload_arm_list(self.parse_overload_arm_list());

        builder.build()
    }

    pub(super) fn parse_implementation_overload_declaration(
        &mut self,
    ) -> ImplementationOverloadDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder =
            ImplementationOverloadDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_overload_modifiers(self.parse_overload_modifiers());
        builder.push_overload_keyword(self.expect(SyntaxKind::OverloadKeyword));
        builder.push_implementation_overload_subject(self.parse_implementation_overload_subject());
        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));
        builder.push_trait_path(self.parse_path());
        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));
        builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));
        builder.push_overload_arm_list(self.parse_overload_arm_list());

        builder.build()
    }

    fn parse_overload_modifiers(&mut self) -> OverloadModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder = OverloadModifiersSyntax::builder(self.syntax_source(), start);

        if self.at_visibility_modifier() {
            builder.push_visibility_token(self.parse_visibility_modifier());
        }

        builder.build()
    }

    fn parse_implementation_overload_subject(&mut self) -> ImplementationOverloadSubjectSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ImplementationOverloadSubjectSyntax::builder(self.syntax_source(), start);

        if self.at(SyntaxKind::OpenParenToken) {
            builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));
            builder.push_ampersand_token(self.expect(SyntaxKind::AmpersandToken));

            if self.at(SyntaxKind::MutKeyword) {
                builder.push_mut_token(self.expect(SyntaxKind::MutKeyword));
            }

            builder.push_path(self.parse_path());
            builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));
        } else {
            builder.push_path(self.parse_path());
        }

        builder.build()
    }

    fn parse_overload_arm_list(&mut self) -> OverloadArmListSyntax {
        let start = self.peek().full_range().start();
        let terminators = overload_arm_list_terminators();
        let recovery_kinds = separated_list_recovery_kinds(
            &OVERLOAD_ARM_START_KINDS,
            SyntaxKind::CommaToken,
            &terminators,
        );

        let spec = SeparatedListSpec {
            separator_kind: SyntaxKind::CommaToken,
            terminators: &terminators,
            recovery_kinds: &recovery_kinds,
            allow_trailing_separator: true,
        };

        let mut builder = OverloadArmListSyntax::builder(self.syntax_source(), start);

        builder.push_open_brace_token(self.expect(SyntaxKind::OpenBraceToken));

        if self.at_any(&terminators) {
            builder.push_overload_arm(self.parse_overload_arm());
        } else {
            self.parse_separated_list(&mut builder, spec, Parser::parse_overload_arm);
        }

        builder.push_close_brace_token(self.expect(SyntaxKind::CloseBraceToken));

        builder.build()
    }

    fn parse_overload_arm(&mut self) -> OverloadArmSyntax {
        let start = self.peek().full_range().start();
        let mut builder = OverloadArmSyntax::builder(self.syntax_source(), start);

        builder.push_path(self.parse_path());

        builder.build()
    }

    pub(super) fn should_parse_callable_overload_declaration(&mut self) -> bool {
        if !self.at_overload_declaration_start() {
            return false;
        }

        self.scan_ahead(|scan| {
            if !scan.consume_overload_header_for_scan() {
                return false;
            }

            scan.at(SyntaxKind::IdentifierToken)
                && scan.lookahead(1).kind() == SyntaxKind::EqualsToken
        })
    }

    pub(super) fn should_parse_implementation_overload_declaration(&mut self) -> bool {
        if !self.at_overload_declaration_start() {
            return false;
        }

        self.scan_ahead(|scan| {
            if !scan.consume_overload_header_for_scan() {
                return false;
            }

            if !scan.consume_implementation_overload_subject_for_scan() {
                return false;
            }

            scan.at(SyntaxKind::OpenParenToken)
        })
    }

    fn at_overload_declaration_start(&mut self) -> bool {
        self.at(SyntaxKind::OverloadKeyword) || self.at_visibility_modifier()
    }

    fn consume_overload_header_for_scan(&mut self) -> bool {
        if self.at_visibility_modifier() {
            self.consume();
        }

        if !self.at(SyntaxKind::OverloadKeyword) {
            return false;
        }

        self.consume();

        true
    }

    fn consume_implementation_overload_subject_for_scan(&mut self) -> bool {
        if self.at(SyntaxKind::OpenParenToken) {
            self.consume();

            if !self.at(SyntaxKind::AmpersandToken) {
                return false;
            }

            self.consume();

            if self.at(SyntaxKind::MutKeyword) {
                self.consume();
            }

            if !self.consume_path_for_scan() {
                return false;
            }

            if !self.at(SyntaxKind::CloseParenToken) {
                return false;
            }

            self.consume();

            return true;
        }

        self.consume_path_for_scan()
    }
}

impl SeparatedListSyntaxSink<OverloadArmSyntax> for OverloadArmListSyntaxBuilder {
    fn push_item(&mut self, item: OverloadArmSyntax) {
        OverloadArmListSyntaxBuilder::push_overload_arm(self, item);
    }

    fn push_separator(&mut self, separator: SyntaxToken) {
        OverloadArmListSyntaxBuilder::push_separator_token(self, separator);
    }
}

fn overload_arm_list_terminators() -> Vec<SyntaxKind> {
    let mut terminators =
        Vec::with_capacity(2 + MODULE_ITEM_START_KINDS.len() + MEMBER_KEYWORD_RECOVERY_KINDS.len());

    terminators.push(SyntaxKind::CloseBraceToken);
    terminators.push(SyntaxKind::EndOfFileToken);
    terminators.extend_from_slice(&MODULE_ITEM_START_KINDS);
    terminators.extend_from_slice(&MEMBER_KEYWORD_RECOVERY_KINDS);

    terminators
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_source::TextRange;
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::parser::parse_compilation_unit;
    use crate::test_support::{marker_offset, parse_diagnostic_kinds};

    #[test]
    fn parser_parses_module_level_callable_overload_declarations() {
        let source = "module main; public overload draw = {fast,slow,}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit
            .callable_overload_declarations()
            .collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one callable overload declaration: {declarations:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(
            declaration.full_text(),
            "public overload draw = {fast,slow,}"
        );

        assert_eq!(
            declaration
                .overload_modifiers()
                .visibility_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(declaration.overload_arm_list().overload_arms().count(), 2);
        assert_eq!(
            declaration.overload_arm_list().separator_tokens().count(),
            2
        );
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_parses_module_level_implementation_overload_declarations() {
        let source = "module main; overload (&mut Point)(Shape) = {point_shape}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit
            .implementation_overload_declarations()
            .collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one implementation overload declaration: {declarations:?}");
        };

        let subject = declaration.implementation_overload_subject();

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(
            declaration.full_text(),
            "overload (&mut Point)(Shape) = {point_shape}"
        );

        assert!(subject.open_paren_token().is_some());
        assert!(subject.ampersand_token().is_some());
        assert!(subject.mut_token().is_some());
        assert_eq!(declaration.trait_path().full_text(), "Shape");
        assert_eq!(declaration.overload_arm_list().overload_arms().count(), 1);
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_parses_overload_declarations_in_block_module_bodies() {
        let source = concat!(
            "module main { ",
            "overload draw = {draw_fast} ",
            "overload Point(Shape) = {point_shape} ",
            "}"
        );

        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.block_module_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one block module declaration: {declarations:?}");
        };

        let body = declaration.module_body();

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(body.callable_overload_declarations().count(), 1);
        assert_eq!(body.implementation_overload_declarations().count(), 1);
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_parses_callable_overload_declarations_in_type_and_implementation_bodies() {
        let source = concat!(
            "module main; ",
            "struct Canvas { overload draw = {draw_fast} } ",
            "union Shape { overload area = {circle_area} } ",
            "impl Canvas { overload paint = {paint_basic} }"
        );

        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];

        let struct_declarations = source_unit.struct_declarations().collect::<Vec<_>>();
        let union_declarations = source_unit.union_declarations().collect::<Vec<_>>();
        let implementation_declarations = source_unit
            .inherent_implementation_declarations()
            .collect::<Vec<_>>();

        let [struct_declaration] = struct_declarations.as_slice() else {
            panic!("expected one struct declaration: {struct_declarations:?}");
        };

        let [union_declaration] = union_declarations.as_slice() else {
            panic!("expected one union declaration: {union_declarations:?}");
        };

        let [implementation_declaration] = implementation_declarations.as_slice() else {
            panic!(
                "expected one inherent implementation declaration: {implementation_declarations:?}"
            );
        };

        assert_eq!(source_unit.full_text(), source);

        assert_eq!(
            struct_declaration
                .struct_body()
                .callable_overload_declarations()
                .count(),
            1
        );

        assert_eq!(
            union_declaration
                .union_body()
                .callable_overload_declarations()
                .count(),
            1
        );

        assert_eq!(
            implementation_declaration
                .implementation_body()
                .callable_overload_declarations()
                .count(),
            1
        );

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_reports_missing_overload_arm_separator() {
        let source = "module main; overload draw = {fast slow}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let insertion = marker_offset(source, "slow");

        let declarations = source_unit
            .callable_overload_declarations()
            .collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one callable overload declaration: {declarations:?}");
        };

        let separators = declaration
            .overload_arm_list()
            .separator_tokens()
            .collect::<Vec<_>>();

        let [separator] = separators.as_slice() else {
            panic!("expected one separator token: {separators:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert!(separator.is_missing());
        assert_eq!(separator.range(), TextRange::empty(insertion));

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [DiagnosticKind::SyntaxExpectedToken]
        );
    }

    #[test]
    fn parser_recovers_bad_overload_arm_tokens_without_losing_later_arms() {
        let source = "module main; overload draw = {fast,$,slow}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declarations = source_unit
            .callable_overload_declarations()
            .collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one callable overload declaration: {declarations:?}");
        };

        let arm_list = declaration.overload_arm_list();
        let arms = arm_list.overload_arms().collect::<Vec<_>>();
        let skipped_syntax = arm_list.skipped_syntax().collect::<Vec<_>>();

        let [first, recovered, last] = arms.as_slice() else {
            panic!("expected three overload arms: {arms:?}");
        };

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(first.full_text(), "fast");

        assert!(
            recovered
                .path()
                .identifier_tokens()
                .next()
                .is_some_and(|token| token.is_missing())
        );

        assert_eq!(last.full_text(), "slow");
        assert_eq!(skipped.full_text(), "$");

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxExpectedToken,
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );
    }
}
