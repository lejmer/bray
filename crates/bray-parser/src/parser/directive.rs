use bray_syntax::{
    AbiDirectiveSyntax, AccessExpressionSyntax, CopyDirectiveSyntax, DirectiveArgumentListSyntax,
    DirectiveArgumentListSyntaxBuilder, DirectiveArgumentSyntax, EntrypointDirectiveSyntax,
    ExpressionSyntax, LayoutDirectiveSyntax, LinkDirectiveSyntax, PrimaryExpressionSyntax,
    SymbolDirectiveSyntax, SyntaxKind, SyntaxToken, TagDirectiveSyntax, TargetDirectiveSyntax,
    TestDirectiveSyntax, ThreadLocalDirectiveSyntax,
};

use super::expression::EXPRESSION_START_KINDS;
use super::recovery::RecoverySyntaxSink;
use super::separated::{SeparatedListSpec, SeparatedListSyntaxSink, separated_list_recovery_kinds};
use super::state::Parser;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DirectiveScanKind {
    ArgumentList,
    Bare,
    Unknown,
}

impl Parser {
    pub(super) fn recover_unsupported_directive(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
        stop_kinds: &[SyntaxKind],
    ) {
        let marker = self.peek();
        let name = self.lookahead(1);

        if name.kind() == SyntaxKind::IdentifierToken
            && let Some(text) = self.token_text(&name)
            && let Some(kind) = SyntaxKind::directive_from_name(text)
        {
            let source = self.syntax_source();

            self.record_syntax_diagnostic(crate::diagnostic::invalid_directive_target(
                &source, &marker, &name, kind,
            ));
        }

        self.recover_current_and_until(builder, stop_kinds);
    }

