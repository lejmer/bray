use bray_diagnostics::{Diagnostic, DiagnosticBag};
use bray_source::SourceSnapshot;
use bray_syntax::{SyntaxKind, SyntaxToken};

use crate::cursor::{ParserCursor, ParserCursorCheckpoint};
use crate::diagnostic;
use crate::lexer::LexerTokenSource;

pub(super) const MAX_SYNTAX_NESTING_DEPTH: usize = 128;

pub(super) struct Parser {
    snapshot: SourceSnapshot,
    cursor: ParserCursor,
    syntax_nesting_depth: usize,
    syntax_nesting_limit_reported: bool,
}

impl Parser {
    pub(super) fn new(snapshot: SourceSnapshot) -> Self {
        // LexerTokenSource owns a snapshot handle, cloning shares immutable source text.
        let token_source = LexerTokenSource::new(snapshot.clone());

        Self {
            snapshot,
            cursor: ParserCursor::new(token_source),
            syntax_nesting_depth: 0,
            syntax_nesting_limit_reported: false,
        }
    }

    pub(super) fn finish(self) -> DiagnosticBag {
        self.cursor.finish()
    }

    /// Runs a syntax-only lookahead decision on a forked parser.
    pub(super) fn scan_ahead(&mut self, scan: impl FnOnce(&mut Parser) -> bool) -> bool {
        let checkpoint = self.cursor.checkpoint();

        let mut fork = self.fork_from_checkpoint(&checkpoint);

        let decision = scan(&mut fork);

        self.cursor.absorb_lexical_diagnostics_from(&fork.cursor);

        decision
    }

    fn fork_from_checkpoint(&self, checkpoint: &ParserCursorCheckpoint) -> Self {
        Self {
            // Forked parsers share immutable source text with the main parser.
            snapshot: self.snapshot.clone(),
            cursor: checkpoint.fork(),
            syntax_nesting_depth: self.syntax_nesting_depth,
            syntax_nesting_limit_reported: self.syntax_nesting_limit_reported,
        }
    }

    pub(super) fn try_enter_syntax_nesting(&mut self) -> bool {
        if self.syntax_nesting_depth >= MAX_SYNTAX_NESTING_DEPTH {
            if !self.syntax_nesting_limit_reported {
                let actual = self.peek();
                let source = self.syntax_source();

                self.record_syntax_diagnostic(diagnostic::nesting_limit_exceeded(
                    &source,
                    &actual,
                    MAX_SYNTAX_NESTING_DEPTH,
                ));

                self.syntax_nesting_limit_reported = true;
            }

            return false;
        }

        self.syntax_nesting_depth += 1;

        true
    }

    pub(super) fn leave_syntax_nesting(&mut self) {
        self.syntax_nesting_depth = self
            .syntax_nesting_depth
            .checked_sub(1)
            .unwrap_or_else(|| panic!("parser syntax nesting depth became unbalanced"));

        if self.syntax_nesting_depth == 0 {
            self.syntax_nesting_limit_reported = false;
        }
    }

    pub(super) fn syntax_source(&self) -> SourceSnapshot {
        // Syntax nodes share immutable source text by cloning the snapshot handle.
        self.snapshot.clone()
    }

    pub(super) fn lookahead(&mut self, distance: usize) -> SyntaxToken {
        self.cursor.lookahead(distance)
    }

    pub(super) fn peek(&mut self) -> SyntaxToken {
        self.lookahead(0)
    }

    pub(super) fn at(&mut self, kind: SyntaxKind) -> bool {
        self.peek().kind() == kind
    }

    pub(super) fn at_any(&mut self, kinds: &[SyntaxKind]) -> bool {
        let kind = self.peek().kind();

        kinds.contains(&kind)
    }

    pub(super) fn consume_if(&mut self, kind: SyntaxKind) -> Option<SyntaxToken> {
        if !self.at(kind) {
            return None;
        }

        Some(self.cursor.consume())
    }

    pub(super) fn consume(&mut self) -> SyntaxToken {
        let kind = self.peek().kind();

        match self.consume_if(kind) {
            Some(token) => token,
            None => panic!("parser token changed between peek and consume"),
        }
    }

    pub(super) fn expect(&mut self, kind: SyntaxKind) -> SyntaxToken {
        self.cursor.expect(kind)
    }

    pub(super) fn token_text(&self, token: &SyntaxToken) -> Option<&str> {
        token.text(self.snapshot.text())
    }

    pub(super) fn skip_until(
        &mut self,
        recovery_set: crate::cursor::RecoverySet<'_>,
    ) -> Vec<SyntaxToken> {
        self.cursor.skip_until(recovery_set)
    }

    pub(super) fn skip_current_and_until(
        &mut self,
        recovery_set: crate::cursor::RecoverySet<'_>,
    ) -> Vec<SyntaxToken> {
        self.cursor.skip_current_and_until(recovery_set)
    }

    pub(super) fn skip_one(&mut self) -> Option<SyntaxToken> {
        self.cursor.skip_one()
    }

