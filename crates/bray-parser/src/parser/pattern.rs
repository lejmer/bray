use bray_syntax::{
    CasePatternEntrySyntax, CasePatternEntrySyntaxBuilder, CasePatternSyntax,
    CasePatternSyntaxBuilder, IrrefutablePatternEntrySyntax, IrrefutablePatternEntrySyntaxBuilder,
    IrrefutablePatternSyntax, IrrefutablePatternSyntaxBuilder, PathSyntax, SyntaxKind, SyntaxToken,
};

use super::recovery::RecoverySyntaxSink;
use super::state::Parser;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PatternContext {
    Irrefutable,
    Case,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PatternEntryContext {
    Array,
    Payload,
    Product,
}

enum PatternNode {
    Irrefutable(IrrefutablePatternSyntax),
    Case(CasePatternSyntax),
}

enum PatternEntryNode {
    Irrefutable(IrrefutablePatternEntrySyntax),
    Case(CasePatternEntrySyntax),
}

enum PatternBuilder {
    Irrefutable(IrrefutablePatternSyntaxBuilder),
    Case(CasePatternSyntaxBuilder),
}

enum PatternEntryBuilder {
    Irrefutable(IrrefutablePatternEntrySyntaxBuilder),
    Case(CasePatternEntrySyntaxBuilder),
}

macro_rules! forward_context_token_pushers {
    ($($method:ident),+ $(,)?) => {
        $(
            fn $method(&mut self, token: SyntaxToken) {
                match self {
                    Self::Irrefutable(builder) => builder.$method(token),
                    Self::Case(builder) => builder.$method(token),
                }
            }
        )+
    };
}

macro_rules! define_push_context_pattern {
    () => {
        fn push_pattern(&mut self, pattern: PatternNode) {
            match (self, pattern) {
                (Self::Irrefutable(builder), PatternNode::Irrefutable(pattern)) => {
                    builder.push_irrefutable_pattern(pattern);
                }
                (Self::Case(builder), PatternNode::Case(pattern)) => {
                    builder.push_case_pattern(pattern);
                }
                _ => unreachable!(),
            }
        }
    };
}

macro_rules! impl_context_recovery_sink {
    ($builder:ty) => {
        impl RecoverySyntaxSink for $builder {
            fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
                match self {
                    Self::Irrefutable(builder) => builder.push_skipped_tokens(tokens),
                    Self::Case(builder) => builder.push_skipped_tokens(tokens),
                }
            }
        }
    };
}

impl Parser {
    pub(super) fn parse_irrefutable_pattern_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> IrrefutablePatternSyntax {
        let PatternNode::Irrefutable(pattern) =
            self.parse_pattern_primary_until(PatternContext::Irrefutable, at_boundary)
        else {
            unreachable!();
        };

        pattern
    }

    pub(super) fn parse_case_pattern_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> CasePatternSyntax {
        let first = self.parse_case_pattern_primary_until(at_boundary);

        if !self.at(SyntaxKind::PipeToken) {
            return first;
        }

        let start = first.full_range().start();
        let mut builder = CasePatternSyntax::builder(self.syntax_source(), start);

        builder.push_case_pattern(first);

        while self.at(SyntaxKind::PipeToken) {
            builder.push_pipe_token(self.expect(SyntaxKind::PipeToken));
            builder.push_case_pattern(self.parse_case_pattern_primary_until(at_boundary));
        }

        builder.build()
    }

    fn parse_case_pattern_primary_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> CasePatternSyntax {
        let PatternNode::Case(pattern) = self
            .parse_pattern_primary_until(PatternContext::Case, &mut |parser| {
                parser.at(SyntaxKind::PipeToken) || at_boundary(parser)
            })
        else {
            unreachable!();
        };

        pattern
    }

