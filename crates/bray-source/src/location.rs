use crate::id::SourceId;
use crate::line_index::{LineColumn, LineIndex, LspPosition};
use crate::origin::SourceOrigin;
use crate::snapshot::SourceSnapshot;
use crate::span::SourceSpan;
use crate::text::TextSize;

/// Line-column and LSP locations resolved from a source span.
///
/// Source locations are derived presentation data. Compiler data should store
/// [`SourceSpan`] and resolve it to `SourceLocation` only at boundaries that
/// need line, column, origin, or LSP coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceLocation<'source> {
    source_id: SourceId,
    origin: &'source SourceOrigin,
    span: SourceSpan,
    start: LineColumn,
    end: LineColumn,
    lsp_start: LspPosition,
    lsp_end: LspPosition,
}

impl<'source> SourceLocation<'source> {
    /// Resolves a source span through a source snapshot and line index.
    pub fn resolve(
        snapshot: &'source SourceSnapshot,
        line_index: &LineIndex,
        span: SourceSpan,
    ) -> Option<Self> {
        if snapshot.source_id() != span.source_id() {
            return None;
        }

        if span.end() > snapshot.text_len() || line_index.text_len() != snapshot.text_len() {
            return None;
        }

        let start = line_index.line_column(span.start())?;
        let end = line_column_end_position(snapshot, line_index, span)?;

        let lsp_start = line_index.lsp_position(span.start())?;
        let lsp_end = line_index.lsp_position(span.end())?;

        Some(Self {
            source_id: snapshot.source_id(),
            origin: snapshot.origin(),
            span,
            start,
            end,
            lsp_start,
            lsp_end,
        })
    }

    /// Returns the source snapshot identity.
    pub const fn source_id(self) -> SourceId {
        self.source_id
    }

    /// Returns the source origin.
    pub const fn origin(self) -> &'source SourceOrigin {
        self.origin
    }

    /// Returns the original source span.
    pub const fn span(self) -> SourceSpan {
        self.span
    }

    /// Returns the one-based source start line and column.
    pub const fn start(self) -> LineColumn {
        self.start
    }

    /// Returns the one-based source end line and column.
    ///
    /// For non-empty spans this is the last covered source position. The LSP
    /// end position remains exclusive.
    pub const fn end(self) -> LineColumn {
        self.end
    }

    /// Returns the zero-based LSP UTF-16 start position.
    pub const fn lsp_start(self) -> LspPosition {
        self.lsp_start
    }

    /// Returns the zero-based LSP UTF-16 end position.
    pub const fn lsp_end(self) -> LspPosition {
        self.lsp_end
    }
}

