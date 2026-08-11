use super::super::source::{
    format_english_path, format_english_quoted_text, format_english_source_input_kind,
};
pub(crate) fn format_english_source_input(
    input: &bray_diagnostics::DiagnosticSourceInput,
) -> String {
    let kind = format_english_source_input_kind(input.kind());

    let origin = match input.origin() {
        bray_diagnostics::DiagnosticSourceInputOrigin::File(path) => {
            format!(" at {}", format_english_path(path))
        }
        bray_diagnostics::DiagnosticSourceInputOrigin::Name(name) => {
            format!(" named {}", format_english_quoted_text(name))
        }
        bray_diagnostics::DiagnosticSourceInputOrigin::Uri(uri) => {
            format!(" for URI {}", format_english_quoted_text(uri))
        }
        bray_diagnostics::DiagnosticSourceInputOrigin::Missing => {
            " without a stable origin".to_owned()
        }
    };

    format!("{kind} source input {}{origin}", input.index())
}