    pub(super) fn parse_target_directive(
        &mut self,
        argument_recovery_kinds: &[SyntaxKind],
    ) -> TargetDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TargetDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));

        builder.push_directive_argument_list(
            self.parse_directive_argument_list(argument_recovery_kinds),
        );

        builder.build()
    }

    pub(super) fn parse_test_directive(
        &mut self,
        argument_recovery_kinds: &[SyntaxKind],
    ) -> TestDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TestDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));

        if self.at(SyntaxKind::OpenParenToken) {
            builder.push_directive_argument_list(
                self.parse_directive_argument_list(argument_recovery_kinds),
            );
        }

        builder.build()
    }

    pub(super) fn parse_entrypoint_directive(&mut self) -> EntrypointDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = EntrypointDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));

        builder.build()
    }

    pub(super) fn parse_copy_directive(&mut self) -> CopyDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = CopyDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));

        builder.build()
    }

    pub(super) fn parse_thread_local_directive(&mut self) -> ThreadLocalDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ThreadLocalDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));

        builder.build()
    }

    pub(super) fn parse_layout_directive(
        &mut self,
        argument_recovery_kinds: &[SyntaxKind],
    ) -> LayoutDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = LayoutDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));

        builder.push_directive_argument_list(
            self.parse_directive_argument_list(argument_recovery_kinds),
        );

        builder.build()
    }

    pub(super) fn parse_tag_directive(
        &mut self,
        argument_recovery_kinds: &[SyntaxKind],
    ) -> TagDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TagDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));

        builder.push_directive_argument_list(
            self.parse_directive_argument_list(argument_recovery_kinds),
        );

        builder.build()
    }

    pub(super) fn parse_link_directive(
        &mut self,
        argument_recovery_kinds: &[SyntaxKind],
    ) -> LinkDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = LinkDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));

        builder.push_directive_argument_list(
            self.parse_directive_argument_list(argument_recovery_kinds),
        );

        builder.build()
    }

    pub(super) fn parse_abi_directive(
        &mut self,
        argument_recovery_kinds: &[SyntaxKind],
    ) -> AbiDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = AbiDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));

        builder.push_directive_argument_list(
            self.parse_directive_argument_list(argument_recovery_kinds),
        );

        builder.build()
    }

    pub(super) fn parse_symbol_directive(
        &mut self,
        argument_recovery_kinds: &[SyntaxKind],
    ) -> SymbolDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = SymbolDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));

        builder.push_directive_argument_list(
            self.parse_directive_argument_list(argument_recovery_kinds),
        );

        builder.build()
    }

    pub(super) fn parse_directive_argument_list(
        &mut self,
        recovery_kinds: &[SyntaxKind],
    ) -> DirectiveArgumentListSyntax {
        let start = self.peek().full_range().start();
        let terminators = directive_argument_list_terminators(recovery_kinds);

        let list_recovery_kinds = separated_list_recovery_kinds(
            &EXPRESSION_START_KINDS,
            SyntaxKind::CommaToken,
            &terminators,
        );

        let spec = SeparatedListSpec {
            separator_kind: SyntaxKind::CommaToken,
            terminators: &terminators,
            recovery_kinds: &list_recovery_kinds,
            allow_trailing_separator: true,
        };

        let mut builder = DirectiveArgumentListSyntax::builder(self.syntax_source(), start);

        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));

        self.parse_separated_list(&mut builder, spec, |parser| {
            parser.parse_directive_argument(&terminators)
        });

        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        builder.build()
    }

    fn parse_directive_argument(&mut self, terminators: &[SyntaxKind]) -> DirectiveArgumentSyntax {
        let start = self.peek().full_range().start();
        let mut builder = DirectiveArgumentSyntax::builder(self.syntax_source(), start);

        let named = self.at(SyntaxKind::IdentifierToken)
            && self.lookahead(1).kind() == SyntaxKind::EqualsToken;

        if named {
            builder.push_name_token(self.parse_identifier());
            builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));
        }

        if named && self.at_static_directive_selector() {
            // Directive selectors use identifier-shaped syntax without changing lexical keyword rules.
            let selector = self.consume().with_kind(SyntaxKind::IdentifierToken);
            let value_start = selector.full_range().start();
            let mut access = AccessExpressionSyntax::builder(self.syntax_source(), value_start);

            access.push_identifier_token(selector);

            let mut primary = PrimaryExpressionSyntax::builder(self.syntax_source(), value_start);

            primary.push_access_expression(access.build());

            let mut expression = ExpressionSyntax::builder(self.syntax_source(), value_start);

            expression.push_primary_expression(primary.build());
            builder.push_expression(expression.build());
        } else {
            let mut at_boundary =
                |parser: &mut Parser| parser.at_directive_argument_boundary(terminators);

            builder.push_expression(self.parse_expression_until(&mut at_boundary));
        }

        builder.build()
    }

    fn at_directive_argument_boundary(&mut self, terminators: &[SyntaxKind]) -> bool {
        self.at(SyntaxKind::CommaToken)
            || self.at_any(terminators)
            || self.at_probable_named_directive_argument_start()
    }

    fn at_probable_named_directive_argument_start(&mut self) -> bool {
        self.at(SyntaxKind::IdentifierToken) && self.lookahead(1).kind() == SyntaxKind::EqualsToken
    }

    pub(super) fn at_directive_kind(&mut self, kind: SyntaxKind) -> bool {
        if !self.at(SyntaxKind::AtToken) {
            return false;
        }

        let Some(name) = kind.directive_name() else {
            return false;
        };

        let name_token = self.lookahead(1);

        name_token.kind() == SyntaxKind::IdentifierToken
            && self.token_text(&name_token) == Some(name)
    }

    fn at_static_directive_selector(&mut self) -> bool {
        if !self.at(SyntaxKind::StaticKeyword) {
            return false;
        }

        match self.lookahead(1).kind() {
            SyntaxKind::MutKeyword => false,
            SyntaxKind::IdentifierToken => self.lookahead(2).kind() == SyntaxKind::EqualsToken,
            _ => true,
        }
    }

    fn scan_directive_arguments(&mut self, stop_kinds: &[SyntaxKind]) {
        let mut depth = 0usize;
        let mut previous = None;

        loop {
            let kind = self.peek().kind();

            let selector =
                previous == Some(SyntaxKind::EqualsToken) && self.at_static_directive_selector();

            if kind == SyntaxKind::EndOfFileToken
                || (depth == 0 && kind == SyntaxKind::CloseParenToken)
                || (depth == 0 && stop_kinds.contains(&kind) && !selector)
            {
                return;
            }

            match kind {
                SyntaxKind::OpenParenToken => depth += 1,
                SyntaxKind::CloseParenToken => depth -= 1,
                _ => {}
            }

            previous = Some(self.consume().kind());
        }
    }

    pub(super) fn consume_directive_argument_list_for_scan(&mut self, stop_kinds: &[SyntaxKind]) {
        if self.consume_if(SyntaxKind::OpenParenToken).is_none() {
            return;
        }

        self.scan_directive_arguments(stop_kinds);
        self.consume_if(SyntaxKind::CloseParenToken);
    }

    pub(super) fn consume_directives_for_scan(
        &mut self,
        stop_kinds: &[SyntaxKind],
        mut classify_directive: impl FnMut(&str) -> DirectiveScanKind,
    ) {
        while self.at(SyntaxKind::AtToken) {
            self.consume();

            if !self.at(SyntaxKind::IdentifierToken) {
                continue;
            }

            let name_token = self.consume();

            let directive_kind = self
                .token_text(&name_token)
                .map_or(DirectiveScanKind::Unknown, &mut classify_directive);

            match directive_kind {
                DirectiveScanKind::ArgumentList => {
                    self.consume_directive_argument_list_for_scan(stop_kinds);
                }
                DirectiveScanKind::Bare => {}
                DirectiveScanKind::Unknown => self.skip_unknown_directive_for_scan(stop_kinds),
            }
        }
    }

    pub(super) fn skip_unknown_directive_for_scan(&mut self, stop_kinds: &[SyntaxKind]) {
        if self.consume_if(SyntaxKind::OpenParenToken).is_some() {
            self.scan_directive_arguments(stop_kinds);
            self.consume_if(SyntaxKind::CloseParenToken);

            return;
        }

        while !self.at_any(stop_kinds) && !self.at(SyntaxKind::EndOfFileToken) {
            self.consume();
        }
    }
}

