use std::io::{self, Write};
use std::ops::Range;

use bray_messages::{DiagnosticRenderer, RenderedDiagnostic, RenderedDiagnosticNote};
use bray_source::{LineColumn, LineIndex, SourceLocation, SourceOrigin, SourceSnapshot};

use crate::diagnostic_output::source_map::ResolvedSourceSpan;
use crate::output_path::path_to_output_string;
use crate::terminal_style::{color_frame_text, color_note_heading, color_severity_label};

const SOURCE_FRAME_MAX_COLUMNS: usize = 88;
const SOURCE_FRAME_MAX_LINES: u32 = 6;
const SOURCE_FRAME_OMISSION: &str = "...";

pub(super) fn write_source_frame(
    renderer: DiagnosticRenderer,
    diagnostic: &RenderedDiagnostic,
    resolved: ResolvedSourceSpan<'_>,
    writer: &mut impl Write,
) -> io::Result<()> {
    let lines = frame_lines(resolved.snapshot, resolved.line_index, resolved.location);
    let gutter_width = gutter_width(&lines);
    let viewport = TextFrameViewport::new(&lines, resolved.location);

    writeln!(
        writer,
        "{}{}:{}",
        color_frame_text("at "),
        source_name(resolved.location.origin()),
        format_location_range(resolved.location)
    )?;

    writeln!(
        writer,
        "{}",
        color_frame_text(&blank_gutter(gutter_width, '|'))
    )?;

    for line in lines {
        match line {
            FrameLine::Source(line) => {
                write_source_line(line, gutter_width, viewport, writer)?;
                write_marker_line(
                    line,
                    gutter_width,
                    viewport,
                    resolved.location,
                    diagnostic,
                    writer,
                )?;
            }
            FrameLine::Omitted => {
                writeln!(
                    writer,
                    "{}",
                    color_frame_text(&format!(
                        "{SOURCE_FRAME_OMISSION:>gutter_width$} | {SOURCE_FRAME_OMISSION}"
                    ))
                )?;
            }
        }
    }

    write_frame_notes(renderer, diagnostic.notes(), gutter_width, writer)
}

fn write_source_line(
    line: SourceFrameLine<'_>,
    gutter_width: usize,
    viewport: TextFrameViewport,
    writer: &mut impl Write,
) -> io::Result<()> {
    let rendered = clipped_text(line.text, viewport);

    write!(
        writer,
        "{}",
        color_frame_text(&format!("{:>gutter_width$} | ", line.number))
    )?;

    writeln!(writer, "{rendered}")
}

fn write_marker_line(
    line: SourceFrameLine<'_>,
    gutter_width: usize,
    viewport: TextFrameViewport,
    location: SourceLocation<'_>,
    diagnostic: &RenderedDiagnostic,
    writer: &mut impl Write,
) -> io::Result<()> {
    let Some(marker) = marker_line(line, viewport, location) else {
        return Ok(());
    };

    write!(
        writer,
        "{}",
        color_frame_text(&format!("{:>gutter_width$} | ", ""))
    )?;

    writeln!(
        writer,
        "{}",
        color_severity_label(diagnostic.severity(), &marker)
    )
}

fn write_frame_notes(
    renderer: DiagnosticRenderer,
    notes: &[RenderedDiagnosticNote],
    gutter_width: usize,
    writer: &mut impl Write,
) -> io::Result<()> {
    for note in notes {
        write!(
            writer,
            "{}",
            color_frame_text(&format!("{:>gutter_width$} > ", ""))
        )?;

        writeln!(
            writer,
            "{}: {}",
            color_note_heading(renderer.render_note_heading(note.rendered_kind())),
            note.message()
        )?;
    }

    Ok(())
}

