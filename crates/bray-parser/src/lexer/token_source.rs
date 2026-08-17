use bray_diagnostics::DiagnosticBag;
use bray_source::{SourceSnapshot, TextSize};
use bray_syntax::SyntaxToken;

use super::scanner::{LexerScanMode, scan_token_at};

/// Controls whether a lexer token source stores tokens produced for lookahead.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum LexerCachePolicy {
    /// Store produced lookahead tokens for repeated parser access.
    #[default]
    CacheTokens,
    /// Re-scan from the current cursor for every lookahead request.
    DoNotCacheTokens,
}

/// Lazy lexical token source over an immutable source snapshot.
///
/// The lexer produces tokens on demand and may cache tokens produced for
/// lookahead. Whitespace and comments are preserved as token trivia. Lexical
/// diagnostics are accumulated as tokens are scanned, including tokens scanned
/// for lookahead.
#[derive(Clone, Debug)]
pub struct LexerTokenSource {
    snapshot: SourceSnapshot,
    cursor: TextSize,
    cache_policy: LexerCachePolicy,
    cached_tokens: Vec<SyntaxToken>,
    diagnostics: DiagnosticBag,
}

impl LexerTokenSource {
    /// Creates a lazy token source with token caching enabled.
    pub fn new(snapshot: SourceSnapshot) -> Self {
        Self::with_cache_policy(snapshot, LexerCachePolicy::default())
    }

    /// Creates a lazy token source with an explicit cache policy.
    pub fn with_cache_policy(snapshot: SourceSnapshot, cache_policy: LexerCachePolicy) -> Self {
        Self {
            snapshot,
            cursor: TextSize::ZERO,
            cache_policy,
            cached_tokens: Vec::new(),
            diagnostics: DiagnosticBag::new(),
        }
    }

    /// Returns the source snapshot being lexed.
    pub const fn source(&self) -> &SourceSnapshot {
        &self.snapshot
    }

    /// Returns the current lexer cursor as a UTF-8 byte offset.
    pub const fn current_offset(&self) -> TextSize {
        self.cursor
    }

    /// Returns the current cache policy.
    pub const fn cache_policy(&self) -> LexerCachePolicy {
        self.cache_policy
    }

    /// Returns lexical diagnostics produced by tokens scanned so far.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Consumes the token source and returns lexical diagnostics produced so far.
    pub fn into_diagnostics(self) -> DiagnosticBag {
        self.diagnostics
    }

    pub(crate) fn merge_diagnostics_from(&mut self, diagnostics: &DiagnosticBag) {
        self.record_diagnostics(diagnostics);
    }

    /// Returns the next token without consuming it.
    pub fn peek(&mut self) -> SyntaxToken {
        self.lookahead(0)
    }

    /// Returns the token `distance` positions after the current cursor.
    ///
    /// `distance == 0` is the same token returned by [`peek`](Self::peek).
    /// Lookahead past the end of the source returns the stable EOF token.
    pub fn lookahead(&mut self, distance: usize) -> SyntaxToken {
        match self.cache_policy {
            LexerCachePolicy::CacheTokens => self.cached_lookahead(distance),
            LexerCachePolicy::DoNotCacheTokens => self.uncached_lookahead(distance),
        }
    }

    /// Returns a window of upcoming tokens without consuming them.
    ///
    /// The returned vector has exactly `len` tokens. If the requested window
    /// reaches past the end of source, EOF tokens fill the remaining entries.
    pub fn lookahead_window(&mut self, len: usize) -> Vec<SyntaxToken> {
        if len == 0 {
            return Vec::new();
        }

        match self.cache_policy {
            LexerCachePolicy::CacheTokens => self.cached_lookahead_window(len),
            LexerCachePolicy::DoNotCacheTokens => self.uncached_lookahead_window(len),
        }
    }

    /// Consumes and returns the next token.
    ///
    /// Consuming EOF is idempotent: the cursor remains at the source end and
    /// later consumes return EOF again.
    pub fn consume(&mut self) -> SyntaxToken {
        let token = self.lookahead(0);

        self.advance_after_consuming(&token);

        token
    }

    /// Consumes the next token using the parser-selected tuple-index scan mode.
    ///
    /// This is used after a member-access dot, where the lexical grammar treats
    /// decimal digits as tuple element indices before ordinary numeric-literal
    /// scanning.
    pub fn consume_tuple_element_index_after_dot(&mut self) -> SyntaxToken {
        self.cached_tokens.clear();

        let token = self.scan_current_token(LexerScanMode::TupleElementIndexAfterDot);

        self.advance_after_consuming_uncached(&token);

        token
    }

    pub(crate) fn consume_generic_close(&mut self) -> SyntaxToken {
        self.cached_tokens.clear();

        let token = self.scan_current_token(LexerScanMode::GenericClose);

        self.advance_after_consuming_uncached(&token);

        token
    }

    pub(crate) fn at_generic_close(&self) -> bool {
        scan_token_at(&self.snapshot, self.cursor, LexerScanMode::GenericClose)
            .into_token()
            .kind()
            == bray_syntax::SyntaxKind::GreaterToken
    }

