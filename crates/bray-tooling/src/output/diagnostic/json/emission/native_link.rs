use super::failure::{DiagnosticEmissionFieldJson, text_field};

pub(in crate::output::diagnostic::json) fn native_link_input_failure_context(
    failure: &bray_diagnostics::DiagnosticNativeLinkInputFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticNativeLinkInputFailure as Failure;

    match failure {
        Failure::UnsupportedStandardLibraryArtifact {
            path,
            artifact_kind,
        } => vec![
            text_field("artifact_path", path),
            text_field("artifact_kind", artifact_kind),
        ],
        Failure::InvalidStandardLibraryArtifact {
            path,
            input_kind,
            cause,
        } => vec![
            text_field("artifact_path", path),
            text_field("input_kind", input_kind),
            text_field("link_input_cause", cause),
        ],
        Failure::InvalidRequirement {
            name,
            link_kind,
            provenance,
        } => vec![
            text_field("link_name", name),
            text_field("link_kind", link_kind),
            text_field("provenance", provenance),
        ],
    }
}
