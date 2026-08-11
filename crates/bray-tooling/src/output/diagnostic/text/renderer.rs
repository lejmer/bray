use std::io::{self, Write};

use bray_diagnostics::DiagnosticBag;
use bray_messages::{
    DiagnosticRenderer, RenderedDiagnostic, RenderedDiagnosticLabel, RenderedDiagnosticNote,
    RenderedDiagnosticRelatedLocation, RenderedDiagnosticSuggestion,
};
use bray_source::SourceStore;

use super::super::source_map::DiagnosticSourceMap;
use super::frame::write_source_frame;
use crate::output::{color_bright_text, color_note_heading, color_severity_label};

pub(crate) fn write_text_diagnostics(
    diagnostics: &DiagnosticBag,
    sources: Option<&SourceStore>,
    writer: &mut impl Write,
) -> io::Result<()> {
    let renderer = DiagnosticRenderer::english();
    let source_map = DiagnosticSourceMap::new(sources);

    let mut first_diagnostic = true;

    for diagnostic in renderer.render_bag(diagnostics) {
        if !first_diagnostic {
            writeln!(writer)?;
        }

        write_text_diagnostic(renderer, &source_map, &diagnostic, writer)?;
        first_diagnostic = false;
    }

    Ok(())
}

fn write_text_diagnostic(
    renderer: DiagnosticRenderer,
    source_map: &DiagnosticSourceMap<'_>,
    diagnostic: &RenderedDiagnostic,
    writer: &mut impl Write,
) -> io::Result<()> {
    write_diagnostic_header(renderer, diagnostic, writer)?;

    let resolved = diagnostic
        .primary_span()
        .and_then(|span| source_map.resolve_source_span(span));

    match resolved {
        Some(resolved) => {
            writeln!(writer)?;

            write_source_frame(renderer, diagnostic, resolved, writer)?;
        }
        None => write_notes(renderer, diagnostic.notes(), writer)?,
    }

    write_labels(
        renderer,
        source_map,
        diagnostic.primary_span(),
        diagnostic.labels(),
        writer,
    )?;

    write_related_locations(renderer, source_map, diagnostic.related_locations(), writer)?;

    write_suggestions(renderer, diagnostic.suggestions(), writer)
}

fn write_labels(
    renderer: DiagnosticRenderer,
    source_map: &DiagnosticSourceMap<'_>,
    primary_span: Option<bray_source::SourceSpan>,
    labels: &[RenderedDiagnosticLabel],
    writer: &mut impl Write,
) -> io::Result<()> {
    for label in labels.iter().filter(|label| {
        label.style() == bray_diagnostics::DiagnosticLabelStyle::Secondary
            || Some(label.span()) != primary_span
    }) {
        let location = source_map
            .resolve(label.span())
            .map(|location| renderer.render_source_location(location))
            .unwrap_or_else(|| renderer.render_source_span(label.span()));

        writeln!(
            writer,
            "{}: {} at {}",
            color_note_heading(renderer.render_label_heading()),
            label.message(),
            location,
        )?;
    }

    Ok(())
}

fn write_related_locations(
    renderer: DiagnosticRenderer,
    source_map: &DiagnosticSourceMap<'_>,
    locations: &[RenderedDiagnosticRelatedLocation],
    writer: &mut impl Write,
) -> io::Result<()> {
    for related in locations {
        let location = source_map
            .resolve(related.span())
            .map(|location| renderer.render_source_location(location))
            .unwrap_or_else(|| renderer.render_source_span(related.span()));

        writeln!(
            writer,
            "{}: {} at {}",
            color_note_heading(renderer.render_related_location_heading()),
            related.message(),
            location,
        )?;
    }

    Ok(())
}

fn write_suggestions(
    renderer: DiagnosticRenderer,
    suggestions: &[RenderedDiagnosticSuggestion],
    writer: &mut impl Write,
) -> io::Result<()> {
    for suggestion in suggestions {
        writeln!(
            writer,
            "{}: {}",
            color_note_heading(
                renderer.render_note_heading(bray_messages::RenderedDiagnosticNoteKind::Help)
            ),
            suggestion.message(),
        )?;
    }

    Ok(())
}