    fn cached_lookahead(&mut self, distance: usize) -> SyntaxToken {
        self.ensure_cached(distance);

        match self.cached_tokens.get(distance) {
            Some(token) => share_token(token),
            None => panic!("lexer cache did not contain requested lookahead token"),
        }
    }

    fn uncached_lookahead(&mut self, distance: usize) -> SyntaxToken {
        let mut offset = self.cursor;
        let mut token = self.scan_token_at(offset, LexerScanMode::Normal);

        for _ in 0..distance {
            if is_eof(&token) {
                return token;
            }

            offset = token.full_range().end();
            token = self.scan_token_at(offset, LexerScanMode::Normal);
        }

        token
    }

    fn cached_lookahead_window(&mut self, len: usize) -> Vec<SyntaxToken> {
        self.ensure_cached(len - 1);

        self.cached_tokens
            .iter()
            .take(len)
            .map(share_token)
            .collect()
    }

    fn uncached_lookahead_window(&mut self, len: usize) -> Vec<SyntaxToken> {
        let mut tokens = Vec::with_capacity(len);
        let mut offset = self.cursor;

        while tokens.len() < len {
            let token = self.scan_token_at(offset, LexerScanMode::Normal);
            let reached_eof = is_eof(&token);
            let next_offset = token.full_range().end();

            tokens.push(token);

            if reached_eof {
                fill_with_eof(&mut tokens, len, next_offset);
                break;
            }

            offset = next_offset;
        }

        tokens
    }

    fn ensure_cached(&mut self, distance: usize) {
        while self.cached_tokens.len() <= distance {
            if let Some(eof) = self.cached_tokens.last().filter(|token| is_eof(token)) {
                let eof_offset = eof.start();

                fill_with_eof(&mut self.cached_tokens, distance + 1, eof_offset);
                return;
            }

            let offset = match self.cached_tokens.last() {
                Some(token) => token.full_range().end(),
                None => self.cursor,
            };

            let token = self.scan_token_at(offset, LexerScanMode::Normal);

            self.cached_tokens.push(token);
        }
    }

    fn scan_current_token(&mut self, mode: LexerScanMode) -> SyntaxToken {
        self.scan_token_at(self.cursor, mode)
    }

    fn scan_token_at(&mut self, offset: TextSize, mode: LexerScanMode) -> SyntaxToken {
        let scan = scan_token_at(&self.snapshot, offset, mode);
        self.record_diagnostics(scan.diagnostics());

        scan.into_token()
    }

    fn record_diagnostics(&mut self, diagnostics: &DiagnosticBag) {
        if diagnostics.is_empty() {
            return;
        }

        self.diagnostics = self.diagnostics.merged(diagnostics);
    }

    fn advance_after_consuming(&mut self, token: &SyntaxToken) {
        if is_eof(token) {
            self.cursor = token.full_range().end();
            return;
        }

        self.cursor = token.full_range().end();

        if self.cache_policy == LexerCachePolicy::CacheTokens && !self.cached_tokens.is_empty() {
            self.cached_tokens.remove(0);
        }
    }

    fn advance_after_consuming_uncached(&mut self, token: &SyntaxToken) {
        if is_eof(token) {
            self.cursor = token.full_range().end();
            return;
        }

        self.cursor = token.full_range().end();
    }
}

fn is_eof(token: &SyntaxToken) -> bool {
    token.is_end_of_file()
}

fn fill_with_eof(tokens: &mut Vec<SyntaxToken>, len: usize, offset: TextSize) {
    while tokens.len() < len {
        tokens.push(SyntaxToken::end_of_file(offset));
    }
}