    fn parse_pattern_until(
        &mut self,
        context: PatternContext,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PatternNode {
        match context {
            PatternContext::Irrefutable => {
                PatternNode::Irrefutable(self.parse_irrefutable_pattern_until(at_boundary))
            }
            PatternContext::Case => PatternNode::Case(self.parse_case_pattern_until(at_boundary)),
        }
    }

    fn parse_pattern_primary_until(
        &mut self,
        context: PatternContext,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PatternNode {
        if at_boundary(self) || at_pattern_hard_boundary(self.peek().kind()) {
            return self.missing_pattern(context);
        }

        match self.peek().kind() {
            SyntaxKind::UnderscoreToken => self.parse_discard_pattern(context),
            SyntaxKind::MutKeyword => self.parse_mutable_binding_pattern(context),
            kind if kind.is_pattern_literal() => self.parse_literal_pattern(context),
            SyntaxKind::NoneKeyword => self.parse_nullable_absent_pattern(context),
            SyntaxKind::QuestionToken => self.parse_nullable_present_pattern(context, at_boundary),
            SyntaxKind::BoxKeyword => self.parse_box_pattern(context, at_boundary),
            SyntaxKind::DotToken => self.parse_leading_dot_pattern(context, at_boundary),
            SyntaxKind::IdentifierToken => self.parse_path_pattern(context, at_boundary),
            SyntaxKind::OpenBraceToken => {
                self.parse_expected_type_product_pattern(context, at_boundary)
            }
            SyntaxKind::OpenParenToken => self.parse_parenthesized_pattern(context, at_boundary),
            SyntaxKind::OpenBracketToken => self.parse_array_pattern(context, at_boundary),
            SyntaxKind::DotDotToken => self.parse_remaining_pattern(context),
            _ => self.parse_unknown_pattern(context, at_boundary),
        }
    }

    fn missing_pattern(&mut self, context: PatternContext) -> PatternNode {
        let start = self.peek().full_range().start();
        let mut builder = PatternBuilder::new(context, self, start);

        builder.push_identifier_token(self.expect(SyntaxKind::IdentifierToken));

        builder.build()
    }

    fn parse_discard_pattern(&mut self, context: PatternContext) -> PatternNode {
        let start = self.peek().full_range().start();
        let mut builder = PatternBuilder::new(context, self, start);

        builder.push_discard_token(self.expect(SyntaxKind::UnderscoreToken));

        builder.build()
    }

    fn parse_mutable_binding_pattern(&mut self, context: PatternContext) -> PatternNode {
        let start = self.peek().full_range().start();
        let mut builder = PatternBuilder::new(context, self, start);

        builder.push_mut_keyword(self.expect(SyntaxKind::MutKeyword));
        builder.push_identifier_token(self.parse_identifier());

        builder.build()
    }

    fn parse_literal_pattern(&mut self, context: PatternContext) -> PatternNode {
        let start = self.peek().full_range().start();
        let mut builder = PatternBuilder::new(context, self, start);

        builder.push_literal_token(self.consume());

        builder.build()
    }

    fn parse_nullable_absent_pattern(&mut self, context: PatternContext) -> PatternNode {
        let start = self.peek().full_range().start();
        let mut builder = PatternBuilder::new(context, self, start);

        builder.push_none_keyword(self.expect(SyntaxKind::NoneKeyword));

        builder.build()
    }

    fn parse_nullable_present_pattern(
        &mut self,
        context: PatternContext,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PatternNode {
        let start = self.peek().full_range().start();
        let mut builder = PatternBuilder::new(context, self, start);

        builder.push_question_token(self.expect(SyntaxKind::QuestionToken));
        builder.push_pattern(self.parse_pattern_primary_until(context, at_boundary));

        builder.build()
    }

    fn parse_box_pattern(
        &mut self,
        context: PatternContext,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PatternNode {
        let start = self.peek().full_range().start();
        let mut builder = PatternBuilder::new(context, self, start);

        builder.push_box_keyword(self.expect(SyntaxKind::BoxKeyword));
        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));

        builder.push_pattern(self.parse_pattern_until(context, &mut |parser| {
            parser.at(SyntaxKind::CloseParenToken) || at_boundary(parser)
        }));

        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        builder.build()
    }

    fn parse_leading_dot_pattern(
        &mut self,
        context: PatternContext,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PatternNode {
        let start = self.peek().full_range().start();
        let mut builder = PatternBuilder::new(context, self, start);

        builder.push_dot_token(self.expect(SyntaxKind::DotToken));
        builder.push_identifier_token(self.parse_identifier());

        if self.at(SyntaxKind::OpenParenToken) {
            self.parse_payload_pattern_body(&mut builder, context, at_boundary);
        }

        builder.build()
    }

    fn parse_path_pattern(
        &mut self,
        context: PatternContext,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PatternNode {
        let start = self.peek().full_range().start();
        let mut builder = PatternBuilder::new(context, self, start);

        builder.push_path(self.parse_path());

        if self.at(SyntaxKind::OpenParenToken) {
            self.parse_payload_pattern_body(&mut builder, context, at_boundary);
        } else if self.at(SyntaxKind::OpenBraceToken) {
            self.parse_product_pattern_body(&mut builder, context, at_boundary);
        }

        builder.build()
    }

    fn parse_expected_type_product_pattern(
        &mut self,
        context: PatternContext,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PatternNode {
        let start = self.peek().full_range().start();
        let mut builder = PatternBuilder::new(context, self, start);

        self.parse_product_pattern_body(&mut builder, context, at_boundary);

        builder.build()
    }

    fn parse_parenthesized_pattern(
        &mut self,
        context: PatternContext,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PatternNode {
        let start = self.peek().full_range().start();
        let mut builder = PatternBuilder::new(context, self, start);

        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));
        builder.push_pattern(self.parse_pattern_until(context, &mut |parser| {
            parser.at_parenthesized_pattern_boundary(at_boundary)
        }));

        while self.at(SyntaxKind::CommaToken) {
            builder.push_separator_token(self.expect(SyntaxKind::CommaToken));

            if self.at(SyntaxKind::CloseParenToken) {
                break;
            }

            builder.push_pattern(self.parse_pattern_until(context, &mut |parser| {
                parser.at_parenthesized_pattern_boundary(at_boundary)
            }));
        }

        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        builder.build()
    }

    fn parse_array_pattern(
        &mut self,
        context: PatternContext,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PatternNode {
        let start = self.peek().full_range().start();
        let mut builder = PatternBuilder::new(context, self, start);

        builder.push_open_bracket_token(self.expect(SyntaxKind::OpenBracketToken));

        self.parse_pattern_entries(
            &mut builder,
            context,
            PatternEntryContext::Array,
            SyntaxKind::CloseBracketToken,
            at_boundary,
        );

        builder.push_close_bracket_token(self.expect(SyntaxKind::CloseBracketToken));

        builder.build()
    }

    fn parse_payload_pattern_body(
        &mut self,
        builder: &mut PatternBuilder,
        context: PatternContext,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) {
        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));

        self.parse_pattern_entries(
            builder,
            context,
            PatternEntryContext::Payload,
            SyntaxKind::CloseParenToken,
            at_boundary,
        );

        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));
    }

    fn parse_product_pattern_body(
        &mut self,
        builder: &mut PatternBuilder,
        context: PatternContext,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) {
        builder.push_open_brace_token(self.expect(SyntaxKind::OpenBraceToken));

        self.parse_pattern_entries(
            builder,
            context,
            PatternEntryContext::Product,
            SyntaxKind::CloseBraceToken,
            at_boundary,
        );

        builder.push_close_brace_token(self.expect(SyntaxKind::CloseBraceToken));
    }

    fn parse_pattern_entries(
        &mut self,
        builder: &mut PatternBuilder,
        context: PatternContext,
        entry_context: PatternEntryContext,
        close_kind: SyntaxKind,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) {
        while !self.at_pattern_entry_sequence_boundary(close_kind, at_boundary) {
            builder.push_entry(
                self.parse_pattern_entry(context, entry_context, &mut |parser| {
                    parser.at(close_kind)
                        || parser.at(SyntaxKind::CommaToken)
                        || at_boundary(parser)
                }),
            );

            if self.at(SyntaxKind::CommaToken) {
                builder.push_separator_token(self.expect(SyntaxKind::CommaToken));
                continue;
            }

            if self.at_pattern_entry_sequence_boundary(close_kind, at_boundary) {
                break;
            }

            builder.push_separator_token(self.expect(SyntaxKind::CommaToken));
        }
    }

    fn parse_pattern_entry(
        &mut self,
        context: PatternContext,
        entry_context: PatternEntryContext,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PatternEntryNode {
        let start = self.peek().full_range().start();
        let mut builder = PatternEntryBuilder::new(context, self, start);

        if self.at(SyntaxKind::DotDotToken) {
            builder.push_dot_dot_token(self.expect(SyntaxKind::DotDotToken));

            return builder.build();
        }

        match entry_context {
            PatternEntryContext::Product => {
                self.parse_product_pattern_entry(&mut builder, context, at_boundary);
            }
            PatternEntryContext::Payload
                if self.at(SyntaxKind::IdentifierToken)
                    && self.lookahead(1).kind() == SyntaxKind::EqualsToken =>
            {
                self.parse_named_pattern_entry(&mut builder, context, at_boundary);
            }
            PatternEntryContext::Array | PatternEntryContext::Payload => {
                builder.push_pattern(self.parse_pattern_until(context, at_boundary));
            }
        }

        builder.build()
    }

    fn parse_product_pattern_entry(
        &mut self,
        builder: &mut PatternEntryBuilder,
        context: PatternContext,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) {
        if !self.at(SyntaxKind::IdentifierToken) {
            self.recover_current_and_until_predicate(builder, |parser| at_boundary(parser));

            return;
        }

        builder.push_identifier_token(self.parse_identifier());

        if self.at(SyntaxKind::EqualsToken) {
            builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));
            builder.push_pattern(self.parse_pattern_until(context, at_boundary));
        }
    }

