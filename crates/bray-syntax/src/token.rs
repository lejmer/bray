use std::fmt::{self, Write};
use std::sync::Arc;

use bray_base::shared_str;
use bray_source::{TextRange, TextSize};

use crate::text::assert_text_len_matches_range;
use crate::{SyntaxKind, SyntaxTrivia};

/// Lossless syntax token produced by lexical analysis.
///
/// A token stores its own source range and exact text, plus leading and trailing
/// trivia. The token range excludes trivia; [`full_range`](Self::full_range)
/// covers both token text and attached trivia.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SyntaxToken {
    kind: SyntaxKind,
    range: TextRange,
    text: Arc<str>,
    leading_trivia: Arc<[SyntaxTrivia]>,
    trailing_trivia: Arc<[SyntaxTrivia]>,
}

impl SyntaxToken {
    /// Creates a token with no leading or trailing trivia.
    ///
    /// Panics when `kind` is not a token kind or when `text` does not cover
    /// exactly the same byte length as `range`.
    pub fn new(kind: SyntaxKind, range: TextRange, text: impl Into<Arc<str>>) -> Self {
        Self::with_trivia(kind, range, text, Vec::new(), Vec::new())
    }

    /// Creates a token with explicit leading and trailing trivia.
    ///
    /// Panics when `kind` is not a token kind or when `text` does not cover
    /// exactly the same byte length as `range`.
    pub fn with_trivia(
        kind: SyntaxKind,
        range: TextRange,
        text: impl Into<Arc<str>>,
        leading_trivia: impl Into<Vec<SyntaxTrivia>>,
        trailing_trivia: impl Into<Vec<SyntaxTrivia>>,
    ) -> Self {
        assert!(kind.is_token());

        let text = shared_str(text);

        assert_text_len_matches_range(text.as_ref(), range);

        Self {
            kind,
            range,
            text,
            leading_trivia: shared_trivia(leading_trivia),
            trailing_trivia: shared_trivia(trailing_trivia),
        }
    }

    /// Creates an end-of-file token at `offset`.
    pub fn end_of_file(offset: TextSize) -> Self {
        Self::new(SyntaxKind::EndOfFileToken, TextRange::empty(offset), "")
    }

    /// Creates an invalid token covering `range`.
    pub fn invalid(range: TextRange, text: impl Into<Arc<str>>) -> Self {
        Self::new(SyntaxKind::InvalidToken, range, text)
    }

    /// Replaces the token's leading trivia.
    pub fn with_leading_trivia(self, trivia: impl Into<Vec<SyntaxTrivia>>) -> Self {
        Self {
            leading_trivia: shared_trivia(trivia),
            ..self
        }
    }

    /// Replaces the token's trailing trivia.
    pub fn with_trailing_trivia(self, trivia: impl Into<Vec<SyntaxTrivia>>) -> Self {
        Self {
            trailing_trivia: shared_trivia(trivia),
            ..self
        }
    }

    /// Returns the token kind.
    pub const fn kind(&self) -> SyntaxKind {
        self.kind
    }

    /// Returns the token source byte range, excluding trivia.
    pub const fn range(&self) -> TextRange {
        self.range
    }

    /// Returns the token start byte offset, excluding trivia.
    pub const fn start(&self) -> TextSize {
        self.range.start()
    }

    /// Returns the token exclusive end byte offset, excluding trivia.
    pub const fn end(&self) -> TextSize {
        self.range.end()
    }

    /// Returns the exact token text, excluding trivia.
    pub fn text(&self) -> &str {
        self.text.as_ref()
    }

    /// Returns the leading trivia attached to this token.
    pub fn leading_trivia(&self) -> &[SyntaxTrivia] {
        &self.leading_trivia
    }

    /// Returns the trailing trivia attached to this token.
    pub fn trailing_trivia(&self) -> &[SyntaxTrivia] {
        &self.trailing_trivia
    }

    /// Returns the source byte range covering this token and all attached trivia.
    pub fn full_range(&self) -> TextRange {
        let with_leading = match self.leading_trivia.first() {
            Some(first) => first.range().cover(self.range),
            None => self.range,
        };

        match self.trailing_trivia.last() {
            Some(last) => with_leading.cover(last.range()),
            None => with_leading,
        }
    }

    /// Appends this token's leading trivia, text, and trailing trivia to `writer`.
    pub fn write_full_text(&self, writer: &mut impl Write) -> fmt::Result {
        for part in self.full_text_parts() {
            writer.write_str(part)?;
        }

        Ok(())
    }

