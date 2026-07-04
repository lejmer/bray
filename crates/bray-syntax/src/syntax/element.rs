use std::fmt::{self, Write};
use std::sync::Arc;

use bray_base::shared_slice;

use crate::SyntaxToken;

/// Internal source-order storage for syntax nodes.
///
/// This storage supports source reconstruction and generic exhaustive traversal.
/// It is not a grammar contract. Grammar-aware code should use named slots and
/// child lists on typed syntax nodes.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SourceOrderElement {
    /// A lexical token with its attached trivia.
    Token(SyntaxToken),
}

impl SourceOrderElement {
    /// Creates a source-order element from a token.
    pub(crate) const fn token(token: SyntaxToken) -> Self {
        Self::Token(token)
    }

    /// Appends this element's exact source text.
    ///
    /// Panics when `source_text` does not contain the element's source ranges.
    pub(crate) fn write_source_text(
        &self,
        source_text: &str,
        writer: &mut dyn Write,
    ) -> fmt::Result {
        match self {
            Self::Token(token) => token.write_full_text(source_text, writer),
        }
    }
}

impl From<SyntaxToken> for SourceOrderElement {
    fn from(token: SyntaxToken) -> Self {
        Self::token(token)
    }
}

/// Internal syntax element storage in source order.
///
/// Internal callers must not infer grammar structure from this order. Typed syntax
/// nodes define the stable named component API.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct SourceOrderElements {
    elements: Arc<[SourceOrderElement]>,
}

impl SourceOrderElements {
    /// Creates source-order element storage.
    pub(crate) fn new(elements: impl IntoIterator<Item = SourceOrderElement>) -> Self {
        Self {
            elements: shared_slice(elements),
        }
    }

    /// Creates source-order element storage from tokens.
    pub(crate) fn from_tokens(tokens: impl IntoIterator<Item = SyntaxToken>) -> Self {
        Self::new(tokens.into_iter().map(SourceOrderElement::from))
    }

    /// Returns an iterator over the source-order storage elements.
    ///
    /// This is for reconstruction and exhaustive traversal infrastructure, not
    /// for grammar-aware analysis.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &SourceOrderElement> + '_ {
        self.elements.iter()
    }

    /// Appends exact source text for each element in source order.
    ///
    /// Panics when `source_text` does not contain any element range.
    pub(crate) fn write_source_text(
        &self,
        source_text: &str,
        writer: &mut dyn Write,
    ) -> fmt::Result {
        for element in self.iter() {
            element.write_source_text(source_text, writer)?;
        }

        Ok(())
    }
}

impl FromIterator<SourceOrderElement> for SourceOrderElements {
    fn from_iter<T: IntoIterator<Item = SourceOrderElement>>(iter: T) -> Self {
        Self::new(iter)
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{TextRange, TextSize};

    use super::{SourceOrderElement, SourceOrderElements};
    use crate::{SyntaxKind, SyntaxToken, SyntaxTrivia};

    #[test]
    fn elements_store_ordered_tokens() {
        let first = SyntaxToken::new(
            SyntaxKind::FuncKeyword,
            TextRange::new(TextSize::ZERO, TextSize::new(4)),
        );

        let second = SyntaxToken::end_of_file(TextSize::new(4));
        let elements = SourceOrderElements::from_tokens([first.clone(), second.clone()]);

        let stored_elements = elements.iter().cloned().collect::<Vec<_>>();

        assert_eq!(
            stored_elements,
            [
                SourceOrderElement::Token(first.clone()),
                SourceOrderElement::Token(second.clone())
            ]
        );

        assert_eq!(
            stored_elements.last(),
            Some(&SourceOrderElement::Token(second))
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

        let elements = SourceOrderElements::from_tokens([token]);

        let mut written_text = String::new();

        match elements.write_source_text(source_text, &mut written_text) {
            Ok(()) => {}
            Err(error) => panic!("writing element text should succeed: {error:?}"),
        }

        assert_eq!(written_text, source_text);
    }

    #[test]
    fn syntax_elements_are_send_and_sync() {
        assert_send_sync::<SourceOrderElement>();
        assert_send_sync::<SourceOrderElements>();
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
