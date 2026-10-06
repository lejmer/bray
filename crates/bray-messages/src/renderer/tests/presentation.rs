use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticBag, DiagnosticId,
    DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind, DiagnosticLabelStyle, SeverityKind,
};
use bray_source::{SourceId, SourceSpan, TextRange, TextSize};

use crate::renderer::DiagnosticRenderer;
use crate::{RenderedDiagnostic, RenderedDiagnosticNoteKind};

#[test]
fn renderer_renders_labels_with_spans_styles_and_typed_args() {
    let span = SourceSpan::new(
        SourceId::new(1),
        TextRange::new(TextSize::new(3), TextSize::new(4)),
    );

    let label = DiagnosticLabel::primary(DiagnosticLabelKind::InvalidCharacter, span).with_arg(
        DiagnosticArg::new(
            DiagnosticArgName::Character,
            DiagnosticArgValue::Character('\u{7f}'),
        ),
    );

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::LexicalInvalidCharacter,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::new(
        DiagnosticArgName::Character,
        DiagnosticArgValue::Character('\u{7f}'),
    ))
    .with_label(label);

    let rendered = DiagnosticRenderer::english().render(&diagnostic);

    assert_eq!(rendered.message(), "invalid character U+007F");

    let [label] = rendered.labels() else {
        panic!("expected one rendered label: {rendered:?}");
    };

    assert_eq!(label.kind(), DiagnosticLabelKind::InvalidCharacter);
    assert_eq!(label.style(), DiagnosticLabelStyle::Primary);
    assert_eq!(label.span(), span);
    assert_eq!(label.message(), "invalid character U+007F");
}

#[test]
fn renderer_keeps_diagnostic_bag_order() {
    let first = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::RequestMissingSourceInput,
        SeverityKind::Error,
    );

    let second = Diagnostic::new(
        DiagnosticId::new(1),
        DiagnosticKind::RequestInvalidWorkerBudget,
        SeverityKind::Error,
    );

    let bag = DiagnosticBag::from(vec![first, second]);
    let rendered = DiagnosticRenderer::english().render_bag(&bag);

    let ids = rendered
        .iter()
        .map(RenderedDiagnostic::id)
        .collect::<Vec<_>>();

    assert_eq!(ids, vec![DiagnosticId::new(0), DiagnosticId::new(1)]);
}

#[test]
fn renderer_formats_terminal_output_headings() {
    let renderer = DiagnosticRenderer::english();

    assert_eq!(renderer.render_severity(SeverityKind::Error), "error");

    assert_eq!(
        renderer.render_label_style(DiagnosticLabelStyle::Secondary),
        "secondary source"
    );

    assert_eq!(
        renderer.render_note_heading(RenderedDiagnosticNoteKind::Note),
        "note"
    );

    assert_eq!(
        renderer.render_note_heading(RenderedDiagnosticNoteKind::Help),
        "help"
    );
}
