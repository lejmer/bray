use crate::newline::{SourceLineBreakKind, SourceNewlinePolicy};
use crate::text::{TextRange, TextSize, TextSizeOverflow};

/// One-based source line and Unicode-scalar column.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LineColumn {
    line: u32,
    column: u32,
}

impl LineColumn {
    /// Creates a one-based line and column pair.
    ///
    /// Panics if either value is zero, which indicates a compiler invariant
    /// violation.
    pub const fn new(line: u32, column: u32) -> Self {
        assert!(line > 0);
        assert!(column > 0);

        Self { line, column }
    }

    /// Returns the one-based line number.
    pub const fn line(self) -> u32 {
        self.line
    }

    /// Returns the one-based Unicode-scalar column.
    pub const fn column(self) -> u32 {
        self.column
    }
}

/// Zero-based LSP position using UTF-16 code units for the character value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LspPosition {
    line: u32,
    character: u32,
}

impl LspPosition {
    /// Creates a zero-based LSP position.
    pub const fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }

    /// Returns the zero-based LSP line.
    pub const fn line(self) -> u32 {
        self.line
    }

    /// Returns the zero-based LSP UTF-16 character offset.
    pub const fn character(self) -> u32 {
        self.character
    }
}

/// Derived index that maps UTF-8 byte offsets to line-column and LSP positions.
///
/// The index preserves source text offsets exactly. LF and CRLF are accepted
/// source line breaks. A lone CR is still indexed as a recovery line break so
/// diagnostics after an invalid line ending stay aligned with editor line
/// numbers.
#[derive(Debug, Eq, PartialEq)]
pub struct LineIndex {
    text_len: TextSize,
    lines: Vec<LineMetrics>,
}

impl LineIndex {
    /// Builds a line index for UTF-8 source text.
    pub fn new(text: &str) -> Result<Self, TextSizeOverflow> {
        let text_len = TextSize::try_from(text.len())?;
        let newline_policy = SourceNewlinePolicy::DEFAULT;

        let mut lines = Vec::new();
        let mut current_line = LineMetrics::new(TextSize::ZERO);
        let mut characters = text.char_indices().peekable();

        while let Some((byte_index, character)) = characters.next() {
            let offset = TextSize::try_from(byte_index)?;
            let next_character = characters.peek().map(|(_, next_character)| *next_character);

            match source_line_break(newline_policy, character, next_character) {
                Some(line_break) => {
                    current_line.end = offset;
                    lines.push(current_line);

                    if line_break.consumes_next_character {
                        let _line_feed = characters.next();
                    }

                    let next_line_start = TextSize::try_from(byte_index + line_break.byte_len)?;
                    current_line = LineMetrics::new(next_line_start);
                }
                None => current_line.push_character(offset, character),
            }
        }

        current_line.end = text_len;

        lines.push(current_line);

        Ok(Self { text_len, lines })
    }

    /// Returns the indexed source text byte length.
    pub const fn text_len(&self) -> TextSize {
        self.text_len
    }

    /// Returns the number of lines in the indexed text.
    pub const fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Returns the start byte offset for a zero-based line.
    pub fn line_start(&self, line: u32) -> Option<TextSize> {
        let index = usize::try_from(line).ok()?;

        self.lines.get(index).map(|metrics| metrics.start)
    }

    /// Returns the one-based source line and column for a byte offset.
    pub fn line_column(&self, offset: TextSize) -> Option<LineColumn> {
        self.position(offset).map(|position| position.line_column)
    }

    /// Returns the zero-based LSP UTF-16 position for a byte offset.
    pub fn lsp_position(&self, offset: TextSize) -> Option<LspPosition> {
        self.position(offset).map(|position| position.lsp_position)
    }

    /// Returns the UTF-8 byte offset for a zero-based LSP UTF-16 position.
    ///
    /// Returns `None` when the line does not exist, the UTF-16 character value
    /// falls inside a surrogate pair, or the position is beyond the line end.
    pub fn offset_for_lsp_position(&self, position: LspPosition) -> Option<TextSize> {
        let line_index = usize::try_from(position.line()).ok()?;
        let metrics = self.lines.get(line_index)?;

        metrics.offset_for_lsp_character(position.character())
    }

    /// Returns the source text range covered by a zero-based LSP UTF-16 range.
    pub fn text_range_for_lsp_range(
        &self,
        start: LspPosition,
        end: LspPosition,
    ) -> Option<TextRange> {
        if start > end {
            return None;
        }

        TextRange::try_new(
            self.offset_for_lsp_position(start)?,
            self.offset_for_lsp_position(end)?,
        )
    }

    fn position(&self, offset: TextSize) -> Option<ResolvedPosition> {
        if offset > self.text_len {
            return None;
        }

        let line_index = self.line_index_for_offset(offset)?;
        let metrics = self.lines.get(line_index)?;
        let line_number = u32::try_from(line_index).ok()?;

        metrics.position(offset, line_number)
    }

