use bray_source::{SourceSnapshot, TextRange, TextSize};
use bray_syntax::{SyntaxKind, SyntaxToken};

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
/// The current implementation deliberately classifies every non-EOF UTF-8
/// scalar as a one-character invalid token. The cursor, ranges, EOF behavior,
/// cache policy, and parser-driven tuple-index entrypoint are the stable
/// parser-facing contract that later real tokenization will fill in.
#[derive(Clone, Debug)]
pub struct LexerTokenSource {
    snapshot: SourceSnapshot,
    cursor: TextSize,
    cache_policy: LexerCachePolicy,
    cached_tokens: Vec<SyntaxToken>,
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
    ///
    /// TODO: Until real tokenization lands, this shares the invalid-token
    ///       stub path while preserving the distinct parser entrypoint.
    pub fn consume_tuple_element_index_after_dot(&mut self) -> SyntaxToken {
        self.cached_tokens.clear();

        let token = self.scan_current_token(LexerScanMode::TupleElementIndexAfterDot);

        self.advance_after_consuming_uncached(&token);

        token
    }

    fn cached_lookahead(&mut self, distance: usize) -> SyntaxToken {
        self.ensure_cached(distance);

        match self.cached_tokens.get(distance) {
            Some(token) => share_token(token),
            None => panic!("lexer cache did not contain requested lookahead token"),
        }
    }

