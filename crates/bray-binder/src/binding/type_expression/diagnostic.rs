use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    SeverityKind,
};
use bray_source::SourceSpan;
use bray_syntax::SourceSyntaxNode;

pub(super) fn callable_abi_diagnostic(
    syntax: &impl SourceSyntaxNode,
    kind: DiagnosticKind,
) -> Diagnostic {
    let span = SourceSpan::new(syntax.source().source_id(), syntax.full_range());

    let spelling = syntax
        .source()
        .text_slice(syntax.full_range())
        .unwrap_or_default();

    Diagnostic::new(
        DiagnosticId::new(span.range().start().bytes()),
        kind,
        SeverityKind::Error,
    )
    .with_primary_span(span)
    .with_arg(DiagnosticArg::token_text(spelling))
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::CallableAbiDirective,
        span,
    ))
}

pub(super) fn source_diagnostic(
    syntax: &impl SourceSyntaxNode,
    kind: DiagnosticKind,
) -> Diagnostic {
    let span = SourceSpan::new(syntax.source().source_id(), syntax.full_range());

    Diagnostic::new(
        DiagnosticId::new(span.range().start().bytes()),
        kind,
        SeverityKind::Error,
    )
    .with_primary_span(span)
}