    fn line_index_for_offset(&self, offset: TextSize) -> Option<usize> {
        match self.lines.binary_search_by_key(&offset, |line| line.start) {
            Ok(index) => Some(index),
            Err(0) => None,
            Err(index) => Some(index - 1),
        }
    }
}

fn source_line_break(
    newline_policy: SourceNewlinePolicy,
    character: char,
    next_character: Option<char>,
) -> Option<IndexedLineBreak> {
    if character == '\r' && next_character != Some('\n') {
        return Some(IndexedLineBreak::new(1, false));
    }

    let line_break_kind = newline_policy.line_break_kind(character, next_character)?;
    let consumes_next_character = line_break_kind == SourceLineBreakKind::CarriageReturnLineFeed;

    Some(IndexedLineBreak::new(
        line_break_kind.byte_len(),
        consumes_next_character,
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct IndexedLineBreak {
    byte_len: usize,
    consumes_next_character: bool,
}

impl IndexedLineBreak {
    const fn new(byte_len: usize, consumes_next_character: bool) -> Self {
        Self {
            byte_len,
            consumes_next_character,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct LineMetrics {
    start: TextSize,
    end: TextSize,
    character_offsets: Vec<TextSize>,
    utf16_columns: Vec<u32>,
    utf16_len: u32,
}

impl LineMetrics {
    fn new(start: TextSize) -> Self {
        Self {
            start,
            end: start,
            character_offsets: Vec::new(),
            utf16_columns: Vec::new(),
            utf16_len: 0,
        }
    }

    fn push_character(&mut self, offset: TextSize, character: char) {
        self.character_offsets.push(offset);
        self.utf16_columns.push(self.utf16_len);

        let units = if character.len_utf16() == 1 { 1 } else { 2 };

        // UTF-16 units for a scalar never exceed its UTF-8 byte length, and
        // source text byte length already fits in `TextSize`.
        self.utf16_len += units;
    }

    fn position(&self, offset: TextSize, line_number: u32) -> Option<ResolvedPosition> {
        if offset < self.start || offset > self.end {
            return None;
        }

        let character_index = match self.character_offsets.binary_search(&offset) {
            Ok(index) => index,
            Err(index) if offset == self.end => index,
            Err(_) => return None,
        };

        let zero_based_column = u32::try_from(character_index).ok()?;

        let line_column = LineColumn::new(
            line_number.checked_add(1)?,
            zero_based_column.checked_add(1)?,
        );

        let utf16_column = match self.utf16_columns.get(character_index) {
            Some(column) => *column,
            None => self.utf16_len,
        };

        let lsp_position = LspPosition::new(line_number, utf16_column);

        Some(ResolvedPosition {
            line_column,
            lsp_position,
        })
    }

    fn offset_for_lsp_character(&self, character: u32) -> Option<TextSize> {
        match self.utf16_columns.binary_search(&character) {
            Ok(index) => self.character_offsets.get(index).copied(),
            Err(index) if index == self.utf16_columns.len() && character == self.utf16_len => {
                Some(self.end)
            }
            Err(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ResolvedPosition {
    line_column: LineColumn,
    lsp_position: LspPosition,
}

#[cfg(test)]
mod tests {
    use super::{LineColumn, LineIndex, LspPosition};
    use crate::{TextRange, TextSize};

    #[test]
    fn line_index_maps_utf8_offsets_to_line_columns() {
        let index = line_index("aé\n𝄞b");

        assert_eq!(index.line_count(), 2);
        assert_eq!(index.line_start(0), Some(TextSize::new(0)));
        assert_eq!(index.line_start(1), Some(TextSize::new(4)));

        assert_eq!(
            index.line_column(TextSize::new(0)),
            Some(LineColumn::new(1, 1))
        );
        assert_eq!(
            index.line_column(TextSize::new(1)),
            Some(LineColumn::new(1, 2))
        );
        assert_eq!(
            index.line_column(TextSize::new(3)),
            Some(LineColumn::new(1, 3))
        );
        assert_eq!(
            index.line_column(TextSize::new(4)),
            Some(LineColumn::new(2, 1))
        );
        assert_eq!(
            index.line_column(TextSize::new(8)),
            Some(LineColumn::new(2, 2))
        );
        assert_eq!(
            index.line_column(TextSize::new(9)),
            Some(LineColumn::new(2, 3))
        );
    }

    #[test]
    fn line_index_maps_offsets_to_lsp_utf16_positions() {
        let index = line_index("aé\n𝄞b");

        assert_eq!(
            index.lsp_position(TextSize::new(0)),
            Some(LspPosition::new(0, 0))
        );
        assert_eq!(
            index.lsp_position(TextSize::new(1)),
            Some(LspPosition::new(0, 1))
        );
        assert_eq!(
            index.lsp_position(TextSize::new(3)),
            Some(LspPosition::new(0, 2))
        );
        assert_eq!(
            index.lsp_position(TextSize::new(4)),
            Some(LspPosition::new(1, 0))
        );
        assert_eq!(
            index.lsp_position(TextSize::new(8)),
            Some(LspPosition::new(1, 2))
        );
        assert_eq!(
            index.lsp_position(TextSize::new(9)),
            Some(LspPosition::new(1, 3))
        );
    }

    #[test]
    fn line_index_maps_lsp_utf16_positions_to_offsets() {
        let index = line_index("a𝄞b\né");

        assert_eq!(
            index.offset_for_lsp_position(LspPosition::new(0, 0)),
            Some(TextSize::new(0))
        );
        assert_eq!(
            index.offset_for_lsp_position(LspPosition::new(0, 1)),
            Some(TextSize::new(1))
        );
        assert_eq!(index.offset_for_lsp_position(LspPosition::new(0, 2)), None);
        assert_eq!(
            index.offset_for_lsp_position(LspPosition::new(0, 3)),
            Some(TextSize::new(5))
        );
        assert_eq!(
            index.offset_for_lsp_position(LspPosition::new(0, 4)),
            Some(TextSize::new(6))
        );
        assert_eq!(
            index.offset_for_lsp_position(LspPosition::new(1, 0)),
            Some(TextSize::new(7))
        );
        assert_eq!(
            index.offset_for_lsp_position(LspPosition::new(1, 1)),
            Some(TextSize::new(9))
        );
    }

    #[test]
    fn line_index_maps_lsp_utf16_ranges_to_text_ranges() {
        let index = line_index("a𝄞b\né");

        assert_eq!(
            index.text_range_for_lsp_range(LspPosition::new(0, 1), LspPosition::new(0, 3)),
            Some(TextRange::new(TextSize::new(1), TextSize::new(5)))
        );
        assert_eq!(
            index.text_range_for_lsp_range(LspPosition::new(0, 2), LspPosition::new(0, 3)),
            None
        );
        assert_eq!(
            index.text_range_for_lsp_range(LspPosition::new(1, 0), LspPosition::new(0, 0)),
            None
        );
    }

    #[test]
    fn line_index_rejects_offsets_inside_utf8_codepoints() {
        let index = line_index("aé\n𝄞b");

        assert_eq!(index.line_column(TextSize::new(2)), None);
        assert_eq!(index.lsp_position(TextSize::new(6)), None);
    }

    #[test]
    fn line_index_tracks_empty_final_line_after_newline() {
        let index = line_index("a\n");

        assert_eq!(index.line_count(), 2);

        assert_eq!(
            index.line_column(TextSize::new(2)),
            Some(LineColumn::new(2, 1))
        );
        assert_eq!(
            index.lsp_position(TextSize::new(2)),
            Some(LspPosition::new(1, 0))
        );
    }

    #[test]
    fn line_index_treats_crlf_as_one_line_break() {
        let index = line_index("a\r\nb");

        assert_eq!(index.line_count(), 2);

        assert_eq!(
            index.line_column(TextSize::new(1)),
            Some(LineColumn::new(1, 2))
        );
        assert_eq!(
            index.line_column(TextSize::new(3)),
            Some(LineColumn::new(2, 1))
        );
        assert_eq!(
            index.lsp_position(TextSize::new(3)),
            Some(LspPosition::new(1, 0))
        );
    }

    #[test]
    fn line_index_recovers_lone_cr_as_a_line_break() {
        let index = line_index("a\rb");

        assert_eq!(index.line_count(), 2);

        assert_eq!(
            index.line_column(TextSize::new(1)),
            Some(LineColumn::new(1, 2))
        );

        assert_eq!(
            index.line_column(TextSize::new(2)),
            Some(LineColumn::new(2, 1))
        );

        assert_eq!(
            index.lsp_position(TextSize::new(2)),
            Some(LspPosition::new(1, 0))
        );
    }

    #[test]
    fn line_index_handles_mixed_newline_spellings() {
        let index = line_index("a\nb\r\nc\rd");

        assert_eq!(index.line_count(), 4);
        assert_eq!(index.line_start(0), Some(TextSize::new(0)));
        assert_eq!(index.line_start(1), Some(TextSize::new(2)));
        assert_eq!(index.line_start(2), Some(TextSize::new(5)));
        assert_eq!(index.line_start(3), Some(TextSize::new(7)));

        assert_eq!(
            index.line_column(TextSize::new(5)),
            Some(LineColumn::new(3, 1))
        );

        assert_eq!(
            index.line_column(TextSize::new(6)),
            Some(LineColumn::new(3, 2))
        );

        assert_eq!(
            index.line_column(TextSize::new(7)),
            Some(LineColumn::new(4, 1))
        );
    }

    fn line_index(text: &str) -> LineIndex {
        match LineIndex::new(text) {
            Ok(index) => index,
            Err(error) => panic!("test source should fit in TextSize: {error:?}"),
        }
    }
}