    fn parse_named_pattern_entry(
        &mut self,
        builder: &mut PatternEntryBuilder,
        context: PatternContext,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) {
        builder.push_identifier_token(self.parse_identifier());
        builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));
        builder.push_pattern(self.parse_pattern_until(context, at_boundary));
    }

    fn parse_remaining_pattern(&mut self, context: PatternContext) -> PatternNode {
        let start = self.peek().full_range().start();
        let mut builder = PatternBuilder::new(context, self, start);

        builder.push_dot_dot_token(self.expect(SyntaxKind::DotDotToken));

        builder.build()
    }

    fn parse_unknown_pattern(
        &mut self,
        context: PatternContext,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PatternNode {
        let start = self.peek().full_range().start();
        let mut builder = PatternBuilder::new(context, self, start);

        self.recover_current_and_until_predicate(&mut builder, |parser| {
            at_boundary(parser) || at_pattern_hard_boundary(parser.peek().kind())
        });

        builder.build()
    }

    fn at_parenthesized_pattern_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        self.at(SyntaxKind::CommaToken) || self.at(SyntaxKind::CloseParenToken) || at_boundary(self)
    }

    fn at_pattern_entry_sequence_boundary(
        &mut self,
        close_kind: SyntaxKind,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        self.at(close_kind)
            || self.at(SyntaxKind::EndOfFileToken)
            || at_boundary(self)
            || at_pattern_entry_sequence_hard_boundary(self.peek().kind())
    }
}