fn frame_lines<'source>(
    snapshot: &'source SourceSnapshot,
    line_index: &LineIndex,
    location: SourceLocation<'_>,
) -> Vec<FrameLine<'source>> {
    let start_line = location.start().line();
    let end_line = location.end().line();
    let line_count = end_line.saturating_sub(start_line).saturating_add(1);

    if line_count <= SOURCE_FRAME_MAX_LINES {
        return source_lines(snapshot, line_index, start_line..end_line.saturating_add(1));
    }

    let head_count = SOURCE_FRAME_MAX_LINES / 2;
    let tail_count = SOURCE_FRAME_MAX_LINES.saturating_sub(head_count);
    let tail_start = end_line.saturating_add(1).saturating_sub(tail_count);

    let mut lines = source_lines(
        snapshot,
        line_index,
        start_line..start_line.saturating_add(head_count),
    );

    lines.push(FrameLine::Omitted);

    lines.extend(source_lines(
        snapshot,
        line_index,
        tail_start..end_line.saturating_add(1),
    ));

    lines
}

fn source_lines<'source>(
    snapshot: &'source SourceSnapshot,
    line_index: &LineIndex,
    lines: Range<u32>,
) -> Vec<FrameLine<'source>> {
    lines
        .filter_map(|line_number| {
            source_line_text(snapshot, line_index, line_number)
                .map(|text| FrameLine::Source(SourceFrameLine::new(line_number, text)))
        })
        .collect()
}

fn source_line_text<'source>(
    snapshot: &'source SourceSnapshot,
    line_index: &LineIndex,
    line_number: u32,
) -> Option<&'source str> {
    let zero_based_line = line_number.checked_sub(1)?;
    let start = byte_index(line_index.line_start(zero_based_line)?)?;

    let end = match line_index.line_start(zero_based_line.checked_add(1)?) {
        Some(next_line_start) => byte_index(next_line_start)?,
        None => byte_index(snapshot.text_len())?,
    };

    let line = snapshot.text().get(start..end)?;

    Some(trim_line_break(line))
}

fn trim_line_break(text: &str) -> &str {
    text.strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .or_else(|| text.strip_suffix('\r'))
        .unwrap_or(text)
}

fn byte_index(offset: bray_source::TextSize) -> Option<usize> {
    usize::try_from(offset.bytes()).ok()
}

fn gutter_width(lines: &[FrameLine<'_>]) -> usize {
    lines.iter().map(FrameLine::gutter_width).max().unwrap_or(1)
}

fn blank_gutter(width: usize, marker: char) -> String {
    format!("{:>width$} {marker}", "")
}

fn decimal_digits(value: u32) -> usize {
    value.to_string().len()
}

fn source_name(origin: &SourceOrigin) -> String {
    match origin {
        SourceOrigin::File { path } => path_to_output_string(path),
        SourceOrigin::Virtual { name } => format!("<virtual:{name}>"),
        SourceOrigin::Generated { name } => format!("<generated:{name}>"),
        SourceOrigin::LspDocument { uri } => source_name_for_lsp_document(uri),
        SourceOrigin::TestFixture { name } => format!("<test-fixture:{name}>"),
        SourceOrigin::Stdin => String::from("<stdin>"),
    }
}

fn source_name_for_lsp_document(uri: &str) -> String {
    match SourceOrigin::path_from_document_uri(uri) {
        Ok(Some(path)) => path_to_output_string(&path),
        Ok(None) | Err(_) => uri.to_owned(),
    }
}

fn format_location_range(location: SourceLocation<'_>) -> String {
    let start = location.start();
    let end = location.end();

    if start == end {
        return format_position(start);
    }

    format!("{}..{}", format_position(start), format_position(end))
}

fn format_position(position: LineColumn) -> String {
    format!("{}:{}", position.line(), position.column())
}

fn clipped_text(text: &str, viewport: TextFrameViewport) -> String {
    let char_count = text.chars().count();
    let start = viewport.start_column.min(char_count);
    let end = viewport.end_column().min(char_count);

    let start_byte = byte_index_for_char(text, start);
    let end_byte = byte_index_for_char(text, end);

    let mut rendered = String::new();

    if start > 0 {
        rendered.push_str(SOURCE_FRAME_OMISSION);
    }

    if let Some(slice) = text.get(start_byte..end_byte) {
        rendered.push_str(slice);
    }

    if end < char_count {
        rendered.push_str(SOURCE_FRAME_OMISSION);
    }

    rendered
}

fn byte_index_for_char(text: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }

    match text.char_indices().nth(char_index) {
        Some((byte_index, _)) => byte_index,
        None => text.len(),
    }
}

