use std::ops::Range;

/// UTF-8 byte offset inside source text.
///
/// Compiler spans are represented by byte offsets. Line and column coordinates
/// are derived presentation data.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextSize(u32);

impl TextSize {
    /// Zero byte offset.
    pub const ZERO: Self = Self(0);

    /// Maximum representable byte offset.
    pub const MAX: Self = Self(u32::MAX);

    /// Creates a byte offset from a compact raw value.
    pub const fn new(bytes: u32) -> Self {
        Self(bytes)
    }

    /// Returns the raw byte offset.
    pub const fn bytes(self) -> u32 {
        self.0
    }

    /// Adds two byte offsets, returning `None` on overflow.
    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        match self.0.checked_add(rhs.0) {
            Some(bytes) => Some(Self(bytes)),
            None => None,
        }
    }
}

impl From<u32> for TextSize {
    fn from(bytes: u32) -> Self {
        Self::new(bytes)
    }
}

impl From<TextSize> for u32 {
    fn from(text_size: TextSize) -> Self {
        text_size.bytes()
    }
}

impl TryFrom<usize> for TextSize {
    type Error = TextSizeOverflow;

    fn try_from(bytes: usize) -> Result<Self, Self::Error> {
        match u32::try_from(bytes) {
            Ok(bytes) => Ok(Self::new(bytes)),
            Err(_) => Err(TextSizeOverflow::new(bytes)),
        }
    }
}

/// Error returned when a byte count cannot fit in [`TextSize`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TextSizeOverflow {
    bytes: usize,
}

impl TextSizeOverflow {
    /// Creates an overflow marker for an unrepresentable byte count.
    pub const fn new(bytes: usize) -> Self {
        Self { bytes }
    }

    /// Returns the byte count that could not fit in [`TextSize`].
    pub const fn bytes(self) -> usize {
        self.bytes
    }
}

/// Half-open UTF-8 byte range inside source text.
///
/// The start offset is included and the end offset is excluded.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextRange {
    start: TextSize,
    end: TextSize,
}

impl TextRange {
    /// Empty range at byte offset zero.
    pub const EMPTY: Self = Self {
        start: TextSize::ZERO,
        end: TextSize::ZERO,
    };

    /// Creates a half-open byte range.
    ///
    /// Panics if `start` is greater than `end`, which indicates a compiler
    /// invariant violation.
    pub const fn new(start: TextSize, end: TextSize) -> Self {
        assert!(start.bytes() <= end.bytes());

        Self { start, end }
    }

    /// Creates a range when `start <= end`.
    pub const fn try_new(start: TextSize, end: TextSize) -> Option<Self> {
        if start.bytes() <= end.bytes() {
            Some(Self { start, end })
        } else {
            None
        }
    }

    /// Creates an empty range at `offset`.
    pub const fn empty(offset: TextSize) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }

    /// Creates a range from a start offset and byte length.
    pub const fn with_len(start: TextSize, len: TextSize) -> Option<Self> {
        match start.checked_add(len) {
            Some(end) => Some(Self { start, end }),
            None => None,
        }
    }

    /// Returns the inclusive start byte offset.
    pub const fn start(self) -> TextSize {
        self.start
    }

    /// Returns the exclusive end byte offset.
    pub const fn end(self) -> TextSize {
        self.end
    }

    /// Returns the byte length of the range.
    pub const fn len(self) -> TextSize {
        TextSize::new(self.end.bytes() - self.start.bytes())
    }

    /// Returns whether the range covers no bytes.
    pub const fn is_empty(self) -> bool {
        self.start.bytes() == self.end.bytes()
    }

    /// Returns this range as a standard byte index range.
    pub fn to_usize_range(self) -> Option<Range<usize>> {
        let start = usize::try_from(self.start.bytes()).ok()?;
        let end = usize::try_from(self.end.bytes()).ok()?;

        Some(start..end)
    }

    /// Returns the string slice covered by this range.
    ///
    /// Returns `None` when the range is outside the text or does not align with
    /// UTF-8 scalar boundaries.
    pub fn slice_str(self, text: &str) -> Option<&str> {
        text.get(self.to_usize_range()?)
    }

    /// Returns the byte slice covered by this range.
    ///
    /// Unlike [`slice_str`](Self::slice_str), this only requires the range to be
    /// within bounds.
    pub fn slice_bytes(self, bytes: &[u8]) -> Option<&[u8]> {
        bytes.get(self.to_usize_range()?)
    }

    /// Returns whether `offset` is inside the half-open range.
    pub const fn contains(self, offset: TextSize) -> bool {
        self.start.bytes() <= offset.bytes() && offset.bytes() < self.end.bytes()
    }

    /// Returns whether `range` is fully inside this range.
    pub const fn contains_range(self, range: Self) -> bool {
        self.start.bytes() <= range.start.bytes() && range.end.bytes() <= self.end.bytes()
    }

    /// Returns the smallest range containing both ranges.
    pub const fn cover(self, range: Self) -> Self {
        let start = if self.start.bytes() <= range.start.bytes() {
            self.start
        } else {
            range.start
        };

        let end = if self.end.bytes() >= range.end.bytes() {
            self.end
        } else {
            range.end
        };

        Self { start, end }
    }
}

