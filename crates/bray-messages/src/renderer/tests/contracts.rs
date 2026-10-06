use bray_diagnostics::{Diagnostic, DiagnosticId, DiagnosticKind, SeverityKind};

use crate::renderer::DiagnosticRenderer;
use crate::{RenderedDiagnostic, RenderedDiagnosticLabel};

#[test]
fn renderer_exposes_missing_arguments_as_deterministic_placeholders() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::SourceTextTooLarge,
        SeverityKind::Error,
    );

    let rendered = DiagnosticRenderer::english().render(&diagnostic);

    assert_eq!(
        rendered.message(),
        "{source_input} is too large: {byte_count} bytes"
    );
}

#[test]
fn rendered_values_are_send_and_sync() {
    assert_send_sync::<DiagnosticRenderer>();
    assert_send_sync::<RenderedDiagnostic>();
    assert_send_sync::<RenderedDiagnosticLabel>();
}

fn assert_send_sync<T: Send + Sync>() {}
