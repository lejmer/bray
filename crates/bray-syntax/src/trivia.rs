use std::sync::Arc;

use bray_base::shared_str;
use bray_source::{TextRange, TextSize};

use crate::SyntaxKind;
use crate::text::assert_text_len_matches_range;

/// Lossless trivia attached to a syntax token.
///
/// Trivia stores whitespace and comments. It is not represented as ordinary
/// syntax nodes, but it is retained so syntax token streams can recreate source
/// text exactly.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SyntaxTrivia {
    kind: SyntaxKind,
    range: TextRange,
    text: Arc<str>,
}

impl SyntaxTrivia {
    /// Creates trivia of a trivia syntax kind.
    ///
    /// Panics when `kind` is not a trivia kind or when `text` does not cover
    /// exactly the same byte length as `range`.
    pub fn new(kind: SyntaxKind, range: TextRange, text: impl Into<Arc<str>>) -> Self {
        assert!(kind.is_trivia());

        let text = shared_str(text);

        assert_text_len_matches_range(text.as_ref(), range);

        Self { kind, range, text }
    }

    /// Creates whitespace trivia.
    pub fn whitespace(range: TextRange, text: impl Into<Arc<str>>) -> Self {
        Self::new(SyntaxKind::WhitespaceTrivia, range, text)
    }

    /// Creates line comment trivia.
    pub fn line_comment(range: TextRange, text: impl Into<Arc<str>>) -> Self {
        Self::new(SyntaxKind::LineCommentTrivia, range, text)
    }

    /// Creates block comment trivia.
    pub fn block_comment(range: TextRange, text: impl Into<Arc<str>>) -> Self {
        Self::new(SyntaxKind::BlockCommentTrivia, range, text)
    }

    /// Creates line documentation comment trivia.
    pub fn documentation_line_comment(range: TextRange, text: impl Into<Arc<str>>) -> Self {
        Self::new(SyntaxKind::DocumentationLineCommentTrivia, range, text)
    }

    /// Creates block documentation comment trivia.
    pub fn documentation_block_comment(range: TextRange, text: impl Into<Arc<str>>) -> Self {
        Self::new(SyntaxKind::DocumentationBlockCommentTrivia, range, text)
    }

    /// Returns the trivia kind.
    pub const fn kind(&self) -> SyntaxKind {
        self.kind
    }

    /// Returns the trivia source byte range.
    pub const fn range(&self) -> TextRange {
        self.range
    }

    /// Returns the start byte offset.
    pub const fn start(&self) -> TextSize {
        self.range.start()
    }

    /// Returns the exclusive end byte offset.
    pub const fn end(&self) -> TextSize {
        self.range.end()
    }

    /// Returns the exact trivia text.
    pub fn text(&self) -> &str {
        self.text.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{TextRange, TextSize};

    use super::SyntaxTrivia;
    use crate::SyntaxKind;

    #[test]
    fn trivia_carries_kind_range_and_text() {
        let range = TextRange::new(TextSize::new(3), TextSize::new(5));
        let trivia = SyntaxTrivia::whitespace(range, "\t ");

        assert_eq!(trivia.kind(), SyntaxKind::WhitespaceTrivia);
        assert_eq!(trivia.range(), range);
        assert_eq!(trivia.start(), TextSize::new(3));
        assert_eq!(trivia.end(), TextSize::new(5));
        assert_eq!(trivia.text(), "\t ");
    }

    #[test]
    fn documentation_comment_trivia_has_distinct_kinds() {
        let line = SyntaxTrivia::documentation_line_comment(
            TextRange::new(TextSize::new(0), TextSize::new(8)),
            "/// docs",
        );

        let block = SyntaxTrivia::documentation_block_comment(
            TextRange::new(TextSize::new(9), TextSize::new(20)),
            "/** docs */",
        );

        assert_eq!(line.kind(), SyntaxKind::DocumentationLineCommentTrivia);
        assert_eq!(block.kind(), SyntaxKind::DocumentationBlockCommentTrivia);
    }

    #[test]
    fn syntax_trivia_is_send_and_sync() {
        assert_send_sync::<SyntaxTrivia>();
    }

    #[test]
    #[should_panic]
    fn trivia_rejects_non_trivia_kinds() {
        let _ = SyntaxTrivia::new(SyntaxKind::IdentifierToken, TextRange::EMPTY, "");
    }

    #[test]
    #[should_panic]
    fn trivia_rejects_text_that_does_not_match_range_length() {
        let _ = SyntaxTrivia::whitespace(TextRange::new(TextSize::ZERO, TextSize::new(3)), " ");
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