#[cfg(test)]
mod tests {
    use super::{TextRange, TextSize, TextSizeOverflow};

    #[test]
    fn text_sizes_count_utf8_bytes() {
        let text = "aé\n";
        let size = TextSize::try_from(text.len());

        assert_eq!(size, Ok(TextSize::new(4)));
    }

    #[test]
    fn text_size_reports_unrepresentable_byte_counts() {
        let too_large = usize::MAX;

        if u32::try_from(too_large).is_ok() {
            return;
        }

        assert_eq!(
            TextSize::try_from(too_large),
            Err(TextSizeOverflow::new(too_large))
        );
    }

    #[test]
    fn text_sizes_are_compact_copyable_wrappers() {
        assert_eq!(size_of::<TextSize>(), size_of::<u32>());

        let size = TextSize::new(11);
        let copied = size;

        assert_eq!(copied.bytes(), 11);
    }

    #[test]
    fn text_ranges_are_half_open_byte_ranges() {
        let range = TextRange::new(TextSize::new(3), TextSize::new(8));

        assert_eq!(range.start(), TextSize::new(3));
        assert_eq!(range.end(), TextSize::new(8));
        assert_eq!(range.len(), TextSize::new(5));

        assert!(range.contains(TextSize::new(3)));
        assert!(range.contains(TextSize::new(7)));
        assert!(!range.contains(TextSize::new(8)));
    }

    #[test]
    fn text_ranges_validate_offset_order() {
        let start = TextSize::new(9);
        let end = TextSize::new(4);

        assert_eq!(TextRange::try_new(start, end), None);
    }

    #[test]
    #[should_panic]
    fn text_range_new_panics_when_start_is_after_end() {
        let _ = TextRange::new(TextSize::new(9), TextSize::new(4));
    }

    #[test]
    fn text_ranges_can_be_built_from_byte_lengths() {
        assert_eq!(
            TextRange::with_len(TextSize::new(10), TextSize::new(5)),
            Some(TextRange::new(TextSize::new(10), TextSize::new(15)))
        );
        assert_eq!(TextRange::with_len(TextSize::MAX, TextSize::new(1)), None);
    }

    #[test]
    fn text_ranges_can_cover_and_contain_other_ranges() {
        let outer = TextRange::new(TextSize::new(2), TextSize::new(12));
        let inner = TextRange::new(TextSize::new(4), TextSize::new(8));
        let after = TextRange::new(TextSize::new(10), TextSize::new(16));

        assert!(outer.contains_range(inner));
        assert!(!outer.contains_range(after));

        assert_eq!(
            inner.cover(after),
            TextRange::new(TextSize::new(4), TextSize::new(16))
        );
    }

    #[test]
    fn text_ranges_slice_strings_on_utf8_boundaries() {
        let text = "aébc";
        let range = TextRange::new(TextSize::new(1), TextSize::new(3));

        assert_eq!(range.slice_str(text), Some("é"));

        assert_eq!(
            TextRange::new(TextSize::new(2), TextSize::new(3)).slice_str(text),
            None
        );
        assert_eq!(
            TextRange::new(TextSize::new(0), TextSize::new(99)).slice_str(text),
            None
        );
    }

    #[test]
    fn text_ranges_slice_bytes_by_raw_offsets() {
        let bytes = "aébc".as_bytes();

        assert_eq!(
            TextRange::new(TextSize::new(2), TextSize::new(4)).slice_bytes(bytes),
            Some(&bytes[2..4])
        );
        assert_eq!(
            TextRange::new(TextSize::new(0), TextSize::new(99)).slice_bytes(bytes),
            None
        );
    }

    #[test]
    fn text_ranges_are_compact_copyable_values() {
        assert_eq!(size_of::<TextRange>(), size_of::<u64>());

        let range = TextRange::empty(TextSize::new(5));
        let copied = range;

        assert_eq!(copied, range);
        assert!(copied.is_empty());
    }
}