    fn uncached_lookahead(&self, distance: usize) -> SyntaxToken {
        let mut offset = self.cursor;
        let mut token = scan_token_at(&self.snapshot, offset, LexerScanMode::Normal);

        for _ in 0..distance {
            if is_eof(&token) {
                return token;
            }

            offset = token.end();
            token = scan_token_at(&self.snapshot, offset, LexerScanMode::Normal);
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

    fn uncached_lookahead_window(&self, len: usize) -> Vec<SyntaxToken> {
        let mut tokens = Vec::with_capacity(len);
        let mut offset = self.cursor;

        while tokens.len() < len {
            let token = scan_token_at(&self.snapshot, offset, LexerScanMode::Normal);
            let reached_eof = is_eof(&token);
            let next_offset = token.end();

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
                Some(token) => token.end(),
                None => self.cursor,
            };

            let token = scan_token_at(&self.snapshot, offset, LexerScanMode::Normal);

            self.cached_tokens.push(token);
        }
    }

    fn scan_current_token(&self, mode: LexerScanMode) -> SyntaxToken {
        scan_token_at(&self.snapshot, self.cursor, mode)
    }

    fn advance_after_consuming(&mut self, token: &SyntaxToken) {
        if is_eof(token) {
            return;
        }

        self.cursor = token.end();

        if self.cache_policy == LexerCachePolicy::CacheTokens && !self.cached_tokens.is_empty() {
            self.cached_tokens.remove(0);
        }
    }

    fn advance_after_consuming_uncached(&mut self, token: &SyntaxToken) {
        if is_eof(token) {
            return;
        }

        self.cursor = token.end();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LexerScanMode {
    Normal,
    TupleElementIndexAfterDot,
}

fn scan_token_at(snapshot: &SourceSnapshot, start: TextSize, mode: LexerScanMode) -> SyntaxToken {
    match mode {
        LexerScanMode::Normal | LexerScanMode::TupleElementIndexAfterDot => {
            scan_invalid_scalar_token(snapshot, start)
        }
    }
}

fn scan_invalid_scalar_token(snapshot: &SourceSnapshot, start: TextSize) -> SyntaxToken {
    if start == snapshot.text_len() {
        return SyntaxToken::end_of_file(start);
    }

    if snapshot.text_len() < start {
        panic!("lexer cursor moved past source text");
    }

    let source_text = snapshot.text();
    let start_index = text_size_to_usize(start);

    let remainder = match source_text.get(start_index..) {
        Some(remainder) => remainder,
        None => panic!("lexer cursor is not on a UTF-8 boundary"),
    };

    let character = match remainder.chars().next() {
        Some(character) => character,
        None => return SyntaxToken::end_of_file(start),
    };

    let character_len = character.len_utf8();

    let token_len = match TextSize::try_from(character_len) {
        Ok(token_len) => token_len,
        Err(error) => panic!("UTF-8 scalar length should fit in TextSize: {error:?}"),
    };

    let range = match TextRange::with_len(start, token_len) {
        Some(range) => range,
        None => panic!("lexer token range overflowed TextSize"),
    };

    let token_text = match snapshot.text_slice(range) {
        Some(token_text) => token_text,
        None => panic!("lexer token range is not on UTF-8 boundaries"),
    };

    SyntaxToken::invalid(range, token_text)
}

fn is_eof(token: &SyntaxToken) -> bool {
    token.kind() == SyntaxKind::EndOfFileToken
}

fn fill_with_eof(tokens: &mut Vec<SyntaxToken>, len: usize, offset: TextSize) {
    while tokens.len() < len {
        tokens.push(SyntaxToken::end_of_file(offset));
    }
}

fn text_size_to_usize(size: TextSize) -> usize {
    match usize::try_from(size.bytes()) {
        Ok(size) => size,
        Err(_) => panic!("TextSize did not fit in usize on this target"),
    }
}

fn share_token(token: &SyntaxToken) -> SyntaxToken {
    // SyntaxToken clones share immutable Arc-backed text and trivia storage.
    token.clone()
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceId, SourceIdentity, SourceOrigin, SourceVersion};

    use super::{LexerCachePolicy, LexerTokenSource};
    use bray_source::{SourceSnapshot, TextRange, TextSize};
    use bray_syntax::SyntaxKind;

    #[test]
    fn peek_does_not_consume_the_next_token() {
        let snapshot = snapshot("ab");

        let mut source = LexerTokenSource::new(snapshot);

        let token = source.peek();

        assert_eq!(token.kind(), SyntaxKind::InvalidToken);
        assert_eq!(token.text(), "a");

        assert_eq!(
            token.range(),
            TextRange::new(TextSize::ZERO, TextSize::new(1))
        );

        assert_eq!(source.current_offset(), TextSize::ZERO);

        let consumed = source.consume();

        assert_eq!(consumed, token);
        assert_eq!(source.current_offset(), TextSize::new(1));
    }

    #[test]
    fn lookahead_windows_are_zero_based_and_do_not_advance() {
        let snapshot = snapshot("abc");

        let mut source = LexerTokenSource::new(snapshot);

        assert_eq!(source.lookahead(0).text(), "a");
        assert_eq!(source.lookahead(1).text(), "b");
        assert_eq!(source.lookahead(2).text(), "c");

        let window = source.lookahead_window(4);
        let texts: Vec<&str> = window.iter().map(|token| token.text()).collect();

        assert_eq!(texts, ["a", "b", "c", ""]);
        assert_eq!(window[3].kind(), SyntaxKind::EndOfFileToken);
        assert_eq!(source.current_offset(), TextSize::ZERO);
    }

    #[test]
    fn consume_advances_by_utf8_byte_ranges() {
        let snapshot = snapshot("aé");

        let mut source = LexerTokenSource::new(snapshot);

        let ascii = source.consume();
        let accented = source.consume();
        let eof = source.consume();

        assert_eq!(
            ascii.range(),
            TextRange::new(TextSize::ZERO, TextSize::new(1))
        );

        assert_eq!(
            accented.range(),
            TextRange::new(TextSize::new(1), TextSize::new(3))
        );

        assert_eq!(accented.text(), "é");
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
        let snapshot = snapshot("ab");

        let mut source =
            LexerTokenSource::with_cache_policy(snapshot, LexerCachePolicy::DoNotCacheTokens);

        assert_eq!(source.cache_policy(), LexerCachePolicy::DoNotCacheTokens);
        assert_eq!(source.peek().text(), "a");
        assert_eq!(source.lookahead(1).text(), "b");
        assert_eq!(source.consume().text(), "a");
        assert_eq!(source.consume().text(), "b");
        assert_eq!(source.consume().kind(), SyntaxKind::EndOfFileToken);
    }

    #[test]
    fn tuple_index_scan_after_dot_has_a_parser_driven_entrypoint() {
        let snapshot = snapshot(".1");

        let mut source = LexerTokenSource::new(snapshot);

        let dot = source.consume();
        let cached_ordinary_next = source.lookahead(0);
        let tuple_index = source.consume_tuple_element_index_after_dot();

        assert_eq!(dot.text(), ".");
        assert_eq!(cached_ordinary_next.text(), "1");
        assert_eq!(tuple_index.kind(), SyntaxKind::InvalidToken);
        assert_eq!(tuple_index.text(), "1");

        assert_eq!(
            tuple_index.range(),
            TextRange::new(TextSize::new(1), TextSize::new(2))
        );

        assert_eq!(source.current_offset(), TextSize::new(2));
    }

    #[test]
    fn lexer_token_sources_are_send_and_sync() {
        assert_send_sync::<LexerTokenSource>();
    }

    fn assert_send_sync<T: Send + Sync>() {}

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
