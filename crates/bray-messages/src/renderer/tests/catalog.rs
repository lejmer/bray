use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind,
    DiagnosticLabel, DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, DiagnosticSourceInput,
    DiagnosticSourceInputOrigin, DiagnosticSuggestion, DiagnosticSuggestionKind, SeverityKind,
};
use bray_source::{SourceId, SourceSpan, TextRange, TextSize};

use crate::RenderedDiagnosticNoteKind;
use crate::catalog::{forbidden_internal_term, forbidden_ordinary_diagnostic_term};
use crate::renderer::DiagnosticRenderer;

#[test]
fn primary_templates_render_every_required_contract_argument() {
    let renderer = DiagnosticRenderer::english();

    for &kind in DiagnosticKind::ALL {
        let required = kind.quality_contract().primary_message_args();
        let rendered = renderer.diagnostic_argument_names(kind);

        assert!(
            required.iter().all(|name| rendered.contains(name)),
            "{kind:?} primary template omits required context: required {required:?}, rendered {rendered:?}",
        );
    }
}

#[test]
fn renderer_renders_every_diagnostic_kind() {
    let renderer = DiagnosticRenderer::english();

    for (index, &kind) in DiagnosticKind::ALL.iter().enumerate() {
        let id = u32::try_from(index)
            .map(DiagnosticId::new)
            .unwrap_or_else(|_| panic!("diagnostic inventory must fit diagnostic IDs"));

        let span = SourceSpan::new(
            SourceId::new(1),
            TextRange::new(TextSize::new(2), TextSize::new(3)),
        );

        let diagnostic = Diagnostic::new(id, kind, SeverityKind::Error)
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::InvalidCharacter,
                span,
            ))
            .with_note(DiagnosticNote::new(
                DiagnosticNoteKind::SourceFileMustBeReadable,
            ))
            .with_related_location(DiagnosticRelatedLocation::new(
                DiagnosticRelatedLocationKind::FirstDeclaration,
                span,
            ))
            .with_suggestion(DiagnosticSuggestion::manual(
                DiagnosticSuggestionKind::FormatSource,
            ));

        let rendered = renderer.render(&diagnostic);

        assert_eq!(rendered.kind(), kind);
        assert!(!rendered.message().is_empty(), "{kind:?}");

        for (component, message) in std::iter::once(("primary", rendered.message()))
            .chain(
                rendered
                    .labels()
                    .iter()
                    .map(|label| ("label", label.message())),
            )
            .chain(rendered.notes().iter().map(|note| ("note", note.message())))
            .chain(
                rendered
                    .related_locations()
                    .iter()
                    .map(|location| ("related location", location.message())),
            )
            .chain(
                rendered
                    .suggestions()
                    .iter()
                    .map(|suggestion| ("suggestion", suggestion.message())),
            )
        {
            assert!(
                !message.contains([';', '—']),
                "{kind:?} {component} uses prohibited prose punctuation: {message:?}"
            );

            let forbidden = if kind.as_str().starts_with("inspection_") {
                forbidden_internal_term(message)
            } else {
                forbidden_ordinary_diagnostic_term(message)
            };

            assert_eq!(
                forbidden, None,
                "{kind:?} {component} exposes compiler implementation language: {message:?}"
            );
        }
    }
}

#[test]
fn primary_messages_never_contain_recovery_instructions() {
    let renderer = DiagnosticRenderer::english();

    for &kind in DiagnosticKind::ALL {
        assert!(
            !renderer.primary_message_contains_recovery_instruction(kind),
            "{kind:?} must put recovery guidance in a typed note or suggestion"
        );
    }
}

#[test]
fn renderer_renders_diagnostic_messages_from_structured_catalog() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(7),
        DiagnosticKind::SourceFileReadFailed,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::source_input(DiagnosticSourceInput::new(
        0,
        bray_source::SourceInputKind::File,
        DiagnosticSourceInputOrigin::File("main.bray".into()),
    )))
    .with_arg(DiagnosticArg::io_error_kind(
        DiagnosticIoErrorKind::NotFound,
    ))
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::SourceFileMustBeReadable,
    ));

    let rendered = DiagnosticRenderer::english().render(&diagnostic);

    assert_eq!(rendered.id(), DiagnosticId::new(7));
    assert_eq!(rendered.kind(), DiagnosticKind::SourceFileReadFailed);
    assert_eq!(rendered.severity(), SeverityKind::Error);

    assert_eq!(
        rendered.message(),
        "could not read file source input 0 at main.bray: not found"
    );

    let [note] = rendered.notes() else {
        panic!("expected one rendered note: {rendered:?}");
    };

    assert_eq!(note.kind(), DiagnosticNoteKind::SourceFileMustBeReadable);
    assert_eq!(note.rendered_kind(), RenderedDiagnosticNoteKind::Help);

    assert_eq!(
        note.message(),
        "source files must be readable before compilation"
    );
}
