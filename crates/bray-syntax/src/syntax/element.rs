use std::fmt::{self, Write};
use std::slice;
use std::sync::Arc;

use bray_base::shared_slice;
use bray_source::TextRange;

use crate::{SyntaxKind, SyntaxToken};

/// Ordered syntax element stored by syntax nodes.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SyntaxElement {
    /// A lexical token with its attached trivia.
    Token(SyntaxToken),
}

impl SyntaxElement {
    /// Creates a syntax element from a token.
    pub const fn token(token: SyntaxToken) -> Self {
        Self::Token(token)
    }

    /// Returns this element's syntax kind.
    pub const fn kind(&self) -> SyntaxKind {
        match self {
            Self::Token(token) => token.kind(),
        }
    }

    /// Returns the full source range covered by this element.
    pub fn full_range(&self) -> TextRange {
        match self {
            Self::Token(token) => token.full_range(),
        }
    }

    /// Returns this element as a token when it is one.
    pub const fn as_token(&self) -> Option<&SyntaxToken> {
        match self {
            Self::Token(token) => Some(token),
        }
    }

    /// Appends this element's exact source text.
    ///
    /// Panics when `source_text` does not contain the element's source ranges.
    pub fn write_source_text(&self, source_text: &str, writer: &mut dyn Write) -> fmt::Result {
        match self {
            Self::Token(token) => token.write_full_text(source_text, writer),
        }
    }
}

impl From<SyntaxToken> for SyntaxElement {
    fn from(token: SyntaxToken) -> Self {
        Self::token(token)
    }
}

/// Ordered syntax elements in source order.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct SyntaxElements {
    elements: Arc<[SyntaxElement]>,
}

impl SyntaxElements {
    /// Creates an ordered syntax element collection.
    pub fn new(elements: impl IntoIterator<Item = SyntaxElement>) -> Self {
        Self {
            elements: shared_slice(elements),
        }
    }

    /// Creates an ordered syntax element collection from tokens.
    pub fn from_tokens(tokens: impl IntoIterator<Item = SyntaxToken>) -> Self {
        Self::new(tokens.into_iter().map(SyntaxElement::from))
    }

    /// Returns the elements as a slice.
    pub fn as_slice(&self) -> &[SyntaxElement] {
        &self.elements
    }

    /// Returns the number of stored syntax elements.
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Returns whether this collection contains no syntax elements.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Returns an iterator over the syntax elements.
    pub fn iter(&self) -> slice::Iter<'_, SyntaxElement> {
        self.elements.iter()
    }

    /// Returns an iterator over token elements.
    pub fn tokens(&self) -> impl Iterator<Item = &SyntaxToken> + '_ {
        self.iter().filter_map(SyntaxElement::as_token)
    }

    /// Returns the last token element, if any.
    pub fn last_token(&self) -> Option<&SyntaxToken> {
        self.tokens().last()
    }

    /// Appends exact source text for each element in source order.
    ///
    /// Panics when `source_text` does not contain any element range.
    pub fn write_source_text(&self, source_text: &str, writer: &mut dyn Write) -> fmt::Result {
        for element in self.iter() {
            element.write_source_text(source_text, writer)?;
        }

        Ok(())
    }
}

impl FromIterator<SyntaxElement> for SyntaxElements {
    fn from_iter<T: IntoIterator<Item = SyntaxElement>>(iter: T) -> Self {
        Self::new(iter)
    }
}

impl AsRef<[SyntaxElement]> for SyntaxElements {
    fn as_ref(&self) -> &[SyntaxElement] {
        self.as_slice()
    }
}

impl<'elements> IntoIterator for &'elements SyntaxElements {
    type Item = &'elements SyntaxElement;
    type IntoIter = slice::Iter<'elements, SyntaxElement>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{TextRange, TextSize};

    use super::{SyntaxElement, SyntaxElements};
    use crate::{SyntaxKind, SyntaxToken, SyntaxTrivia};

    #[test]
    fn elements_store_ordered_tokens() {
        let first = SyntaxToken::new(
            SyntaxKind::FuncKeyword,
            TextRange::new(TextSize::ZERO, TextSize::new(4)),
        );

        let second = SyntaxToken::end_of_file(TextSize::new(4));
        let elements = SyntaxElements::from_tokens([first.clone(), second.clone()]);

        assert_eq!(elements.len(), 2);

        assert_eq!(
            elements.as_slice(),
            [
                SyntaxElement::Token(first.clone()),
                SyntaxElement::Token(second.clone())
            ]
        );

        assert_eq!(
            elements.tokens().cloned().collect::<Vec<_>>(),
            [first, second]
        );

        assert_eq!(
            elements.last_token().map(SyntaxToken::kind),
            Some(SyntaxKind::EndOfFileToken)
        );
    }

    #[test]
    fn elements_write_source_text_in_order() {
        let source_text = " func";
        let leading = SyntaxTrivia::whitespace(TextRange::new(TextSize::ZERO, TextSize::new(1)));

        let token = SyntaxToken::new(
            SyntaxKind::FuncKeyword,
            TextRange::new(TextSize::new(1), TextSize::new(5)),
        )
        .with_leading_trivia([leading]);

        let elements = SyntaxElements::from_tokens([token]);

        let mut written_text = String::new();

        match elements.write_source_text(source_text, &mut written_text) {
            Ok(()) => {}
            Err(error) => panic!("writing element text should succeed: {error:?}"),
        }

        assert_eq!(written_text, source_text);
    }

    #[test]
    fn syntax_elements_are_send_and_sync() {
        assert_send_sync::<SyntaxElement>();
        assert_send_sync::<SyntaxElements>();
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