    /// Returns this token's leading trivia, text, and trailing trivia.
    pub fn full_text(&self) -> String {
        self.full_text_parts().collect()
    }

    fn full_text_parts(&self) -> impl Iterator<Item = &str> {
        self.leading_trivia()
            .iter()
            .map(SyntaxTrivia::text)
            .chain(std::iter::once(self.text()))
            .chain(self.trailing_trivia().iter().map(SyntaxTrivia::text))
    }
}

fn shared_trivia(trivia: impl Into<Vec<SyntaxTrivia>>) -> Arc<[SyntaxTrivia]> {
    Arc::<[SyntaxTrivia]>::from(trivia.into())
}

#[cfg(test)]
mod tests {
    use bray_source::{TextRange, TextSize};

    use super::SyntaxToken;
    use crate::{SyntaxKind, SyntaxTrivia};

    #[test]
    fn tokens_carry_kind_range_text_and_trivia() {
        let leading =
            SyntaxTrivia::whitespace(TextRange::new(TextSize::ZERO, TextSize::new(1)), " ");

        let trailing =
            SyntaxTrivia::whitespace(TextRange::new(TextSize::new(5), TextSize::new(6)), "\n");

        let token = SyntaxToken::with_trivia(
            SyntaxKind::FuncKeyword,
            TextRange::new(TextSize::new(1), TextSize::new(5)),
            "func",
            [leading.clone()],
            [trailing.clone()],
        );

        assert_eq!(token.kind(), SyntaxKind::FuncKeyword);

        assert_eq!(
            token.range(),
            TextRange::new(TextSize::new(1), TextSize::new(5))
        );

        assert_eq!(token.start(), TextSize::new(1));
        assert_eq!(token.end(), TextSize::new(5));

        assert_eq!(token.text(), "func");

        assert_eq!(token.leading_trivia(), &[leading]);
        assert_eq!(token.trailing_trivia(), &[trailing]);

        assert_eq!(
            token.full_range(),
            TextRange::new(TextSize::ZERO, TextSize::new(6))
        );
    }

    #[test]
    fn eof_tokens_are_empty_at_offset() {
        let token = SyntaxToken::end_of_file(TextSize::new(8));

        assert_eq!(token.kind(), SyntaxKind::EndOfFileToken);
        assert_eq!(token.range(), TextRange::empty(TextSize::new(8)));
        assert_eq!(token.text(), "");
        assert_eq!(token.full_text(), "");
    }

    #[test]
    fn invalid_tokens_use_invalid_kind() {
        let token = SyntaxToken::invalid(TextRange::new(TextSize::new(2), TextSize::new(5)), "???");

        assert_eq!(token.kind(), SyntaxKind::InvalidToken);
        assert_eq!(token.text(), "???");
    }

    #[test]
    fn tokens_reconstruct_lossless_text_with_trivia() {
        let leading =
            SyntaxTrivia::line_comment(TextRange::new(TextSize::ZERO, TextSize::new(7)), "// docs");

        let separator =
            SyntaxTrivia::whitespace(TextRange::new(TextSize::new(7), TextSize::new(8)), "\n");

        let token = SyntaxToken::new(
            SyntaxKind::IdentifierToken,
            TextRange::new(TextSize::new(8), TextSize::new(12)),
            "main",
        )
        .with_leading_trivia([leading, separator])
        .with_trailing_trivia([SyntaxTrivia::whitespace(
            TextRange::new(TextSize::new(12), TextSize::new(13)),
            " ",
        )]);

        assert_eq!(token.full_text(), "// docs\nmain ");

        let mut written_text = String::new();

        match token.write_full_text(&mut written_text) {
            Ok(()) => {}
            Err(error) => panic!("writing token text should succeed: {error:?}"),
        }

        assert_eq!(written_text, "// docs\nmain ");
    }

    #[test]
    fn syntax_tokens_are_send_and_sync() {
        assert_send_sync::<SyntaxToken>();
    }

    #[test]
    #[should_panic]
    fn tokens_reject_non_token_kinds() {
        let _ = SyntaxToken::new(SyntaxKind::SourceUnit, TextRange::EMPTY, "");
    }

    #[test]
    #[should_panic]
    fn tokens_reject_text_that_does_not_match_range_length() {
        let _ = SyntaxToken::new(
            SyntaxKind::IdentifierToken,
            TextRange::new(TextSize::ZERO, TextSize::new(2)),
            "abc",
        );
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
