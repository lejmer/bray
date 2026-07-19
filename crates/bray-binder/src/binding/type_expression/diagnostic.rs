use bray_diagnostics::{Diagnostic, DiagnosticId, DiagnosticKind, SeverityKind};
use bray_source::SourceSpan;
use bray_syntax::SourceSyntaxNode;

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