    pub(super) fn record_syntax_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.cursor.record_syntax_diagnostic(diagnostic);
    }

    pub(super) fn skip_until_balanced_close_brace_or_recovery(
        &mut self,
        recovery_set: crate::cursor::RecoverySet<'_>,
    ) -> Vec<SyntaxToken> {
        self.cursor
            .skip_until_balanced_close_brace_or_recovery(recovery_set)
    }

    pub(super) fn skip_current_and_until_balanced_close_brace_or_recovery(
        &mut self,
        recovery_set: crate::cursor::RecoverySet<'_>,
    ) -> Vec<SyntaxToken> {
        self.cursor
            .skip_current_and_until_balanced_close_brace_or_recovery(recovery_set)
    }

    pub(super) fn scan_until_balanced_close_paren(&mut self, stop_kinds: &[SyntaxKind]) {
        self.cursor
            .skip_until_balanced_close_paren(crate::cursor::RecoverySet::new(stop_kinds));
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::{SourceUnitSyntax, SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use super::{MAX_SYNTAX_NESTING_DEPTH, Parser};
    use crate::test_support::{diagnostic_kinds, source};

    #[test]
    fn parser_scan_ahead_leaves_main_cursor_position_unchanged() {
        let sources = source_store(["func main"]);
        let snapshot = source(&sources, 0);
        let mut parser = Parser::new(snapshot);

        let saw_identifier_after_func = parser.scan_ahead(|scan| {
            assert_eq!(scan.consume().kind(), SyntaxKind::FuncKeyword);

            scan.at(SyntaxKind::IdentifierToken)
        });

        assert!(saw_identifier_after_func);
        assert_eq!(parser.peek().kind(), SyntaxKind::FuncKeyword);
    }

    #[test]
    fn parser_scan_ahead_discards_recovery_nodes_and_syntax_diagnostics() {
        let sources = source_store(["module main;"]);
        let snapshot = source(&sources, 0);
        let mut parser = Parser::new(snapshot.clone());

        let built_skipped_syntax_in_scan = parser.scan_ahead(|scan| {
            let mut builder = SourceUnitSyntax::builder(snapshot.clone());

            scan.recover_until(&mut builder, &[SyntaxKind::SemicolonToken]);

            builder.push_token(scan.expect(SyntaxKind::SemicolonToken));
            builder.push_token(scan.expect(SyntaxKind::EndOfFileToken));

            let speculative_unit = builder.build();

            speculative_unit.skipped_syntax().next().is_some()
        });

        assert!(built_skipped_syntax_in_scan);

        let source_unit = parser.parse_source_unit();
        let diagnostics = parser.finish();

        assert_eq!(source_unit.full_text(), "module main;");
        assert!(source_unit.skipped_syntax().next().is_none());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_scan_ahead_preserves_demanded_lexical_diagnostics() {
        let sources = source_store(["$"]);
        let snapshot = source(&sources, 0);
        let mut parser = Parser::new(snapshot);

        let saw_invalid_token = parser.scan_ahead(|scan| scan.at(SyntaxKind::InvalidToken));

        assert!(saw_invalid_token);

        let diagnostics = parser.finish();

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::LexicalInvalidCharacter]
        );
    }

    #[test]
    fn parser_scan_ahead_discards_parser_diagnostics_from_abandoned_scan() {
        let sources = source_store(["main"]);
        let snapshot = source(&sources, 0);
        let mut parser = Parser::new(snapshot);

        let inserted_missing_token =
            parser.scan_ahead(|scan| scan.expect(SyntaxKind::FuncKeyword).is_missing());

        assert!(inserted_missing_token);
        assert_eq!(parser.peek().kind(), SyntaxKind::IdentifierToken);

        let diagnostics = parser.finish();

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn deeply_nested_expressions_recover_without_exhausting_the_native_stack() {
        let expression_text = format!(
            "{}value",
            "- ".repeat(MAX_SYNTAX_NESTING_DEPTH.saturating_mul(2))
        );

        let sources = source_store([format!("{expression_text};")]);
        let snapshot = source(&sources, 0);
        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);

        assert_eq!(expression.full_text(), expression_text);
        assert_eq!(parser.peek().kind(), SyntaxKind::SemicolonToken);

        let diagnostics = parser.finish();

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxNestingLimitExceeded]
        );
    }

    #[test]
    fn deeply_nested_types_recover_without_losing_source_text() {
        let type_text = format!(
            "{}Value",
            "& ".repeat(MAX_SYNTAX_NESTING_DEPTH.saturating_mul(2))
        );

        let sources = source_store([type_text.as_str()]);
        let snapshot = source(&sources, 0);
        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::EndOfFileToken);

        let ty = parser.parse_type_expression_until(&mut boundary);

        assert_eq!(ty.full_text(), type_text);
        assert_eq!(parser.peek().kind(), SyntaxKind::EndOfFileToken);

        let diagnostics = parser.finish();

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxNestingLimitExceeded]
        );
    }

    #[test]
    fn deeply_nested_patterns_recover_without_losing_source_text() {
        let pattern_text = format!(
            "{}value",
            "? ".repeat(MAX_SYNTAX_NESTING_DEPTH.saturating_mul(2))
        );

        let sources = source_store([format!("{pattern_text};")]);
        let snapshot = source(&sources, 0);
        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let pattern = parser.parse_case_pattern_until(&mut boundary);

        assert_eq!(pattern.full_text(), pattern_text);
        assert_eq!(parser.peek().kind(), SyntaxKind::SemicolonToken);

        let diagnostics = parser.finish();

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxNestingLimitExceeded]
        );
    }
}