impl PatternBuilder {
    fn new(context: PatternContext, parser: &mut Parser, start: bray_source::TextSize) -> Self {
        match context {
            PatternContext::Irrefutable => Self::Irrefutable(IrrefutablePatternSyntax::builder(
                parser.syntax_source(),
                start,
            )),
            PatternContext::Case => {
                Self::Case(CasePatternSyntax::builder(parser.syntax_source(), start))
            }
        }
    }

    forward_context_token_pushers!(
        push_discard_token,
        push_mut_keyword,
        push_literal_token,
        push_none_keyword,
        push_question_token,
        push_box_keyword,
        push_open_paren_token,
        push_close_paren_token,
        push_dot_token,
        push_identifier_token,
        push_open_bracket_token,
        push_close_bracket_token,
        push_open_brace_token,
        push_close_brace_token,
        push_separator_token,
        push_dot_dot_token,
    );

    fn push_path(&mut self, path: PathSyntax) {
        match self {
            Self::Irrefutable(builder) => builder.push_path(path),
            Self::Case(builder) => builder.push_path(path),
        }
    }

    define_push_context_pattern!();

    fn push_entry(&mut self, entry: PatternEntryNode) {
        match (self, entry) {
            (Self::Irrefutable(builder), PatternEntryNode::Irrefutable(entry)) => {
                builder.push_irrefutable_pattern_entry(entry);
            }
            (Self::Case(builder), PatternEntryNode::Case(entry)) => {
                builder.push_case_pattern_entry(entry);
            }
            _ => unreachable!(),
        }
    }

    fn build(self) -> PatternNode {
        match self {
            Self::Irrefutable(builder) => PatternNode::Irrefutable(builder.build()),
            Self::Case(builder) => PatternNode::Case(builder.build()),
        }
    }
}

impl_context_recovery_sink!(PatternBuilder);

impl PatternEntryBuilder {
    fn new(context: PatternContext, parser: &mut Parser, start: bray_source::TextSize) -> Self {
        match context {
            PatternContext::Irrefutable => Self::Irrefutable(
                IrrefutablePatternEntrySyntax::builder(parser.syntax_source(), start),
            ),
            PatternContext::Case => Self::Case(CasePatternEntrySyntax::builder(
                parser.syntax_source(),
                start,
            )),
        }
    }

