use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};
use bray_source::{SourceId, SourceSpan, TextRange, TextSize};
use bray_syntax::SyntaxKind;

use crate::RenderedDiagnosticNoteKind;
use crate::renderer::DiagnosticRenderer;

#[test]
fn execution_guarantee_rejections_identify_the_clause() {
    for (keyword, spelling) in [
        (SyntaxKind::ExecutesKeyword, "executes"),
        (SyntaxKind::WhenKeyword, "when"),
    ] {
        let span = SourceSpan::new(SourceId::new(0), TextRange::empty(TextSize::ZERO));

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::CheckingExecutionGuaranteeUnsupported,
            SeverityKind::Error,
        )
        .with_primary_span(span)
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::InvalidDeclaration,
            span,
        ))
        .with_arg(DiagnosticArg::actual_syntax_kind(keyword));

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(
            rendered.message(),
            format!("Bray cannot yet support guarantees declared with the {spelling} keyword")
        );

        assert_eq!(rendered.primary_span(), Some(span));
        assert_eq!(rendered.labels().len(), 1);
        assert!(!rendered.labels()[0].message().is_empty());
    }
}

#[test]
fn execution_proof_failures_identify_the_promised_behavior_and_cause() {
    let span = SourceSpan::new(SourceId::new(0), TextRange::empty(TextSize::ZERO));

    for (kind, name, expected) in [
        (
            DiagnosticKind::CheckingUnknownExecutionProperty,
            Some("constant"),
            "unknown execution property 'constant'",
        ),
        (
            DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            Some("pure"),
            "this callable cannot establish its 'pure' execution guarantee",
        ),
        (
            DiagnosticKind::CheckingCircularExecutionGuarantee,
            None,
            "this callable's total execution guarantee depends on a recursive call or cleanup cycle",
        ),
    ] {
        let mut diagnostic = Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error)
            .with_primary_span(span)
            .with_label(DiagnosticLabel::secondary(
                DiagnosticLabelKind::ExecutionGuaranteeFailure,
                span,
            ));

        if let Some(name) = name {
            diagnostic = diagnostic.with_arg(DiagnosticArg::referenced_name(name));
        }

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(rendered.message(), expected);

        assert_eq!(
            rendered.labels()[0].message(),
            "this operation or its cleanup cannot establish the required execution guarantee"
        );
    }
}

#[test]
fn pattern_conditions_binding_diagnostic_explains_the_available_forms() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(7),
        DiagnosticKind::CheckingBindingInPatternTest,
        SeverityKind::Error,
    )
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::PatternTestMustNotBind,
    ));

    let rendered = DiagnosticRenderer::english().render(&diagnostic);

    assert_eq!(
        rendered.message(),
        "a `matches` pattern cannot introduce a binding"
    );

    let [note] = rendered.notes() else {
        panic!("expected binding help");
    };

    assert_eq!(note.rendered_kind(), RenderedDiagnosticNoteKind::Help);

    assert_eq!(
        note.message(),
        "replace the binding with `_`, or use `if let` or `while let` to use the matched value"
    );
}
