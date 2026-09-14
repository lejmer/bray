use bray_diagnostics::DiagnosticBag;
use bray_syntax::{SyntaxKind, SyntaxToken};

use crate::diagnostic;
use crate::lexer::LexerTokenSource;

/// Parser recovery synchronization set.
///
/// A cursor skip stops before any token whose kind is in the set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RecoverySet<'kinds> {
    primary_stop_kinds: &'kinds [SyntaxKind],
    additional_stop_kinds: &'kinds [SyntaxKind],
}

impl<'kinds> RecoverySet<'kinds> {
    pub(crate) const fn new(stop_kinds: &'kinds [SyntaxKind]) -> Self {
        Self {
            primary_stop_kinds: stop_kinds,
            additional_stop_kinds: &[],
        }
    }

    pub(crate) const fn with_additional(self, additional_stop_kinds: &'kinds [SyntaxKind]) -> Self {
        Self {
            primary_stop_kinds: self.primary_stop_kinds,
            additional_stop_kinds,
        }
    }

    fn contains(self, kind: SyntaxKind) -> bool {
        self.primary_stop_kinds.contains(&kind) || self.additional_stop_kinds.contains(&kind)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DelimiterPair {
    open: SyntaxKind,
    close: SyntaxKind,
}

impl DelimiterPair {
    const fn new(open: SyntaxKind, close: SyntaxKind) -> Self {
        Self { open, close }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ParserCursorCheckpoint {
    cursor: ParserCursor,
}

impl ParserCursorCheckpoint {
    pub(crate) fn fork(&self) -> ParserCursor {
        self.cursor.clone()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ParserCursor {
    token_source: LexerTokenSource,
    syntax_diagnostics: DiagnosticBag,
}

impl ParserCursor {
    pub(crate) fn new(token_source: LexerTokenSource) -> Self {
        Self {
            token_source,
            syntax_diagnostics: DiagnosticBag::new(),
        }
    }

    pub(crate) fn checkpoint(&self) -> ParserCursorCheckpoint {
        ParserCursorCheckpoint {
            cursor: self.clone(),
        }
    }

    pub(crate) fn peek(&mut self) -> SyntaxToken {
        self.lookahead(0)
    }

    pub(crate) fn lookahead(&mut self, distance: usize) -> SyntaxToken {
        self.token_source.lookahead(distance)
    }

    pub(crate) fn at(&mut self, kind: SyntaxKind) -> bool {
        self.peek().kind() == kind
    }

    pub(crate) fn consume(&mut self) -> SyntaxToken {
        self.token_source.consume()
    }

    pub(crate) fn consume_tuple_element_index_after_dot(&mut self) -> SyntaxToken {
        self.token_source.consume_tuple_element_index_after_dot()
    }

    pub(crate) fn consume_type_punctuation(&mut self) -> SyntaxToken {
        self.token_source.consume_type_punctuation()
    }

    pub(crate) fn at_generic_close(&self) -> bool {
        self.token_source.at_generic_close()
    }

    pub(crate) fn consume_if(&mut self, kind: SyntaxKind) -> Option<SyntaxToken> {
        if !self.at(kind) {
            return None;
        }

        Some(self.consume())
    }

    pub(crate) fn expect(&mut self, kind: SyntaxKind) -> SyntaxToken {
        let token = self.peek();

        if token.kind() == kind {
            return self.consume();
        }

        let diagnostic = if token.is_end_of_file() {
            diagnostic::unexpected_eof(self.token_source.source(), kind, &token)
        } else {
            diagnostic::expected_token(self.token_source.source(), kind, &token)
        };

        self.record_syntax_diagnostic(diagnostic);

        SyntaxToken::missing(kind, token.start())
    }

    /// Consumes tokens until the cursor reaches a recovery token or EOF.
    ///
    /// The recovery token is left unconsumed so the caller can use it for a
    /// named syntax slot. Consumed tokens are returned for attachment under a
    /// skipped-syntax node.
    pub(crate) fn skip_until(&mut self, recovery_set: RecoverySet<'_>) -> Vec<SyntaxToken> {
        self.skip_until_tokens(recovery_set)
    }

    /// Consumes one non-EOF token and then consumes until a recovery token or EOF.
    ///
    /// The recovery token is left unconsumed so the caller can use it for a
    /// named syntax slot. Consumed tokens are returned for attachment under a
    /// skipped-syntax node.
    pub(crate) fn skip_current_and_until(
        &mut self,
        recovery_set: RecoverySet<'_>,
    ) -> Vec<SyntaxToken> {
        let mut skipped_tokens = Vec::new();

        self.consume_skipped_token_into(&mut skipped_tokens);
        skipped_tokens.extend(self.skip_until_tokens(recovery_set));

        skipped_tokens
    }

    /// Consumes one non-EOF token for attachment as skipped syntax.
    pub(crate) fn skip_one(&mut self) -> Option<SyntaxToken> {
        let mut skipped_tokens = Vec::new();

        self.consume_skipped_token_into(&mut skipped_tokens);

        skipped_tokens.into_iter().next()
    }

    pub(crate) fn skip_until_balanced_close_paren(
        &mut self,
        recovery_set: RecoverySet<'_>,
    ) -> Vec<SyntaxToken> {
        self.skip_until_balanced_close(
            DelimiterPair::new(SyntaxKind::OpenParenToken, SyntaxKind::CloseParenToken),
            recovery_set,
            0,
        )
    }

    /// Consumes tokens until an unmatched close brace, recovery token, or EOF.
    ///
    /// The close brace or recovery token at depth zero is left unconsumed so
    /// the caller can use it for a named syntax slot or recovery boundary.
    pub(crate) fn skip_until_balanced_close_brace_or_recovery(
        &mut self,
        recovery_set: RecoverySet<'_>,
    ) -> Vec<SyntaxToken> {
        self.skip_until_balanced_close(
            DelimiterPair::new(SyntaxKind::OpenBraceToken, SyntaxKind::CloseBraceToken),
            recovery_set,
            0,
        )
    }

    pub(crate) fn skip_current_and_until_balanced_close_brace_or_recovery(
        &mut self,
        recovery_set: RecoverySet<'_>,
    ) -> Vec<SyntaxToken> {
        let delimiters =
            DelimiterPair::new(SyntaxKind::OpenBraceToken, SyntaxKind::CloseBraceToken);

        let mut skipped_tokens = Vec::new();
        let mut depth = 0usize;

        if let Some(skipped_token) = self.consume_skipped_token() {
            if skipped_token.kind() == delimiters.open {
                depth += 1;
            }

            skipped_tokens.push(skipped_token);
        }

        skipped_tokens.extend(self.skip_until_balanced_close(delimiters, recovery_set, depth));

        skipped_tokens
    }

    fn skip_until_balanced_close(
        &mut self,
        delimiters: DelimiterPair,
        recovery_set: RecoverySet<'_>,
        initial_depth: usize,
    ) -> Vec<SyntaxToken> {
        let mut skipped_tokens = Vec::new();
        let mut depth = initial_depth;

        loop {
            let token = self.peek();

            if token.is_end_of_file()
                || (token.kind() == delimiters.close && depth == 0)
                || (depth == 0 && recovery_set.contains(token.kind()))
            {
                break;
            }

            let Some(skipped_token) = self.consume_skipped_token() else {
                break;
            };

            match skipped_token.kind() {
                kind if kind == delimiters.open => depth += 1,
                kind if kind == delimiters.close => depth = depth.saturating_sub(1),
                _ => {}
            }

            skipped_tokens.push(skipped_token);
        }

        skipped_tokens
    }

    pub(crate) fn finish(self) -> DiagnosticBag {
        self.token_source
            .into_diagnostics()
            .merged(&self.syntax_diagnostics)
    }

    pub(crate) fn absorb_lexical_diagnostics_from(&mut self, cursor: &Self) {
        self.token_source
            .merge_diagnostics_from(cursor.token_source.diagnostics());
    }

    pub(crate) fn record_syntax_diagnostic(&mut self, diagnostic: bray_diagnostics::Diagnostic) {
        self.syntax_diagnostics = self
            .syntax_diagnostics
            .merged(&DiagnosticBag::single(diagnostic));
    }

    fn skip_until_tokens(&mut self, recovery_set: RecoverySet<'_>) -> Vec<SyntaxToken> {
        let mut skipped_tokens = Vec::new();

        loop {
            let token = self.peek();

            if recovery_set.contains(token.kind()) || token.is_end_of_file() {
                break;
            }

            self.consume_skipped_token_into(&mut skipped_tokens);
        }

        skipped_tokens
    }

    fn consume_skipped_token_into(&mut self, skipped_tokens: &mut Vec<SyntaxToken>) {
        let Some(skipped_token) = self.consume_skipped_token() else {
            return;
        };

        skipped_tokens.push(skipped_token);
    }

    fn consume_skipped_token(&mut self) -> Option<SyntaxToken> {
        let token = self.peek();

        if token.is_end_of_file() {
            return None;
        }

        match self.consume_if(token.kind()) {
            Some(token) => Some(token),
            None => panic!("parser cursor token changed between peek and consume"),
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticArg, DiagnosticKind, DiagnosticSuggestionApplicability};
    use bray_source::{TextRange, TextSize};
    use bray_syntax::SyntaxKind;
    use bray_testing::{assert_goal_state_diagnostic_kind, test_source_snapshot as snapshot};

    use super::{ParserCursor, RecoverySet};
    use crate::lexer::LexerTokenSource;
    use crate::test_support::{diagnostic_kinds, token_kinds};

    #[test]
    fn cursor_peeks_and_lookahead_are_stable_without_consuming() {
        let mut cursor = cursor("func main");

        let first = cursor.peek();
        let first_again = cursor.lookahead(0);
        let second = cursor.lookahead(1);

        assert_eq!(first.kind(), SyntaxKind::FuncKeyword);
        assert_eq!(first_again, first);
        assert_eq!(second.kind(), SyntaxKind::IdentifierToken);

        let consumed = cursor.consume();

        assert_eq!(consumed, first);
        assert_eq!(cursor.peek(), second);
    }

    #[test]
    fn cursor_consume_if_consumes_only_matching_tokens() {
        let mut cursor = cursor("func main");

        assert!(cursor.consume_if(SyntaxKind::IdentifierToken).is_none());

        let token = match cursor.consume_if(SyntaxKind::FuncKeyword) {
            Some(token) => token,
            None => panic!("func keyword should be consumed"),
        };

        assert_eq!(token.kind(), SyntaxKind::FuncKeyword);
        assert_eq!(cursor.peek().kind(), SyntaxKind::IdentifierToken);
    }

    #[test]
    fn cursor_expect_emits_missing_token_diagnostics_without_consuming_actual_token() {
        let mut cursor = cursor("main");

        let expected = cursor.expect(SyntaxKind::FuncKeyword);

        assert_eq!(expected.kind(), SyntaxKind::FuncKeyword);
        assert!(expected.is_missing());

        assert_eq!(expected.range(), TextRange::empty(TextSize::ZERO));

        assert_eq!(cursor.peek().kind(), SyntaxKind::IdentifierToken);

        let diagnostics = cursor.finish();

        let diagnostic = match diagnostics
            .by_kind(DiagnosticKind::SyntaxExpectedToken)
            .next()
        {
            Some(diagnostic) => diagnostic,
            None => panic!("expected syntax diagnostic should be present"),
        };

        assert_eq!(
            diagnostic.primary_span().map(|span| span.range()),
            Some(TextRange::empty(TextSize::ZERO))
        );

        let [suggestion] = diagnostic.suggestions() else {
            panic!("fixed expected syntax should provide one plausible edit");
        };

        assert_eq!(
            suggestion.applicability(),
            DiagnosticSuggestionApplicability::MaybeApplicable
        );

        assert_goal_state_diagnostic_kind(&diagnostics, DiagnosticKind::SyntaxExpectedToken);
    }

    #[test]
    fn cursor_expect_emits_unexpected_eof_diagnostics() {
        let mut cursor = cursor("");

        let expected = cursor.expect(SyntaxKind::FuncKeyword);

        assert_eq!(expected.kind(), SyntaxKind::FuncKeyword);
        assert!(expected.is_missing());

        let diagnostics = cursor.finish();

        let diagnostic = match diagnostics
            .by_kind(DiagnosticKind::SyntaxUnexpectedEof)
            .next()
        {
            Some(diagnostic) => diagnostic,
            None => panic!("unexpected EOF diagnostic should be present"),
        };

        assert_eq!(
            diagnostic.primary_span().map(|span| span.range()),
            Some(TextRange::empty(TextSize::ZERO))
        );

        assert_eq!(
            diagnostic.args(),
            &[
                DiagnosticArg::expected_syntax_kind(SyntaxKind::FuncKeyword),
                DiagnosticArg::actual_syntax_kind(SyntaxKind::EndOfFileToken),
            ]
        );

        let [suggestion] = diagnostic.suggestions() else {
            panic!("fixed EOF syntax should provide one plausible edit");
        };

        assert_eq!(
            suggestion.applicability(),
            DiagnosticSuggestionApplicability::MaybeApplicable
        );

        assert_goal_state_diagnostic_kind(&diagnostics, DiagnosticKind::SyntaxUnexpectedEof);
    }

    #[test]
    fn cursor_non_fixed_expected_syntax_keeps_context_without_fabricating_an_edit() {
        let mut cursor = cursor("func");

        let expected = cursor.expect(SyntaxKind::IdentifierToken);

        assert!(expected.is_missing());

        let diagnostics = cursor.finish();

        let diagnostic = diagnostics
            .by_kind(DiagnosticKind::SyntaxExpectedToken)
            .next()
            .unwrap_or_else(|| panic!("non-fixed expected syntax must be diagnosed"));

        assert!(diagnostic.suggestions().is_empty());

        assert_goal_state_diagnostic_kind(&diagnostics, DiagnosticKind::SyntaxExpectedToken);
    }

    #[test]
    fn cursor_non_fixed_eof_keeps_context_without_fabricating_an_edit() {
        let mut cursor = cursor("");

        let expected = cursor.expect(SyntaxKind::IdentifierToken);

        assert!(expected.is_missing());

        let diagnostics = cursor.finish();

        let diagnostic = diagnostics
            .by_kind(DiagnosticKind::SyntaxUnexpectedEof)
            .next()
            .unwrap_or_else(|| panic!("non-fixed EOF syntax must be diagnosed"));

        assert!(diagnostic.suggestions().is_empty());

        assert_goal_state_diagnostic_kind(&diagnostics, DiagnosticKind::SyntaxUnexpectedEof);
    }

    #[test]
    fn cursor_repeated_eof_peeks_and_consumes_are_safe() {
        let mut cursor = cursor("");

        let first_peek = cursor.peek();
        let second_peek = cursor.lookahead(4);
        let first_consume = cursor.consume();
        let second_consume = cursor.consume();

        assert_eq!(first_peek.kind(), SyntaxKind::EndOfFileToken);
        assert_eq!(second_peek.kind(), SyntaxKind::EndOfFileToken);
        assert_eq!(first_consume.kind(), SyntaxKind::EndOfFileToken);
        assert_eq!(second_consume.kind(), SyntaxKind::EndOfFileToken);
        assert_eq!(first_peek.range(), second_consume.range());
    }

    #[test]
    fn cursor_finish_preserves_lexical_diagnostics_from_demanded_tokens() {
        let mut cursor = cursor("$");

        assert_eq!(cursor.peek().kind(), SyntaxKind::InvalidToken);

        let diagnostics = cursor.finish();

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::LexicalInvalidCharacter]
        );
    }

    #[test]
    fn cursor_finish_merges_lexical_and_syntax_diagnostics_deterministically() {
        let mut cursor = cursor("$");

        let expected = cursor.expect(SyntaxKind::FuncKeyword);

        assert!(expected.is_missing());

        let diagnostics = cursor.finish();

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxExpectedToken
            ]
        );
    }

    #[test]
    fn cursor_skip_until_consumes_until_any_recovery_token() {
        let mut cursor = cursor("main }");

        let skipped = cursor.skip_until(RecoverySet::new(&[
            SyntaxKind::SemicolonToken,
            SyntaxKind::CloseBraceToken,
        ]));

        assert_eq!(token_kinds(&skipped), [SyntaxKind::IdentifierToken]);
        assert_eq!(cursor.peek().kind(), SyntaxKind::CloseBraceToken);

        let diagnostics = cursor.finish();

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn cursor_skip_until_supports_composed_recovery_sets() {
        let mut cursor = cursor("main; tail");

        let skipped = cursor.skip_until(
            RecoverySet::new(&[SyntaxKind::CommaToken])
                .with_additional(&[SyntaxKind::SemicolonToken]),
        );

        assert_eq!(token_kinds(&skipped), [SyntaxKind::IdentifierToken]);
        assert_eq!(cursor.peek().kind(), SyntaxKind::SemicolonToken);
    }

    #[test]
    fn cursor_skip_until_supports_common_recovery_sets() {
        let mut comma = cursor("main, tail");
        let mut close_paren = cursor("main)");
        let mut semicolon = cursor("main;");

        assert_eq!(
            token_kinds(comma.skip_until(RecoverySet::new(&[SyntaxKind::CommaToken]))),
            [SyntaxKind::IdentifierToken]
        );

        assert_eq!(comma.peek().kind(), SyntaxKind::CommaToken);

        assert_eq!(
            token_kinds(close_paren.skip_until(RecoverySet::new(&[SyntaxKind::CloseParenToken]))),
            [SyntaxKind::IdentifierToken]
        );

        assert_eq!(close_paren.peek().kind(), SyntaxKind::CloseParenToken);

        assert_eq!(
            token_kinds(semicolon.skip_until(RecoverySet::new(&[SyntaxKind::SemicolonToken]))),
            [SyntaxKind::IdentifierToken]
        );

        assert_eq!(semicolon.peek().kind(), SyntaxKind::SemicolonToken);
    }

    #[test]
    fn cursor_skip_until_stops_at_eof_without_consuming_eof() {
        let mut cursor = cursor("main");

        let skipped = cursor.skip_until(RecoverySet::new(&[SyntaxKind::EndOfFileToken]));

        assert_eq!(token_kinds(&skipped), [SyntaxKind::IdentifierToken]);
        assert_eq!(cursor.peek().kind(), SyntaxKind::EndOfFileToken);

        let eof = cursor.consume();

        assert!(eof.is_end_of_file());
        assert_eq!(eof.range(), TextRange::empty(TextSize::new(4)));
    }

    #[test]
    fn cursor_skip_until_balanced_close_brace_leaves_outer_close_brace() {
        let mut cursor = cursor("func run() {} }");

        let skipped = cursor.skip_until_balanced_close_brace_or_recovery(RecoverySet::new(&[]));

        assert_eq!(
            token_kinds(&skipped),
            [
                SyntaxKind::FuncKeyword,
                SyntaxKind::IdentifierToken,
                SyntaxKind::OpenParenToken,
                SyntaxKind::CloseParenToken,
                SyntaxKind::OpenBraceToken,
                SyntaxKind::CloseBraceToken
            ]
        );

        assert_eq!(cursor.peek().kind(), SyntaxKind::CloseBraceToken);

        let diagnostics = cursor.finish();

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn cursor_skip_current_until_balanced_close_brace_consumes_current_item() {
        let mut cursor = cursor("construct() {} func run() {}");

        let skipped =
            cursor.skip_current_and_until_balanced_close_brace_or_recovery(RecoverySet::new(&[
                SyntaxKind::FuncKeyword,
            ]));

        assert_eq!(
            token_kinds(&skipped),
            [
                SyntaxKind::ConstructKeyword,
                SyntaxKind::OpenParenToken,
                SyntaxKind::CloseParenToken,
                SyntaxKind::OpenBraceToken,
                SyntaxKind::CloseBraceToken
            ]
        );

        assert_eq!(cursor.peek().kind(), SyntaxKind::FuncKeyword);

        let diagnostics = cursor.finish();

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn cursor_skip_until_does_not_emit_diagnostic_without_skipped_tokens() {
        let mut cursor = cursor(";");

        let skipped = cursor.skip_until(RecoverySet::new(&[SyntaxKind::SemicolonToken]));

        assert!(skipped.is_empty());
        assert_eq!(cursor.peek().kind(), SyntaxKind::SemicolonToken);
        assert!(cursor.finish().is_empty());
    }

    #[test]
    fn cursor_skip_one_consumes_one_token_without_syntax_diagnostics() {
        let mut cursor = cursor("main tail");

        let skipped = match cursor.skip_one() {
            Some(token) => token,
            None => panic!("expected one skipped token"),
        };

        assert_eq!(skipped.kind(), SyntaxKind::IdentifierToken);
        assert_eq!(cursor.peek().kind(), SyntaxKind::IdentifierToken);

        let diagnostics = cursor.finish();

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn cursor_skip_until_preserves_lexical_diagnostics_from_skipped_tokens() {
        let mut cursor = cursor("$;");

        let skipped = cursor.skip_until(RecoverySet::new(&[SyntaxKind::SemicolonToken]));

        assert_eq!(token_kinds(&skipped), [SyntaxKind::InvalidToken]);

        let diagnostics = cursor.finish();

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::LexicalInvalidCharacter]
        );
    }

    #[test]
    fn cursor_checkpoint_fork_does_not_advance_main_cursor() {
        let mut cursor = cursor("func main");

        let checkpoint = cursor.checkpoint();

        let mut fork = checkpoint.fork();

        assert_eq!(fork.consume().kind(), SyntaxKind::FuncKeyword);
        assert_eq!(fork.peek().kind(), SyntaxKind::IdentifierToken);

        assert_eq!(cursor.peek().kind(), SyntaxKind::FuncKeyword);
    }

    #[test]
    fn cursor_abandoned_fork_discards_syntax_diagnostics() {
        let mut cursor = cursor("main");

        let checkpoint = cursor.checkpoint();

        let mut fork = checkpoint.fork();

        let token = fork.expect(SyntaxKind::FuncKeyword);

        assert!(token.is_missing());

        cursor.absorb_lexical_diagnostics_from(&fork);

        assert!(cursor.finish().is_empty());
    }

    #[test]
    fn cursor_abandoned_fork_preserves_demanded_lexical_diagnostics() {
        let mut cursor = cursor("$");

        let checkpoint = cursor.checkpoint();

        let mut fork = checkpoint.fork();

        assert_eq!(fork.peek().kind(), SyntaxKind::InvalidToken);

        cursor.absorb_lexical_diagnostics_from(&fork);

        let diagnostics = cursor.finish();

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::LexicalInvalidCharacter]
        );
    }

    #[test]
    fn cursor_absorbed_fork_lexical_diagnostics_are_deduplicated() {
        let mut cursor = cursor("$");

        let checkpoint = cursor.checkpoint();

        let mut fork = checkpoint.fork();

        assert_eq!(fork.peek().kind(), SyntaxKind::InvalidToken);

        cursor.absorb_lexical_diagnostics_from(&fork);

        assert_eq!(cursor.peek().kind(), SyntaxKind::InvalidToken);

        let diagnostics = cursor.finish();

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::LexicalInvalidCharacter]
        );
    }

    fn cursor(text: &str) -> ParserCursor {
        ParserCursor::new(LexerTokenSource::new(snapshot(text)))
    }
}