fn write_diagnostic_header(
    renderer: DiagnosticRenderer,
    diagnostic: &RenderedDiagnostic,
    writer: &mut impl Write,
) -> io::Result<()> {
    let severity = renderer.render_severity(diagnostic.severity());
    let heading = format!("{severity} E{:04}", diagnostic.code().raw());
    let message = format!(": {}", diagnostic.message());

    writeln!(
        writer,
        "{}{}",
        color_severity_label(diagnostic.severity(), &heading),
        color_bright_text(&message)
    )
}

fn write_notes(
    renderer: DiagnosticRenderer,
    notes: &[RenderedDiagnosticNote],
    writer: &mut impl Write,
) -> io::Result<()> {
    for note in notes {
        writeln!(
            writer,
            "{}: {}",
            color_note_heading(renderer.render_note_heading(note.rendered_kind())),
            note.message()
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
        DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind, DiagnosticRelatedLocation,
        DiagnosticRelatedLocationKind, DiagnosticSourceInput, DiagnosticSourceInputOrigin,
        DiagnosticSuggestion, DiagnosticSuggestionKind, SeverityKind,
    };
    use bray_source::{SourceId, SourceSpan, SourceStore, TextRange, TextSize};

    use super::write_text_diagnostics;
    use crate::output::diagnostic::test_support::file_source_store;

    #[test]
    fn text_output_renders_colored_terminal_diagnostics() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::SourceInvalidUtf8,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::source_input(DiagnosticSourceInput::new(
            0,
            bray_source::SourceInputKind::File,
            DiagnosticSourceInputOrigin::File("main.bray".into()),
        )))
        .with_arg(DiagnosticArg::text_offset(TextSize::new(4)))
        .with_note(DiagnosticNote::new(DiagnosticNoteKind::SourceMustBeUtf8));

        let bag = DiagnosticBag::single(diagnostic);
        let output = render(&bag, None);

        assert!(output.contains("\x1b[31merror E1002\x1b[0m"));

        assert!(output.contains(
            "\x1b[97m: file source input 0 at main.bray contains invalid UTF-8 starting at byte offset 4\x1b[0m"
        ));

        assert!(output.contains("\x1b[97mhelp\x1b[0m: source inputs must be valid UTF-8"));
        assert!(!output.contains("source_invalid_utf8"));
    }

    #[test]
    fn text_output_resolves_source_spans_to_line_columns() {
        let sources = file_source_store("ok\n$");

        let span = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(3), TextSize::new(4)),
        );

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Error,
        )
        .with_primary_span(span)
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::InvalidCharacter,
            span,
        ));

        let bag = DiagnosticBag::single(diagnostic);
        let output = render(&bag, Some(&sources));

        assert!(output.contains("at \x1b[0mmain.bray:2:1"));
        assert!(output.contains("2 | "));
        assert!(output.contains("$"));
        assert!(!output.contains("source 0:3..4"));
    }

    #[test]
    fn text_output_renders_declaration_diagnostics() {
        let sources = file_source_store("struct Point {}\nstruct Point {}");

        let duplicate_span = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(16), TextSize::new(31)),
        );

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(16),
            DiagnosticKind::DeclarationDuplicateName,
            SeverityKind::Error,
        )
        .with_primary_span(duplicate_span)
        .with_arg(DiagnosticArg::declaration_name("Point"))
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::DuplicateDeclaration,
            duplicate_span,
        ));

        let output = render(&DiagnosticBag::single(diagnostic), Some(&sources));

        assert!(output.contains("error E4001"));
        assert!(output.contains("duplicate declaration of 'Point'"));
        assert!(output.contains("main.bray:2:1..2:15"));
    }

    #[test]
    fn text_output_renders_related_locations_labels_and_suggestions() {
        let sources = file_source_store("struct Point {}\nstruct Point {}");

        let first = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::ZERO, TextSize::new(15)),
        );

        let duplicate = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(16), TextSize::new(31)),
        );

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(16),
            DiagnosticKind::DeclarationDuplicateName,
            SeverityKind::Error,
        )
        .with_primary_span(duplicate)
        .with_arg(DiagnosticArg::declaration_name("Point"))
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::DuplicateDeclaration,
            duplicate,
        ))
        .with_related_location(DiagnosticRelatedLocation::new(
            DiagnosticRelatedLocationKind::FirstDeclaration,
            first,
        ))
        .with_suggestion(DiagnosticSuggestion::manual(
            DiagnosticSuggestionKind::FormatSource,
        ));

        let output = render(&DiagnosticBag::single(diagnostic), Some(&sources));

        assert!(
            output
                .lines()
                .any(|line| line.contains('^') && line.contains("duplicate declaration")),
            "expected the duplicate-declaration label on a source marker line:\n{output}"
        );

        assert!(output.contains("related\u{1b}[0m: first declared here at main.bray:1:1..1:15"));
        assert!(output.contains("help\u{1b}[0m: format this source with `bray fmt`"));
    }

    #[test]
    fn text_output_omits_label_list() {
        let sources = file_source_store("bad é");

        let primary_span = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::ZERO, TextSize::new(3)),
        );

        let secondary_span = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(4), TextSize::new(6)),
        );

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::LexicalInvalidIdentifier,
            SeverityKind::Error,
        )
        .with_primary_span(primary_span)
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::InvalidIdentifier,
            primary_span,
        ))
        .with_label(DiagnosticLabel::secondary(
            DiagnosticLabelKind::NonAsciiIdentifier,
            secondary_span,
        ));

        let bag = DiagnosticBag::single(diagnostic);
        let output = render(&bag, Some(&sources));

        assert!(output.contains("invalid identifier"));
        assert!(output.contains("main.bray:1:1..1:3"));
        assert!(output.contains("^^^"));
        assert!(!output.contains("primary source"));
        assert!(!output.contains("secondary source"));
    }

    #[test]
    fn text_output_renders_multiline_source_ranges() {
        let sources = file_source_store("first\nsecond\nthird");

        let span = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(2), TextSize::new(9)),
        );

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Error,
        )
        .with_primary_span(span);

        let bag = DiagnosticBag::single(diagnostic);
        let output = render(&bag, Some(&sources));

        assert!(output.contains("main.bray:1:3..2:3"));
        assert!(output.contains("1 | "));
        assert!(output.contains("first"));
        assert!(output.contains("2 | "));
        assert!(output.contains("second"));
    }

    #[test]
    fn text_output_aligns_omitted_source_frame_separator_with_source_lines() {
        let source = (1..=16)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");

        let source_len = match u32::try_from(source.len()) {
            Ok(source_len) => source_len,
            Err(error) => panic!("test source length should fit in TextSize: {error:?}"),
        };

        let sources = file_source_store(&source);

        let span = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::ZERO, TextSize::new(source_len)),
        );

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Error,
        )
        .with_primary_span(span);

        let bag = DiagnosticBag::single(diagnostic);
        let output = render(&bag, Some(&sources));

        let omitted_line = frame_line_containing(&output, "... | ...");
        let source_line = frame_line_containing(&output, "14 | ");

        assert_eq!(omitted_line.find('|'), source_line.find('|'));
    }

    #[test]
    fn text_output_clips_long_source_lines() {
        let long_source = format!("{}target{}", "a".repeat(120), "b".repeat(20));
        let sources = file_source_store(&long_source);

        let span = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(120), TextSize::new(126)),
        );

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Error,
        )
        .with_primary_span(span);

        let bag = DiagnosticBag::single(diagnostic);
        let output = render(&bag, Some(&sources));

        assert!(output.contains("..."));
        assert!(output.contains("target"));
        assert!(output.contains("^^^^^^"));
    }

    #[test]
    fn text_output_does_not_render_line_column_end_past_same_line_span() {
        let sources = file_source_store("abcdefghijkl");

        let span = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::ZERO, TextSize::new(12)),
        );

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Error,
        )
        .with_primary_span(span);

        let bag = DiagnosticBag::single(diagnostic);
        let output = render(&bag, Some(&sources));

        assert!(output.contains("main.bray:1:1..1:12"));
        assert!(!output.contains("main.bray:1:1..1:13"));
    }

    fn render(bag: &DiagnosticBag, sources: Option<&SourceStore>) -> String {
        let mut output = Vec::new();

        match write_text_diagnostics(bag, sources, &mut output) {
            Ok(()) => {}
            Err(error) => panic!("text diagnostics should write: {error:?}"),
        }

        match String::from_utf8(output) {
            Ok(output) => output,
            Err(error) => panic!("text diagnostics should be UTF-8: {error:?}"),
        }
    }

    fn frame_line_containing<'output>(output: &'output str, needle: &str) -> &'output str {
        match output.lines().find(|line| line.contains(needle)) {
            Some(line) => line,
            None => panic!("expected rendered output to contain {needle:?}:\n{output}"),
        }
    }
}
