use bray_source::{TextRange, TextSize};

use crate::SyntaxKind;

/// Trivia attached to a syntax token.
///
/// Trivia stores a kind and source byte range. Text is resolved from immutable
/// source text by range, avoiding duplicated whitespace and comment spelling.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SyntaxTrivia {
    kind: SyntaxKind,
    range: TextRange,
}

impl SyntaxTrivia {
    /// Creates trivia of a trivia syntax kind.
    ///
    /// Panics when `kind` is not a trivia kind.
    pub fn new(kind: SyntaxKind, range: TextRange) -> Self {
        assert!(kind.is_trivia());

        Self { kind, range }
    }

    /// Creates whitespace trivia.
    pub fn whitespace(range: TextRange) -> Self {
        Self::new(SyntaxKind::WhitespaceTrivia, range)
    }

    /// Creates line comment trivia.
    pub fn line_comment(range: TextRange) -> Self {
        Self::new(SyntaxKind::LineCommentTrivia, range)
    }

    /// Creates block comment trivia.
    pub fn block_comment(range: TextRange) -> Self {
        Self::new(SyntaxKind::BlockCommentTrivia, range)
    }

    /// Creates line documentation comment trivia.
    pub fn documentation_line_comment(range: TextRange) -> Self {
        Self::new(SyntaxKind::DocumentationLineCommentTrivia, range)
    }

    /// Creates block documentation comment trivia.
    pub fn documentation_block_comment(range: TextRange) -> Self {
        Self::new(SyntaxKind::DocumentationBlockCommentTrivia, range)
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

    /// Returns the exact trivia text from `source_text`.
    ///
    /// Returns `None` when the trivia range is outside `source_text` or does
    /// not align with UTF-8 scalar boundaries.
    pub fn text<'source>(&self, source_text: &'source str) -> Option<&'source str> {
        self.range.slice_str(source_text)
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{TextRange, TextSize};

    use super::SyntaxTrivia;
    use crate::SyntaxKind;

    #[test]
    fn trivia_carries_kind_and_range() {
        let source_text = "abc\t ";
        let range = TextRange::new(TextSize::new(3), TextSize::new(5));
        let trivia = SyntaxTrivia::whitespace(range);

        assert_eq!(trivia.kind(), SyntaxKind::WhitespaceTrivia);
        assert_eq!(trivia.range(), range);
        assert_eq!(trivia.start(), TextSize::new(3));
        assert_eq!(trivia.end(), TextSize::new(5));
        assert_eq!(trivia.text(source_text), Some("\t "));
    }

    #[test]
    fn documentation_comment_trivia_has_distinct_kinds() {
        let line = SyntaxTrivia::documentation_line_comment(TextRange::new(
            TextSize::new(0),
            TextSize::new(8),
        ));

        let block = SyntaxTrivia::documentation_block_comment(TextRange::new(
            TextSize::new(9),
            TextSize::new(20),
        ));

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
        let _ = SyntaxTrivia::new(SyntaxKind::IdentifierToken, TextRange::EMPTY);
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
