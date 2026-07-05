use bray_diagnostics::DiagnosticBag;
use bray_syntax::{SyntaxKind, SyntaxToken};

use crate::diagnostic;
use crate::lexer::LexerTokenSource;

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

    pub(crate) fn consume_if(&mut self, kind: SyntaxKind) -> Option<SyntaxToken> {
        if !self.at(kind) {
            return None;
        }

        Some(self.consume())
    }

    pub(crate) fn expect(&mut self, kind: SyntaxKind) -> Option<SyntaxToken> {
        let token = self.peek();

        if token.kind() == kind {
            return Some(self.consume());
        }

        let diagnostic = if token.is_end_of_file() {
            diagnostic::unexpected_eof(self.token_source.source(), kind, &token)
        } else {
            diagnostic::expected_token(self.token_source.source(), kind, &token)
        };

        self.record_syntax_diagnostic(diagnostic);

        None
    }

    pub(crate) fn finish(self) -> DiagnosticBag {
        self.token_source
            .into_diagnostics()
            .merged(&self.syntax_diagnostics)
    }

    fn record_syntax_diagnostic(&mut self, diagnostic: bray_diagnostics::Diagnostic) {
        self.syntax_diagnostics = self
            .syntax_diagnostics
            .merged(&DiagnosticBag::single(diagnostic));
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticArg, DiagnosticKind};
    use bray_source::TextSize;
    use bray_syntax::SyntaxKind;
    use bray_testing::test_source_snapshot as snapshot;

    use super::ParserCursor;
    use crate::lexer::LexerTokenSource;

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

        assert!(cursor.expect(SyntaxKind::FuncKeyword).is_none());
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
            Some(bray_source::TextRange::empty(TextSize::ZERO))
        );
    }

    #[test]
    fn cursor_expect_emits_unexpected_eof_diagnostics() {
        let mut cursor = cursor("");

        assert!(cursor.expect(SyntaxKind::FuncKeyword).is_none());

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
            Some(bray_source::TextRange::empty(TextSize::ZERO))
        );

        assert_eq!(
            diagnostic.args(),
            &[
                DiagnosticArg::expected_syntax_kind(SyntaxKind::FuncKeyword),
                DiagnosticArg::actual_syntax_kind(SyntaxKind::EndOfFileToken),
            ]
        );
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

        assert!(cursor.expect(SyntaxKind::FuncKeyword).is_none());

        let diagnostics = cursor.finish();

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxExpectedToken
            ]
        );
    }

    fn cursor(text: &str) -> ParserCursor {
        ParserCursor::new(LexerTokenSource::new(snapshot(text)))
    }

    fn diagnostic_kinds(diagnostics: &bray_diagnostics::DiagnosticBag) -> Vec<DiagnosticKind> {
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.kind())
            .collect()
    }
}
