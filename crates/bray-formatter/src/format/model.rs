/// Formatted Bray source and whether formatting changed its text.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FormattedSource {
    pub(super) text: String,
    pub(super) changed: bool,
}

impl FormattedSource {
    /// Returns the formatted source text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns whether the formatted source differs from the input source.
    pub const fn changed(&self) -> bool {
        self.changed
    }

    /// Consumes the result and returns the formatted source text.
    pub fn into_text(self) -> String {
        self.text
    }

    pub(crate) fn with_leading_byte_order_mark(mut self) -> Self {
        self.text.insert(0, '\u{feff}');

        self
    }
}
