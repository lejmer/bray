use std::fmt::{self, Write};
use std::sync::Arc;

use bray_base::shared_slice;
use bray_source::{TextRange, TextSize};

use crate::text::text_from_writer;
use crate::{SyntaxKind, SyntaxTrivia};

/// Range-bearing syntax token produced by lexical analysis or syntax traversal.
///
/// A token stores its source range plus leading and trailing trivia. Token text
/// is resolved from immutable source text by range, so tokens stay compact and
/// do not duplicate source spelling.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SyntaxToken {
    kind: SyntaxKind,
    range: TextRange,
    leading_trivia: Arc<[SyntaxTrivia]>,
    trailing_trivia: Arc<[SyntaxTrivia]>,
}

impl SyntaxToken {
    /// Creates a token with no leading or trailing trivia.
    ///
    /// Panics when `kind` is not a token kind.
    pub fn new(kind: SyntaxKind, range: TextRange) -> Self {
        Self::with_trivia(kind, range, Vec::new(), Vec::new())
    }

    /// Creates a token with explicit leading and trailing trivia.
    ///
    /// Panics when `kind` is not a token kind.
    pub fn with_trivia(
        kind: SyntaxKind,
        range: TextRange,
        leading_trivia: impl IntoIterator<Item = SyntaxTrivia>,
        trailing_trivia: impl IntoIterator<Item = SyntaxTrivia>,
    ) -> Self {
        assert!(kind.is_token());

        Self {
            kind,
            range,
            leading_trivia: shared_slice(leading_trivia),
            trailing_trivia: shared_slice(trailing_trivia),
        }
    }

    /// Creates an end-of-file token at `offset`.
    pub fn end_of_file(offset: TextSize) -> Self {
        Self::new(SyntaxKind::EndOfFileToken, TextRange::empty(offset))
    }

    /// Creates an invalid token covering `range`.
    pub fn invalid(range: TextRange) -> Self {
        Self::new(SyntaxKind::InvalidToken, range)
    }

    /// Replaces the token's leading trivia.
    pub fn with_leading_trivia(self, trivia: impl IntoIterator<Item = SyntaxTrivia>) -> Self {
        Self {
            leading_trivia: shared_slice(trivia),
            ..self
        }
    }

    /// Replaces the token's trailing trivia.
    pub fn with_trailing_trivia(self, trivia: impl IntoIterator<Item = SyntaxTrivia>) -> Self {
        Self {
            trailing_trivia: shared_slice(trivia),
            ..self
        }
    }

    /// Returns the token kind.
    pub const fn kind(&self) -> SyntaxKind {
        self.kind
    }

    /// Returns whether this token is the end-of-file marker.
    pub const fn is_end_of_file(&self) -> bool {
        matches!(self.kind, SyntaxKind::EndOfFileToken)
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

    /// Returns the exact token text from `source_text`, excluding trivia.
    ///
    /// Returns `None` when the token range is outside `source_text` or does not
    /// align with UTF-8 scalar boundaries.
    pub fn text<'source>(&self, source_text: &'source str) -> Option<&'source str> {
        self.range.slice_str(source_text)
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

    /// Appends this token's leading trivia, token text, and trailing trivia.
    ///
    /// Panics when `source_text` does not contain the token or trivia ranges.
    pub fn write_full_text(&self, source_text: &str, writer: &mut dyn Write) -> fmt::Result {
        for trivia in self.leading_trivia() {
            writer.write_str(required_text(source_text, trivia.range()))?;
        }

        writer.write_str(required_text(source_text, self.range()))?;

        for trivia in self.trailing_trivia() {
            writer.write_str(required_text(source_text, trivia.range()))?;
        }

        Ok(())
    }

    /// Returns this token's leading trivia, token text, and trailing trivia.
    ///
    /// Panics when `source_text` does not contain the token or trivia ranges.
    pub fn full_text(&self, source_text: &str) -> String {
        text_from_writer(|writer| self.write_full_text(source_text, writer))
    }
}

fn required_text(source_text: &str, range: TextRange) -> &str {
    match range.slice_str(source_text) {
        Some(text) => text,
        None => panic!("syntax range is not covered by source text"),
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{TextRange, TextSize};

    use super::SyntaxToken;
    use crate::{SyntaxKind, SyntaxTrivia};

    #[test]
    fn tokens_carry_kind_range_and_trivia() {
        let source_text = " func\n";
        let leading = SyntaxTrivia::whitespace(TextRange::new(TextSize::ZERO, TextSize::new(1)));

        let trailing = SyntaxTrivia::whitespace(TextRange::new(TextSize::new(5), TextSize::new(6)));

        let token = SyntaxToken::with_trivia(
            SyntaxKind::FuncKeyword,
            TextRange::new(TextSize::new(1), TextSize::new(5)),
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

        assert_eq!(token.text(source_text), Some("func"));
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
        assert!(token.is_end_of_file());
        assert_eq!(token.range(), TextRange::empty(TextSize::new(8)));
        assert_eq!(token.text("abcdefgh"), Some(""));
        assert_eq!(token.full_text("abcdefgh"), "");
    }

    #[test]
    fn invalid_tokens_use_invalid_kind() {
        let token = SyntaxToken::invalid(TextRange::new(TextSize::new(2), TextSize::new(5)));

        assert_eq!(token.kind(), SyntaxKind::InvalidToken);
        assert_eq!(token.text("?????"), Some("???"));
    }

    #[test]
    fn tokens_reconstruct_exact_source_text_with_trivia() {
        let source_text = "// docs\nmain ";
        let leading = SyntaxTrivia::line_comment(TextRange::new(TextSize::ZERO, TextSize::new(7)));

        let separator =
            SyntaxTrivia::whitespace(TextRange::new(TextSize::new(7), TextSize::new(8)));

        let token = SyntaxToken::new(
            SyntaxKind::IdentifierToken,
            TextRange::new(TextSize::new(8), TextSize::new(12)),
        )
        .with_leading_trivia([leading, separator])
        .with_trailing_trivia([SyntaxTrivia::whitespace(TextRange::new(
            TextSize::new(12),
            TextSize::new(13),
        ))]);

        assert_eq!(token.full_text(source_text), source_text);

        let mut written_text = String::new();

        match token.write_full_text(source_text, &mut written_text) {
            Ok(()) => {}
            Err(error) => panic!("writing token text should succeed: {error:?}"),
        }

        assert_eq!(written_text, source_text);
    }

    #[test]
    fn syntax_tokens_are_send_and_sync() {
        assert_send_sync::<SyntaxToken>();
    }

    #[test]
    #[should_panic]
    fn tokens_reject_non_token_kinds() {
        let _ = SyntaxToken::new(SyntaxKind::SourceUnit, TextRange::EMPTY);
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
