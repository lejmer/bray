use crate::id::SourceId;
use crate::text::{TextRange, TextSize};

/// Source snapshot identity plus a byte range inside that snapshot.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceSpan {
    source_id: SourceId,
    range: TextRange,
}

impl SourceSpan {
    /// Creates a span from a source snapshot identity and byte range.
    pub const fn new(source_id: SourceId, range: TextRange) -> Self {
        Self { source_id, range }
    }

    /// Creates an empty span at `offset`.
    pub const fn empty(source_id: SourceId, offset: TextSize) -> Self {
        Self {
            source_id,
            range: TextRange::empty(offset),
        }
    }

    /// Returns the source snapshot identity.
    pub const fn source_id(self) -> SourceId {
        self.source_id
    }

    /// Returns the byte range inside the source snapshot.
    pub const fn range(self) -> TextRange {
        self.range
    }

    /// Returns the start byte offset.
    pub const fn start(self) -> TextSize {
        self.range.start()
    }

    /// Returns the exclusive end byte offset.
    pub const fn end(self) -> TextSize {
        self.range.end()
    }

    /// Returns the byte length of the span.
    pub const fn len(self) -> TextSize {
        self.range.len()
    }

    /// Returns whether the span covers no bytes.
    pub const fn is_empty(self) -> bool {
        self.range.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::SourceSpan;
    use crate::{SourceId, TextRange, TextSize};

    #[test]
    fn source_spans_pair_source_snapshot_id_with_byte_range() {
        let source_id = SourceId::new(3);
        let range = TextRange::new(TextSize::new(2), TextSize::new(9));
        let span = SourceSpan::new(source_id, range);

        assert_eq!(span.source_id(), source_id);
        assert_eq!(span.range(), range);
        assert_eq!(span.start(), TextSize::new(2));
        assert_eq!(span.end(), TextSize::new(9));
        assert_eq!(span.len(), TextSize::new(7));
    }

    #[test]
    fn source_spans_are_compact_copyable_values() {
        assert_eq!(size_of::<SourceSpan>(), 12);

        let span = SourceSpan::empty(SourceId::new(1), TextSize::new(4));
        let copied = span;

        assert_eq!(copied, span);
        assert!(copied.is_empty());
    }
}