fn marker_line(
    line: SourceFrameLine<'_>,
    viewport: TextFrameViewport,
    location: SourceLocation<'_>,
) -> Option<String> {
    let marker_range = marker_range(line, location);
    let visible_range = viewport.start_column..viewport.end_column();

    let start = marker_range.start.max(visible_range.start);
    let end = marker_range.end.min(visible_range.end);

    if end <= start {
        return None;
    }

    let mut marker = String::new();

    if viewport.start_column > 0 {
        marker.push_str(&" ".repeat(SOURCE_FRAME_OMISSION.len()));
    }

    marker.push_str(&" ".repeat(start.saturating_sub(visible_range.start)));
    marker.push_str(&"^".repeat(end.saturating_sub(start).max(1)));

    Some(marker)
}

fn marker_range(line: SourceFrameLine<'_>, location: SourceLocation<'_>) -> Range<usize> {
    let text_len = line.text.chars().count();
    let start_line = location.start().line();
    let end_line = location.end().line();

    let start = if line.number == start_line {
        zero_based_column(location.start())
    } else {
        0
    };

    let end = if line.number == end_line {
        zero_based_column(location.end()).saturating_add(1)
    } else {
        text_len
    };

    start..end.max(start.saturating_add(1))
}

fn zero_based_column(position: LineColumn) -> usize {
    usize::try_from(position.column().saturating_sub(1)).unwrap_or(usize::MAX)
}

#[derive(Clone, Copy, Debug)]
enum FrameLine<'source> {
    Source(SourceFrameLine<'source>),
    Omitted,
}

impl FrameLine<'_> {
    fn gutter_width(&self) -> usize {
        match self {
            Self::Source(line) => decimal_digits(line.number),
            Self::Omitted => SOURCE_FRAME_OMISSION.len(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SourceFrameLine<'source> {
    number: u32,
    text: &'source str,
}

impl<'source> SourceFrameLine<'source> {
    const fn new(number: u32, text: &'source str) -> Self {
        Self { number, text }
    }
}

#[derive(Clone, Copy, Debug)]
struct TextFrameViewport {
    start_column: usize,
}

impl TextFrameViewport {
    fn new(lines: &[FrameLine<'_>], location: SourceLocation<'_>) -> Self {
        let max_line_len = lines
            .iter()
            .filter_map(FrameLine::source)
            .map(|line| line.text.chars().count())
            .max()
            .unwrap_or(0);

        if max_line_len <= SOURCE_FRAME_MAX_COLUMNS {
            return Self { start_column: 0 };
        }

        let marker_range = marker_column_range(lines, location);

        let start_column = viewport_start_column(
            max_line_len,
            marker_range,
            zero_based_column(location.start()),
        );

        Self { start_column }
    }

    const fn end_column(self) -> usize {
        self.start_column.saturating_add(SOURCE_FRAME_MAX_COLUMNS)
    }
}

impl<'source> FrameLine<'source> {
    const fn source(&self) -> Option<SourceFrameLine<'source>> {
        match self {
            Self::Source(line) => Some(*line),
            Self::Omitted => None,
        }
    }
}

fn marker_column_range(lines: &[FrameLine<'_>], location: SourceLocation<'_>) -> Range<usize> {
    let mut start = usize::MAX;
    let mut end = 0;

    for line in lines.iter().filter_map(|line| line.source()) {
        let range = marker_range(line, location);

        start = start.min(range.start);
        end = end.max(range.end);
    }

    if start == usize::MAX {
        return 0..0;
    }

    start..end
}

fn viewport_start_column(
    max_line_len: usize,
    marker_range: Range<usize>,
    focus_column: usize,
) -> usize {
    let max_start = max_line_len.saturating_sub(SOURCE_FRAME_MAX_COLUMNS);
    let marker_width = marker_range.end.saturating_sub(marker_range.start);

    if marker_width <= SOURCE_FRAME_MAX_COLUMNS {
        return marker_range.start.saturating_sub(4).min(max_start);
    }

    focus_column
        .saturating_sub(SOURCE_FRAME_MAX_COLUMNS / 3)
        .min(max_start)
}