    forward_context_token_pushers!(push_identifier_token, push_equals_token, push_dot_dot_token);

    define_push_context_pattern!();

    fn build(self) -> PatternEntryNode {
        match self {
            Self::Irrefutable(builder) => PatternEntryNode::Irrefutable(builder.build()),
            Self::Case(builder) => PatternEntryNode::Case(builder.build()),
        }
    }
}

impl_context_recovery_sink!(PatternEntryBuilder);

fn at_pattern_hard_boundary(kind: SyntaxKind) -> bool {
    kind == SyntaxKind::CommaToken
        || kind == SyntaxKind::EndOfFileToken
        || at_pattern_entry_sequence_hard_boundary(kind)
}

fn at_pattern_entry_sequence_hard_boundary(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::PipeToken
            | SyntaxKind::CloseParenToken
            | SyntaxKind::CloseBracketToken
            | SyntaxKind::CloseBraceToken
            | SyntaxKind::ColonToken
            | SyntaxKind::EqualsToken
            | SyntaxKind::SemicolonToken
    )
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use super::super::state::Parser;
    use crate::test_support::{diagnostic_kinds, source};

    #[test]
    fn parser_parses_irrefutable_pattern_forms() {
        let cases = [
            ("_", 0, 0, 0),
            ("mut value", 0, 0, 0),
            ("true", 0, 0, 0),
            ("none", 0, 0, 0),
            ("?mut value", 1, 0, 0),
            ("box(mut value)", 1, 0, 0),
            ("Point { x = mut value, .. }", 0, 2, 1),
            ("[first, ..]", 0, 2, 1),
        ];

        for (source_text, nested_count, entry_count, separator_count) in cases {
            let sources = source_store([source_text]);
            let snapshot = source(&sources, 0);

            let mut parser = Parser::new(snapshot);
            let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::EndOfFileToken);

            let pattern = parser.parse_irrefutable_pattern_until(&mut boundary);
            let diagnostics = parser.finish();

            assert_eq!(pattern.full_text(), source_text);
            assert_eq!(pattern.irrefutable_patterns().count(), nested_count);
            assert_eq!(pattern.irrefutable_pattern_entries().count(), entry_count);
            assert_eq!(pattern.separator_tokens().count(), separator_count);

            assert!(diagnostics.is_empty(), "{source_text}: {diagnostics:?}");
        }
    }

    #[test]
    fn parser_parses_case_pattern_alternatives_and_payload_entries() {
        let sources = source_store(["Some(value) | none | .Missing"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::EndOfFileToken);

        let pattern = parser.parse_case_pattern_until(&mut boundary);
        let diagnostics = parser.finish();

        assert_eq!(pattern.full_text(), "Some(value) | none | .Missing");
        assert_eq!(pattern.case_patterns().count(), 3);
        assert_eq!(pattern.alternative_separator_tokens().count(), 2);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_parses_case_pattern_alternatives_inside_grouped_patterns() {
        let sources = source_store(["(Some(value) | none)"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::EndOfFileToken);

        let pattern = parser.parse_case_pattern_until(&mut boundary);
        let diagnostics = parser.finish();
        let nested = pattern.case_patterns().collect::<Vec<_>>();

        let [inner] = nested.as_slice() else {
            panic!("expected grouped case pattern child: {nested:?}");
        };

        assert_eq!(pattern.full_text(), "(Some(value) | none)");
        assert_eq!(inner.alternative_separator_tokens().count(), 1);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_recovers_bad_pattern_entries_without_losing_later_entries() {
        let sources = source_store(["[first, $, second]"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::EndOfFileToken);

        let pattern = parser.parse_irrefutable_pattern_until(&mut boundary);
        let diagnostics = parser.finish();

        assert_eq!(pattern.full_text(), "[first, $, second]");
        assert_eq!(pattern.irrefutable_pattern_entries().count(), 3);
        assert_eq!(pattern.skipped_syntax().count(), 1);

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );
    }

    #[test]
    fn parser_pattern_entry_recovery_stops_at_hard_boundaries() {
        let sources = source_store(["[first | second]"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::EndOfFileToken);

        let pattern = parser.parse_irrefutable_pattern_until(&mut boundary);

        assert_eq!(pattern.full_text(), "[first ");
        assert!(parser.at(SyntaxKind::PipeToken));

        let diagnostics = parser.finish();

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxExpectedToken]
        );
    }
}