impl SeparatedListSyntaxSink<DirectiveArgumentSyntax> for DirectiveArgumentListSyntaxBuilder {
    fn push_item(&mut self, item: DirectiveArgumentSyntax) {
        DirectiveArgumentListSyntaxBuilder::push_directive_argument(self, item);
    }

    fn push_separator(&mut self, separator: SyntaxToken) {
        DirectiveArgumentListSyntaxBuilder::push_separator_token(self, separator);
    }
}

fn directive_argument_list_terminators(recovery_kinds: &[SyntaxKind]) -> Vec<SyntaxKind> {
    let mut terminators = Vec::with_capacity(recovery_kinds.len() + 2);

    terminators.push(SyntaxKind::CloseParenToken);
    terminators.extend_from_slice(recovery_kinds);
    terminators.push(SyntaxKind::EndOfFileToken);

    terminators
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::SeparatedSyntaxNode;
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use super::super::state::Parser;
    use crate::test_support::{diagnostic_kinds, source};

    #[test]
    fn static_selectors_do_not_change_declaration_classification() {
        for (text, functions, statics) in [
            (
                r#"
                trusted module app;
                @link(name = "native", kind = static)
                @symbol(name = "read")
                @abi(c)
                extern trusted func read() -> u32 uses(foreign_call);
            "#,
                1,
                0,
            ),
            (
                r#"
                @link(name = "native", kind = static)
                trusted module app;
                @symbol(name = "read")
                @abi(c)
                extern trusted func read() -> u32 uses(foreign_call);
            "#,
                1,
                0,
            ),
            (
                r#"
                trusted module app;
                @link(name = "native", kind = static)
                @symbol(name = "value")
                extern trusted static VALUE: u32;
            "#,
                0,
                1,
            ),
        ] {
            let sources = source_store([text]);
            let result = crate::parser::parse_compilation_unit(&sources);
            let unit = &result.syntax_tree().root().source_units()[0];

            assert!(
                result.diagnostics().is_empty(),
                "{text}: {:?}",
                result.diagnostics()
            );

            assert_eq!(unit.function_declarations().count(), functions, "{text}");
            assert_eq!(unit.static_declarations().count(), statics, "{text}");
        }
    }

    #[test]
    fn missing_selector_values_preserve_following_static_declarations() {
        let sources = source_store([r#"
            module app;
            @link(name = "native", kind =
            static VALUE: u32 = 1;
            func next() {}
        "#]);

        let result = crate::parser::parse_compilation_unit(&sources);
        let unit = &result.syntax_tree().root().source_units()[0];

        assert!(result.diagnostics().has_errors());
        assert_eq!(unit.static_declarations().count(), 1);
        assert_eq!(unit.function_declarations().count(), 1);
    }

    #[test]
    fn parser_preserves_named_static_directive_selectors() {
        let sources = source_store([r#"(name = "native", kind = static)"#]);
        let snapshot = source(&sources, 0);
        let mut parser = Parser::new(snapshot);
        let list = parser.parse_directive_argument_list(&[SyntaxKind::EndOfFileToken]);
        let diagnostics = parser.finish();
        let arguments = list.directive_arguments().collect::<Vec<_>>();

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(arguments.len(), 2);

        assert_eq!(
            arguments[1].name_token().expect("named argument").kind(),
            SyntaxKind::IdentifierToken
        );

        assert_eq!(arguments[1].expression().full_text().trim(), "static");
        assert_eq!(list.full_text(), r#"(name = "native", kind = static)"#);
    }

    #[test]
    fn parser_rejects_positional_static_directive_keywords() {
        let sources = source_store(["(static)"]);
        let snapshot = source(&sources, 0);
        let mut parser = Parser::new(snapshot);

        parser.parse_directive_argument_list(&[SyntaxKind::EndOfFileToken]);
        assert!(!parser.finish().is_empty());
    }

    #[test]
    fn parser_parses_directive_argument_lists() {
        let sources = source_store(["(target=host,\"m\",)"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);

        let list = parser.parse_directive_argument_list(&[SyntaxKind::EndOfFileToken]);
        let diagnostics = parser.finish();
        let arguments = list.directive_arguments().collect::<Vec<_>>();

        let [target, module] = arguments.as_slice() else {
            panic!("expected two directive arguments: {arguments:?}");
        };

        assert_eq!(list.full_text(), "(target=host,\"m\",)");
        assert_eq!(list.separator_tokens().count(), 2);

        assert_eq!(
            target.name_token().map(|token| token.kind()),
            Some(SyntaxKind::IdentifierToken)
        );

        assert_eq!(target.expression().full_text(), "host");
        assert!(module.name_token().is_none());
        assert_eq!(module.expression().full_text(), "\"m\"");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_represents_missing_directive_argument_separators() {
        let sources = source_store(["(first second=tail)"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);

        let list = parser.parse_directive_argument_list(&[SyntaxKind::EndOfFileToken]);
        let diagnostics = parser.finish();
        let arguments = list.directive_arguments().collect::<Vec<_>>();
        let separators = list.separator_tokens().collect::<Vec<_>>();

        let [_first, second] = arguments.as_slice() else {
            panic!("expected two directive arguments: {arguments:?}");
        };

        let [separator] = separators.as_slice() else {
            panic!("expected one directive argument separator: {separators:?}");
        };

        assert_eq!(list.full_text(), "(first second=tail)");
        assert!(separator.is_missing());

        assert_eq!(
            second.name_token().map(|token| token.kind()),
            Some(SyntaxKind::IdentifierToken)
        );

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxExpectedToken]
        );
    }

    #[test]
    fn parser_recovers_bad_directive_arguments_without_losing_later_arguments() {
        let sources = source_store(["($,value)"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);

        let list = parser.parse_directive_argument_list(&[SyntaxKind::EndOfFileToken]);
        let diagnostics = parser.finish();
        let arguments = list.directive_arguments().collect::<Vec<_>>();
        let skipped_syntax = list.skipped_syntax().collect::<Vec<_>>();

        let [recovered, value] = arguments.as_slice() else {
            panic!("expected two directive arguments: {arguments:?}");
        };

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        assert_eq!(list.full_text(), "($,value)");
        assert_eq!(recovered.expression().full_text(), "$");
        assert_eq!(value.expression().full_text(), "value");
        assert_eq!(skipped.full_text(), "$");

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxExpectedExpression,
            ]
        );
    }
}