fn share_token(token: &SyntaxToken) -> SyntaxToken {
    // SyntaxToken clones copy compact range/kind data and share immutable trivia storage.
    token.clone()
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_source::{SourceId, SourceIdentity, SourceOrigin, SourceVersion};
    use bray_testing::{assert_goal_state_diagnostics, diagnostics_of_kind};
    use std::ops::Deref;

    use super::{LexerCachePolicy, LexerTokenSource};
    use crate::test_support::{diagnostic_kinds, token_kinds};
    use bray_source::{SourceSnapshot, TextRange, TextSize};
    use bray_syntax::{SyntaxKind, SyntaxToken, SyntaxTrivia};

    #[test]
    fn peek_does_not_consume_the_next_token() {
        let snapshot = snapshot("ab cd");

        let mut source = LexerTokenSource::new(snapshot);

        let token = source.peek();

        assert_eq!(token.kind(), SyntaxKind::IdentifierToken);
        assert_eq!(token_text(source.source().text(), &token), "ab");

        assert_eq!(
            token.range(),
            TextRange::new(TextSize::ZERO, TextSize::new(2))
        );

        assert_eq!(source.current_offset(), TextSize::ZERO);

        let consumed = source.consume();

        assert_eq!(consumed, token);
        assert_eq!(source.current_offset(), TextSize::new(3));
    }

    #[test]
    fn lookahead_windows_are_zero_based_and_do_not_advance() {
        let snapshot = snapshot("a b c");

        let mut source = LexerTokenSource::new(snapshot);

        let first = source.lookahead(0);
        let second = source.lookahead(1);
        let third = source.lookahead(2);

        assert_eq!(token_text(source.source().text(), &first), "a");
        assert_eq!(token_text(source.source().text(), &second), "b");
        assert_eq!(token_text(source.source().text(), &third), "c");

        let window = source.lookahead_window(4);

        let texts: Vec<&str> = window
            .iter()
            .map(|token| token_text(source.source().text(), token))
            .collect();

        assert_eq!(texts, ["a", "b", "c", ""]);
        assert_eq!(window[3].kind(), SyntaxKind::EndOfFileToken);
        assert_eq!(source.current_offset(), TextSize::ZERO);
    }

    #[test]
    fn consume_advances_by_utf8_byte_ranges() {
        let snapshot = snapshot("é+");

        let mut source = LexerTokenSource::new(snapshot);

        let invalid = source.consume();
        let plus = source.consume();
        let eof = source.consume();

        assert_eq!(invalid.kind(), SyntaxKind::InvalidToken);
        assert_eq!(token_text(source.source().text(), &invalid), "é");

        assert_eq!(
            invalid.range(),
            TextRange::new(TextSize::ZERO, TextSize::new(2))
        );

        assert_eq!(
            plus.range(),
            TextRange::new(TextSize::new(2), TextSize::new(3))
        );

        assert_eq!(plus.kind(), SyntaxKind::PlusToken);
        assert_eq!(eof.kind(), SyntaxKind::EndOfFileToken);
        assert_eq!(eof.range(), TextRange::empty(TextSize::new(3)));
        assert_eq!(source.current_offset(), TextSize::new(3));
    }

    #[test]
    fn eof_is_stable_when_peeked_or_consumed_repeatedly() {
        let snapshot = snapshot("");

        let mut source = LexerTokenSource::new(snapshot);

        let peeked = source.peek();
        let consumed = source.consume();
        let consumed_again = source.consume();

        assert_eq!(peeked.kind(), SyntaxKind::EndOfFileToken);
        assert_eq!(consumed.kind(), SyntaxKind::EndOfFileToken);
        assert_eq!(consumed_again.kind(), SyntaxKind::EndOfFileToken);
        assert_eq!(peeked.range(), TextRange::empty(TextSize::ZERO));
        assert_eq!(consumed.range(), peeked.range());
        assert_eq!(consumed_again.range(), peeked.range());
        assert_eq!(source.current_offset(), TextSize::ZERO);
    }

    #[test]
    fn eof_after_skipped_whitespace_advances_to_source_end_when_consumed() {
        let snapshot = snapshot(" \t\r\n");

        let mut source = LexerTokenSource::new(snapshot);

        let eof = source.consume();

        assert_eq!(eof.kind(), SyntaxKind::EndOfFileToken);
        assert_eq!(eof.range(), TextRange::empty(TextSize::new(4)));

        assert_eq!(
            trivia_texts(source.source().text(), eof.leading_trivia()),
            [" \t\r\n"]
        );

        assert_eq!(source.current_offset(), TextSize::new(4));
    }

    #[test]
    fn lookahead_past_eof_returns_eof() {
        let snapshot = snapshot("a");

        let mut source = LexerTokenSource::new(snapshot);

        let token = source.lookahead(5);

        assert_eq!(token.kind(), SyntaxKind::EndOfFileToken);
        assert_eq!(token.range(), TextRange::empty(TextSize::new(1)));
        assert_eq!(source.current_offset(), TextSize::ZERO);
    }

    #[test]
    fn cache_policy_can_disable_token_caching() {
        let snapshot = snapshot("a b");

        let mut source =
            LexerTokenSource::with_cache_policy(snapshot, LexerCachePolicy::DoNotCacheTokens);

        assert_eq!(source.cache_policy(), LexerCachePolicy::DoNotCacheTokens);
        let peeked = source.peek();
        let lookahead = source.lookahead(1);
        let first = source.consume();
        let second = source.consume();

        assert_eq!(token_text(source.source().text(), &peeked), "a");
        assert_eq!(token_text(source.source().text(), &lookahead), "b");
        assert_eq!(token_text(source.source().text(), &first), "a");
        assert_eq!(token_text(source.source().text(), &second), "b");
        assert_eq!(source.consume().kind(), SyntaxKind::EndOfFileToken);
    }

    #[test]
    fn tuple_index_scan_after_dot_has_a_parser_driven_entrypoint() {
        let snapshot = snapshot(".1");

        let mut source = LexerTokenSource::new(snapshot);

        let dot = source.consume();
        let cached_ordinary_next = source.lookahead(0);
        let tuple_index = source.consume_tuple_element_index_after_dot();

        assert_eq!(token_text(source.source().text(), &dot), ".");
        assert_eq!(dot.kind(), SyntaxKind::DotToken);

        assert_eq!(
            token_text(source.source().text(), &cached_ordinary_next),
            "1"
        );

        assert_eq!(
            cached_ordinary_next.kind(),
            SyntaxKind::DecimalIntegerLiteralToken
        );

        assert_eq!(tuple_index.kind(), SyntaxKind::TupleElementIndexToken);
        assert_eq!(token_text(source.source().text(), &tuple_index), "1");

        assert_eq!(
            tuple_index.range(),
            TextRange::new(TextSize::new(1), TextSize::new(2))
        );

        assert_eq!(source.current_offset(), TextSize::new(2));
    }

    #[test]
    fn keywords_win_only_after_scanning_the_full_identifier_text() {
        let tokens = token_stream("if ifx self Self bool");

        assert_eq!(
            token_kinds(&*tokens),
            [
                SyntaxKind::IfKeyword,
                SyntaxKind::IdentifierToken,
                SyntaxKind::SelfValueKeyword,
                SyntaxKind::SelfTypeKeyword,
                SyntaxKind::IdentifierToken,
                SyntaxKind::EndOfFileToken,
            ]
        );

        assert_eq!(
            token_texts(&tokens),
            ["if", "ifx", "self", "Self", "bool", ""]
        );
    }

    #[test]
    fn ascii_identifiers_can_contain_digits_and_underscores_after_the_first_character() {
        let tokens = token_stream("parse_int BufferReader value2");

        assert_eq!(
            token_kinds(&*tokens),
            [
                SyntaxKind::IdentifierToken,
                SyntaxKind::IdentifierToken,
                SyntaxKind::IdentifierToken,
                SyntaxKind::EndOfFileToken,
            ]
        );

        assert_eq!(
            token_texts(&tokens),
            ["parse_int", "BufferReader", "value2", ""]
        );
    }

    #[test]
    fn underscore_is_a_single_token_unless_adjacent_identifier_text_makes_it_invalid() {
        let tokens = token_stream("_ _foo __ _1");

        assert_eq!(
            token_kinds(&*tokens),
            [
                SyntaxKind::UnderscoreToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::EndOfFileToken,
            ]
        );

        assert_eq!(token_texts(&tokens), ["_", "_foo", "__", "_1", ""]);
    }

    #[test]
    fn operator_and_punctuation_tokens_use_longest_matching() {
        let tokens = token_stream(
            "-> == != <= >= && || << >> ** += -= *= /= %= @= &= |= ^= <<= >>= **= .. ( ) { } [ ] , ; : . ? = + - * / % @ & | ^ ~ ! < > _",
        );

        assert_eq!(
            token_kinds(&*tokens),
            [
                SyntaxKind::ArrowToken,
                SyntaxKind::EqualsEqualsToken,
                SyntaxKind::BangEqualsToken,
                SyntaxKind::LessEqualsToken,
                SyntaxKind::GreaterEqualsToken,
                SyntaxKind::AmpersandAmpersandToken,
                SyntaxKind::PipePipeToken,
                SyntaxKind::LessLessToken,
                SyntaxKind::GreaterGreaterToken,
                SyntaxKind::StarStarToken,
                SyntaxKind::PlusEqualsToken,
                SyntaxKind::MinusEqualsToken,
                SyntaxKind::StarEqualsToken,
                SyntaxKind::SlashEqualsToken,
                SyntaxKind::PercentEqualsToken,
                SyntaxKind::AtEqualsToken,
                SyntaxKind::AmpersandEqualsToken,
                SyntaxKind::PipeEqualsToken,
                SyntaxKind::CaretEqualsToken,
                SyntaxKind::LessLessEqualsToken,
                SyntaxKind::GreaterGreaterEqualsToken,
                SyntaxKind::StarStarEqualsToken,
                SyntaxKind::DotDotToken,
                SyntaxKind::OpenParenToken,
                SyntaxKind::CloseParenToken,
                SyntaxKind::OpenBraceToken,
                SyntaxKind::CloseBraceToken,
                SyntaxKind::OpenBracketToken,
                SyntaxKind::CloseBracketToken,
                SyntaxKind::CommaToken,
                SyntaxKind::SemicolonToken,
                SyntaxKind::ColonToken,
                SyntaxKind::DotToken,
                SyntaxKind::QuestionToken,
                SyntaxKind::EqualsToken,
                SyntaxKind::PlusToken,
                SyntaxKind::MinusToken,
                SyntaxKind::StarToken,
                SyntaxKind::SlashToken,
                SyntaxKind::PercentToken,
                SyntaxKind::AtToken,
                SyntaxKind::AmpersandToken,
                SyntaxKind::PipeToken,
                SyntaxKind::CaretToken,
                SyntaxKind::TildeToken,
                SyntaxKind::BangToken,
                SyntaxKind::LessToken,
                SyntaxKind::GreaterToken,
                SyntaxKind::UnderscoreToken,
                SyntaxKind::EndOfFileToken,
            ]
        );
    }

    #[test]
    fn generic_application_close_is_separate_from_member_access() {
        let tokens = token_stream("Argument<u32>.with_options Generic<T>: T");

        assert_eq!(
            token_kinds(&*tokens),
            [
                SyntaxKind::IdentifierToken,
                SyntaxKind::LessToken,
                SyntaxKind::IdentifierToken,
                SyntaxKind::GreaterToken,
                SyntaxKind::DotToken,
                SyntaxKind::IdentifierToken,
                SyntaxKind::IdentifierToken,
                SyntaxKind::LessToken,
                SyntaxKind::IdentifierToken,
                SyntaxKind::GreaterToken,
                SyntaxKind::ColonToken,
                SyntaxKind::IdentifierToken,
                SyntaxKind::EndOfFileToken,
            ]
        );
    }

    #[test]
    fn generic_close_scan_splits_adjacent_closing_angles() {
        let mut source = LexerTokenSource::new(snapshot(">>"));

        assert_eq!(source.peek().kind(), SyntaxKind::GreaterGreaterToken);

        assert_eq!(
            source.consume_generic_close().kind(),
            SyntaxKind::GreaterToken
        );

        assert_eq!(source.peek().kind(), SyntaxKind::GreaterToken);
    }

    #[test]
    fn invalid_adjacent_operator_forms_are_single_invalid_tokens() {
        let tokens = token_stream("=== ->> +- :: ...");

        assert_eq!(
            token_kinds(&*tokens),
            [
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::EndOfFileToken,
            ]
        );

        assert_eq!(token_texts(&tokens), ["===", "->>", "+-", "::", "...", ""]);
    }

    #[test]
    fn non_ascii_identifier_characters_make_identifier_like_text_invalid() {
        let tokens = token_stream("é aé");

        assert_eq!(
            token_kinds(&*tokens),
            [
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::EndOfFileToken,
            ]
        );

        assert_eq!(token_texts(&tokens), ["é", "aé", ""]);

        assert_eq!(
            tokens[1].range(),
            TextRange::new(TextSize::new(3), TextSize::new(6))
        );
    }

    #[test]
    fn numeric_literals_are_classified_by_spelling() {
        let tokens = token_stream("1 0b1010 0xFF 1.25 1e+10 1i 1.25i 0xFi");

        assert_eq!(
            token_kinds(&*tokens),
            [
                SyntaxKind::DecimalIntegerLiteralToken,
                SyntaxKind::BinaryIntegerLiteralToken,
                SyntaxKind::HexadecimalIntegerLiteralToken,
                SyntaxKind::RealLiteralToken,
                SyntaxKind::RealLiteralToken,
                SyntaxKind::ImaginaryLiteralToken,
                SyntaxKind::ImaginaryLiteralToken,
                SyntaxKind::ImaginaryLiteralToken,
                SyntaxKind::EndOfFileToken,
            ]
        );

        assert_eq!(
            token_texts(&tokens),
            [
                "1", "0b1010", "0xFF", "1.25", "1e+10", "1i", "1.25i", "0xFi", ""
            ]
        );
    }

    #[test]
    fn numeric_literal_digit_separators_must_be_between_digits() {
        let tokens = token_stream("1_000 0b1010_0011 0xCAFE_BABE 1_ 1__2 0x_FF");

        assert_eq!(
            token_kinds(&*tokens),
            [
                SyntaxKind::DecimalIntegerLiteralToken,
                SyntaxKind::BinaryIntegerLiteralToken,
                SyntaxKind::HexadecimalIntegerLiteralToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::EndOfFileToken,
            ]
        );

        assert_eq!(
            token_texts(&tokens),
            [
                "1_000",
                "0b1010_0011",
                "0xCAFE_BABE",
                "1_",
                "1__2",
                "0x_FF",
                ""
            ]
        );
    }

    #[test]
    fn malformed_numeric_literals_and_suffixes_are_invalid_single_tokens() {
        let tokens = token_stream("1i32 1u8 1.0r64 0b102 0x 1e+");

        assert_eq!(
            token_kinds(&*tokens),
            [
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::EndOfFileToken,
            ]
        );

        assert_eq!(
            token_texts(&tokens),
            ["1i32", "1u8", "1.0r64", "0b102", "0x", "1e+", ""]
        );
    }

    #[test]
    fn dot_prefixed_and_dot_suffixed_numbers_do_not_form_real_literals() {
        let tokens = token_stream(".5 1.");

        assert_eq!(
            token_kinds(&*tokens),
            [
                SyntaxKind::DotToken,
                SyntaxKind::DecimalIntegerLiteralToken,
                SyntaxKind::DecimalIntegerLiteralToken,
                SyntaxKind::DotToken,
                SyntaxKind::EndOfFileToken,
            ]
        );

        assert_eq!(token_texts(&tokens), [".", "5", "1", ".", ""]);
    }

    #[test]
    fn character_and_string_literals_accept_valid_escapes() {
        let tokens = token_stream(r#"'a' '\'' '\n' '\u{1F600}' "text\n\u{41}""#);

        assert_eq!(
            token_kinds(&*tokens),
            [
                SyntaxKind::CharacterLiteralToken,
                SyntaxKind::CharacterLiteralToken,
                SyntaxKind::CharacterLiteralToken,
                SyntaxKind::CharacterLiteralToken,
                SyntaxKind::StringLiteralToken,
                SyntaxKind::EndOfFileToken,
            ]
        );

        assert_eq!(
            token_texts(&tokens),
            [
                r#"'a'"#,
                r#"'\''"#,
                r#"'\n'"#,
                r#"'\u{1F600}'"#,
                r#""text\n\u{41}""#,
                ""
            ]
        );
    }

    #[test]
    fn invalid_character_and_string_literal_spellings_are_single_tokens() {
        let tokens = token_stream(r#"'' 'ab' '\q' '\u{110000}' "bad\q" "\u{}""#);

        assert_eq!(
            token_kinds(&*tokens),
            [
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::EndOfFileToken,
            ]
        );

        assert_eq!(
            token_texts(&tokens),
            [
                r#"''"#,
                r#"'ab'"#,
                r#"'\q'"#,
                r#"'\u{110000}'"#,
                r#""bad\q""#,
                r#""\u{}""#,
                ""
            ]
        );
    }

    #[test]
    fn unterminated_quoted_literals_stop_before_line_break_or_eof() {
        let tokens = token_stream("'a\n\"text");

        assert_eq!(
            token_kinds(&*tokens),
            [
                SyntaxKind::InvalidToken,
                SyntaxKind::InvalidToken,
                SyntaxKind::EndOfFileToken,
            ]
        );

        assert_eq!(token_texts(&tokens), ["'a", "\"text", ""]);
    }

    #[test]
    fn invalid_lexical_forms_emit_structured_diagnostics_and_recover() {
        let source = consumed_source("a \u{feff} \r é _foo === $ z");

        assert_goal_state_diagnostics(source.diagnostics());

        let misplaced_bom =
            diagnostics_of_kind(source.diagnostics(), DiagnosticKind::LexicalMisplacedBom);

        bray_testing::assert_goal_state_diagnostic_kind(
            &misplaced_bom,
            DiagnosticKind::LexicalMisplacedBom,
        );

        let lone_carriage_return = diagnostics_of_kind(
            source.diagnostics(),
            DiagnosticKind::LexicalLoneCarriageReturn,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &lone_carriage_return,
            DiagnosticKind::LexicalLoneCarriageReturn,
        );

        let non_ascii_identifier = diagnostics_of_kind(
            source.diagnostics(),
            DiagnosticKind::LexicalNonAsciiIdentifier,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &non_ascii_identifier,
            DiagnosticKind::LexicalNonAsciiIdentifier,
        );

        let invalid_identifier = diagnostics_of_kind(
            source.diagnostics(),
            DiagnosticKind::LexicalInvalidIdentifier,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &invalid_identifier,
            DiagnosticKind::LexicalInvalidIdentifier,
        );

        let invalid_operator = diagnostics_of_kind(
            source.diagnostics(),
            DiagnosticKind::LexicalInvalidOperatorOrPunctuation,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &invalid_operator,
            DiagnosticKind::LexicalInvalidOperatorOrPunctuation,
        );

        let invalid_character = diagnostics_of_kind(
            source.diagnostics(),
            DiagnosticKind::LexicalInvalidCharacter,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &invalid_character,
            DiagnosticKind::LexicalInvalidCharacter,
        );

        assert_eq!(
            diagnostic_kinds(source.diagnostics()),
            [
                DiagnosticKind::LexicalMisplacedBom,
                DiagnosticKind::LexicalLoneCarriageReturn,
                DiagnosticKind::LexicalNonAsciiIdentifier,
                DiagnosticKind::LexicalInvalidIdentifier,
                DiagnosticKind::LexicalInvalidOperatorOrPunctuation,
                DiagnosticKind::LexicalInvalidCharacter,
            ]
        );

        assert_eq!(source.diagnostics().len(), 6);
    }

    #[test]
    fn malformed_literals_emit_specific_structured_diagnostics() {
        let source = consumed_source(r#"1u8 0x "bad\q" "\u{}" '' 'ab' '\u{110000}'"#);

        assert_goal_state_diagnostics(source.diagnostics());

        let invalid_suffix = diagnostics_of_kind(
            source.diagnostics(),
            DiagnosticKind::LexicalInvalidNumericSuffix,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &invalid_suffix,
            DiagnosticKind::LexicalInvalidNumericSuffix,
        );

        let malformed_numeric = diagnostics_of_kind(
            source.diagnostics(),
            DiagnosticKind::LexicalMalformedNumericLiteral,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &malformed_numeric,
            DiagnosticKind::LexicalMalformedNumericLiteral,
        );

        let unknown_escape =
            diagnostics_of_kind(source.diagnostics(), DiagnosticKind::LexicalUnknownEscape);

        bray_testing::assert_goal_state_diagnostic_kind(
            &unknown_escape,
            DiagnosticKind::LexicalUnknownEscape,
        );

        let invalid_unicode_escape = diagnostics_of_kind(
            source.diagnostics(),
            DiagnosticKind::LexicalInvalidUnicodeEscape,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &invalid_unicode_escape,
            DiagnosticKind::LexicalInvalidUnicodeEscape,
        );

        let malformed_character = diagnostics_of_kind(
            source.diagnostics(),
            DiagnosticKind::LexicalMalformedCharacterLiteral,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &malformed_character,
            DiagnosticKind::LexicalMalformedCharacterLiteral,
        );

        assert_eq!(
            diagnostic_kinds(source.diagnostics()),
            [
                DiagnosticKind::LexicalInvalidNumericSuffix,
                DiagnosticKind::LexicalMalformedNumericLiteral,
                DiagnosticKind::LexicalUnknownEscape,
                DiagnosticKind::LexicalInvalidUnicodeEscape,
                DiagnosticKind::LexicalMalformedCharacterLiteral,
                DiagnosticKind::LexicalMalformedCharacterLiteral,
                DiagnosticKind::LexicalInvalidUnicodeEscape,
            ]
        );
    }

    #[test]
    fn unterminated_literals_and_comments_emit_diagnostics_while_recovering_to_eof() {
        let string_source = consumed_source(r#""open"#);
        let character_source = consumed_source("'o");
        let comment_source = consumed_source("value /* open");

        assert_goal_state_diagnostics(string_source.diagnostics());
        assert_goal_state_diagnostics(character_source.diagnostics());
        assert_goal_state_diagnostics(comment_source.diagnostics());

        bray_testing::assert_goal_state_diagnostic_kind(
            string_source.diagnostics(),
            DiagnosticKind::LexicalUnterminatedStringLiteral,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            character_source.diagnostics(),
            DiagnosticKind::LexicalUnterminatedCharacterLiteral,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            comment_source.diagnostics(),
            DiagnosticKind::LexicalUnterminatedBlockComment,
        );

        assert_eq!(
            diagnostic_kinds(string_source.diagnostics()),
            [DiagnosticKind::LexicalUnterminatedStringLiteral]
        );

        assert_eq!(
            diagnostic_kinds(character_source.diagnostics()),
            [DiagnosticKind::LexicalUnterminatedCharacterLiteral]
        );

        assert_eq!(
            diagnostic_kinds(comment_source.diagnostics()),
            [DiagnosticKind::LexicalUnterminatedBlockComment]
        );

        assert_eq!(comment_source.diagnostics().len(), 1);
    }

    #[test]
    fn nested_unterminated_block_comment_edit_closes_every_remaining_comment() {
        let source = "value /* outer /* inner";
        let token_source = consumed_source(source);

        let diagnostic = token_source
            .diagnostics()
            .by_kind(DiagnosticKind::LexicalUnterminatedBlockComment)
            .next()
            .unwrap_or_else(|| panic!("nested comment must publish its termination diagnostic"));

        let [suggestion] = diagnostic.suggestions() else {
            panic!("nested comment must publish one complete correction: {diagnostic:?}");
        };

        let [edit] = suggestion.edits() else {
            panic!("nested comment correction must contain one edit: {suggestion:?}");
        };

        assert_eq!(edit.replacement(), "*/*/");

        let corrected = format!("{source}{}", edit.replacement());
        let reparsed = consumed_source(&corrected);

        assert!(
            reparsed.diagnostics().is_empty(),
            "{:#?}",
            reparsed.diagnostics()
        );
    }

    #[test]
    fn repeated_uncached_lookahead_deduplicates_lexical_diagnostics() {
        let mut source =
            LexerTokenSource::with_cache_policy(snapshot("$"), LexerCachePolicy::DoNotCacheTokens);

        assert_eq!(source.lookahead(0).kind(), SyntaxKind::InvalidToken);
        assert_eq!(source.lookahead(0).kind(), SyntaxKind::InvalidToken);

        assert_eq!(
            diagnostic_kinds(source.diagnostics()),
            [DiagnosticKind::LexicalInvalidCharacter]
        );
    }

    #[test]
    fn tuple_index_scan_rejects_leading_zeroes() {
        let snapshot = snapshot(".01");

        let mut source = LexerTokenSource::new(snapshot);

        assert_eq!(source.consume().kind(), SyntaxKind::DotToken);

        let token = source.consume_tuple_element_index_after_dot();

        assert_eq!(token.kind(), SyntaxKind::InvalidToken);
        assert_eq!(token_text(source.source().text(), &token), "01");
    }

    #[test]
    fn tokens_carry_leading_and_trailing_trivia_once() {
        let mut source = LexerTokenSource::new(snapshot("  func // hi\r\n  main"));

        let function_keyword = source.consume();
        let main_identifier = source.consume();

        assert_eq!(function_keyword.kind(), SyntaxKind::FuncKeyword);

        assert_eq!(
            token_text(source.source().text(), &function_keyword),
            "func"
        );

        assert_eq!(
            function_keyword.range(),
            TextRange::new(TextSize::new(2), TextSize::new(6))
        );

        assert_eq!(
            trivia_kinds(function_keyword.leading_trivia()),
            [SyntaxKind::WhitespaceTrivia]
        );

        assert_eq!(
            trivia_texts(source.source().text(), function_keyword.leading_trivia()),
            ["  "]
        );

        assert_eq!(
            trivia_kinds(function_keyword.trailing_trivia()),
            [
                SyntaxKind::WhitespaceTrivia,
                SyntaxKind::LineCommentTrivia,
                SyntaxKind::WhitespaceTrivia,
            ]
        );

        assert_eq!(
            trivia_texts(source.source().text(), function_keyword.trailing_trivia()),
            [" ", "// hi", "\r\n"]
        );

        assert_eq!(main_identifier.kind(), SyntaxKind::IdentifierToken);
        assert_eq!(token_text(source.source().text(), &main_identifier), "main");

        assert_eq!(
            trivia_kinds(main_identifier.leading_trivia()),
            [SyntaxKind::WhitespaceTrivia]
        );

        assert_eq!(
            trivia_texts(source.source().text(), main_identifier.leading_trivia()),
            ["  "]
        );

        assert!(main_identifier.trailing_trivia().is_empty());
    }

    #[test]
    fn eof_carries_final_trivia_after_the_last_ordinary_token() {
        let mut source = LexerTokenSource::new(snapshot("func\n// final\r\n"));

        let function_keyword = source.consume();
        let eof = source.consume();

        assert_eq!(function_keyword.kind(), SyntaxKind::FuncKeyword);

        assert_eq!(
            trivia_texts(source.source().text(), function_keyword.trailing_trivia()),
            ["\n"]
        );

        assert_eq!(eof.kind(), SyntaxKind::EndOfFileToken);
        assert_eq!(eof.range(), TextRange::empty(TextSize::new(15)));

        assert_eq!(
            trivia_kinds(eof.leading_trivia()),
            [SyntaxKind::LineCommentTrivia, SyntaxKind::WhitespaceTrivia,]
        );

        assert_eq!(
            trivia_texts(source.source().text(), eof.leading_trivia()),
            ["// final", "\r\n"]
        );

        assert!(eof.trailing_trivia().is_empty());
    }

    #[test]
    fn documentation_and_nested_block_comments_are_preserved_as_trivia() {
        let tokens = token_stream("/// line\n/** block */\n/* outer /* inner */ end */\nvalue");

        let value = match tokens.first() {
            Some(token) => token,
            None => panic!("test source should produce a token"),
        };

        assert_eq!(value.kind(), SyntaxKind::IdentifierToken);

        assert_eq!(
            trivia_kinds(value.leading_trivia()),
            [
                SyntaxKind::DocumentationLineCommentTrivia,
                SyntaxKind::WhitespaceTrivia,
                SyntaxKind::DocumentationBlockCommentTrivia,
                SyntaxKind::WhitespaceTrivia,
                SyntaxKind::BlockCommentTrivia,
                SyntaxKind::WhitespaceTrivia,
            ]
        );

        assert_eq!(
            trivia_texts(tokens.source_text(), value.leading_trivia()),
            [
                "/// line",
                "\n",
                "/** block */",
                "\n",
                "/* outer /* inner */ end */",
                "\n",
            ]
        );
    }

    #[test]
    fn token_stream_reconstructs_source_text_with_trivia() {
        let text = "  func // hi\r\n/** docs */\nvalue /* tail */\n// eof\n";
        let tokens = token_stream(text);

        let reconstructed: String = tokens
            .iter()
            .map(|token| token.full_text(tokens.source_text()))
            .collect();

        assert_eq!(reconstructed, text);
    }

    #[test]
    fn lexer_token_sources_are_send_and_sync() {
        assert_send_sync::<LexerTokenSource>();
    }

    fn assert_send_sync<T: Send + Sync>() {}

    struct TokenStream {
        snapshot: SourceSnapshot,
        tokens: Vec<SyntaxToken>,
    }

    impl TokenStream {
        fn source_text(&self) -> &str {
            self.snapshot.text()
        }
    }

    impl Deref for TokenStream {
        type Target = [SyntaxToken];

        fn deref(&self) -> &Self::Target {
            &self.tokens
        }
    }

    fn token_stream(text: &str) -> TokenStream {
        let snapshot = snapshot(text);
        let mut source = LexerTokenSource::new(snapshot.clone());
        let mut tokens = Vec::new();

        loop {
            let token = source.consume();
            let is_eof = token.kind() == SyntaxKind::EndOfFileToken;

            tokens.push(token);

            if is_eof {
                return TokenStream { snapshot, tokens };
            }
        }
    }

    fn token_texts(tokens: &TokenStream) -> Vec<&str> {
        tokens
            .iter()
            .map(|token| token_text(tokens.source_text(), token))
            .collect()
    }

    fn trivia_kinds(trivia: &[SyntaxTrivia]) -> Vec<SyntaxKind> {
        trivia.iter().map(SyntaxTrivia::kind).collect()
    }

    fn trivia_texts<'source>(
        source_text: &'source str,
        trivia: &[SyntaxTrivia],
    ) -> Vec<&'source str> {
        trivia
            .iter()
            .map(|trivia| match trivia.text(source_text) {
                Some(text) => text,
                None => panic!("test trivia range should resolve into source text"),
            })
            .collect()
    }

    fn token_text<'source>(source_text: &'source str, token: &SyntaxToken) -> &'source str {
        match token.text(source_text) {
            Some(text) => text,
            None => panic!("test token range should resolve into source text"),
        }
    }

    fn consumed_source(text: &str) -> LexerTokenSource {
        let mut source = LexerTokenSource::new(snapshot(text));

        loop {
            let token = source.consume();

            if token.kind() == SyntaxKind::EndOfFileToken {
                return source;
            }
        }
    }

    fn snapshot(text: &str) -> SourceSnapshot {
        match SourceSnapshot::new(
            SourceId::new(0),
            SourceIdentity::new(0),
            SourceOrigin::virtual_source("lexer-test"),
            SourceVersion::new(0),
            text,
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test source should fit in TextSize: {error:?}"),
        }
    }
}
