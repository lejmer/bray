use std::fmt::{self, Write};

use crate::syntax::SourceSyntaxNode;

/// Text-writing contract for syntax values that carry their source context.
pub trait SyntaxText {
    /// Appends the exact source text represented by this syntax value.
    fn write_full_text(&self, writer: &mut dyn Write) -> fmt::Result;

    /// Returns the exact source text represented by this syntax value.
    fn full_text(&self) -> String {
        text_from_writer(|writer| self.write_full_text(writer))
    }
}

impl<T> SyntaxText for T
where
    T: SourceSyntaxNode,
{
    fn write_full_text(&self, writer: &mut dyn Write) -> fmt::Result {
        self.write_full_text_from(self.source().text(), writer)
    }
}

pub(crate) fn text_from_writer(write_text: impl FnOnce(&mut dyn Write) -> fmt::Result) -> String {
    let mut text = String::new();

    match write_text(&mut text) {
        Ok(()) => {}
        Err(error) => panic!("writing syntax text should not fail for a String: {error:?}"),
    }

    text
}