fn line_column_end_position(
    snapshot: &SourceSnapshot,
    line_index: &LineIndex,
    span: SourceSpan,
) -> Option<LineColumn> {
    if span.start() == span.end() {
        return line_index.line_column(span.end());
    }

    let start = usize::try_from(span.start().bytes()).ok()?;
    let end = usize::try_from(span.end().bytes()).ok()?;
    let text = snapshot.text().get(start..end)?;

    for (relative_offset, _) in text.char_indices().rev() {
        let offset = TextSize::try_from(start.checked_add(relative_offset)?).ok()?;

        if let Some(position) = line_index.line_column(offset) {
            return Some(position);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::SourceLocation;
    use crate::{
        LineColumn, LineIndex, LspPosition, SourceId, SourceIdentity, SourceOrigin, SourceSnapshot,
        SourceSpan, SourceVersion, TextRange, TextSize,
    };

    #[test]
    fn source_locations_resolve_line_column_and_lsp_positions() {
        let snapshot = snapshot(
            SourceId::new(3),
            SourceIdentity::new(5),
            SourceOrigin::file("main.bray"),
            SourceVersion::new(1),
            "aé\n𝄞b",
        );

        let line_index = line_index(snapshot.text());

        let span = SourceSpan::new(
            snapshot.source_id(),
            TextRange::new(TextSize::new(4), TextSize::new(8)),
        );

        let location = match SourceLocation::resolve(&snapshot, &line_index, span) {
            Some(location) => location,
            None => panic!("span should resolve inside snapshot"),
        };

        assert_eq!(location.source_id(), SourceId::new(3));

        assert_eq!(
            location.origin().file_path().and_then(|path| path.to_str()),
            Some("main.bray")
        );

        assert_eq!(location.span(), span);
        assert_eq!(location.start(), LineColumn::new(2, 1));
        assert_eq!(location.end(), LineColumn::new(2, 1));

        assert_eq!(location.lsp_start(), LspPosition::new(1, 0));
        assert_eq!(location.lsp_end(), LspPosition::new(1, 2));
    }

    #[test]
    fn source_locations_use_last_covered_line_column_end_position() {
        let snapshot = snapshot(
            SourceId::new(3),
            SourceIdentity::new(5),
            SourceOrigin::file("main.bray"),
            SourceVersion::new(1),
            "abc",
        );

        let line_index = line_index(snapshot.text());

        let span = SourceSpan::new(
            snapshot.source_id(),
            TextRange::new(TextSize::ZERO, TextSize::new(3)),
        );

        let location = match SourceLocation::resolve(&snapshot, &line_index, span) {
            Some(location) => location,
            None => panic!("span should resolve inside snapshot"),
        };

        assert_eq!(location.start(), LineColumn::new(1, 1));
        assert_eq!(location.end(), LineColumn::new(1, 3));
        assert_eq!(location.lsp_start(), LspPosition::new(0, 0));
        assert_eq!(location.lsp_end(), LspPosition::new(0, 3));
    }

    #[test]
    fn source_locations_can_cover_multiple_lines() {
        let snapshot = snapshot(
            SourceId::new(3),
            SourceIdentity::new(5),
            SourceOrigin::file("main.bray"),
            SourceVersion::new(1),
            "a\nbc",
        );

        let line_index = line_index(snapshot.text());

        let span = SourceSpan::new(
            snapshot.source_id(),
            TextRange::new(TextSize::ZERO, TextSize::new(4)),
        );

        let location = match SourceLocation::resolve(&snapshot, &line_index, span) {
            Some(location) => location,
            None => panic!("multi-line span should resolve inside snapshot"),
        };

        assert_eq!(location.start(), LineColumn::new(1, 1));
        assert_eq!(location.end(), LineColumn::new(2, 2));
        assert_eq!(location.lsp_start(), LspPosition::new(0, 0));
        assert_eq!(location.lsp_end(), LspPosition::new(1, 2));
    }

    #[test]
    fn empty_source_locations_use_the_insertion_position_as_line_column_end() {
        let snapshot = snapshot(
            SourceId::new(3),
            SourceIdentity::new(5),
            SourceOrigin::file("main.bray"),
            SourceVersion::new(1),
            "abc",
        );

        let line_index = line_index(snapshot.text());
        let span = SourceSpan::empty(snapshot.source_id(), TextSize::new(2));

        let location = match SourceLocation::resolve(&snapshot, &line_index, span) {
            Some(location) => location,
            None => panic!("empty span should resolve inside snapshot"),
        };

        assert_eq!(location.start(), LineColumn::new(1, 3));
        assert_eq!(location.end(), LineColumn::new(1, 3));
        assert_eq!(location.lsp_start(), LspPosition::new(0, 2));
        assert_eq!(location.lsp_end(), LspPosition::new(0, 2));
    }

    #[test]
    fn source_locations_reject_spans_for_other_sources() {
        let snapshot = snapshot(
            SourceId::new(3),
            SourceIdentity::new(5),
            SourceOrigin::stdin(),
            SourceVersion::new(1),
            "abc",
        );

        let line_index = line_index(snapshot.text());
        let span = SourceSpan::empty(SourceId::new(4), TextSize::ZERO);

        assert_eq!(SourceLocation::resolve(&snapshot, &line_index, span), None);
    }

    fn snapshot(
        source_id: SourceId,
        identity: SourceIdentity,
        origin: SourceOrigin,
        version: SourceVersion,
        text: &str,
    ) -> SourceSnapshot {
        match SourceSnapshot::new(source_id, identity, origin, version, text) {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test source should fit in TextSize: {error:?}"),
        }
    }

    fn line_index(text: &str) -> LineIndex {
        match LineIndex::new(text) {
            Ok(index) => index,
            Err(error) => panic!("test source should fit in TextSize: {error:?}"),
        }
    }
}
