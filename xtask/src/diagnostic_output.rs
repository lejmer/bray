use bray_diagnostics::DiagnosticBag;
use bray_source::SourceStore;
use bray_tooling::{OutputFormat, write_diagnostics};

pub(crate) fn render_diagnostics(
    diagnostics: &DiagnosticBag,
    sources: &SourceStore,
) -> Option<String> {
    if diagnostics.is_empty() {
        return None;
    }

    let mut standard_output = Vec::new();
    let mut standard_error = Vec::new();

    write_diagnostics(
        diagnostics,
        Some(sources),
        OutputFormat::Text,
        &mut standard_output,
        &mut standard_error,
    )
    .ok()?;

    let rendered = String::from_utf8(standard_error).ok()?;
    let rendered = rendered.trim_end();

    (!rendered.is_empty()).then(|| rendered.to_owned())
}
