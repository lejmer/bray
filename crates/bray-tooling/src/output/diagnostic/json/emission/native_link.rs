use super::failure::{
    DiagnosticEmissionFieldJson, DiagnosticEmissionFieldValueJson, field, text_field,
};

pub(in crate::output::diagnostic::json) fn native_link_input_failure_context(
    failure: &bray_diagnostics::DiagnosticNativeLinkInputFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticNativeLinkInputFailure as Failure;

    match failure {
        Failure::UnsupportedStandardLibraryArtifact {
            path,
            artifact_kind,
        } => vec![
            field(
                "artifact_path",
                DiagnosticEmissionFieldValueJson::Path(
                    super::super::DiagnosticPathJson::from_path(path),
                ),
            ),
            text_field("artifact_kind", *artifact_kind),
        ],
        Failure::InvalidStandardLibraryArtifact {
            path,
            input_kind,
            cause,
        } => vec![
            field(
                "artifact_path",
                DiagnosticEmissionFieldValueJson::Path(
                    super::super::DiagnosticPathJson::from_path(path),
                ),
            ),
            text_field("input_kind", *input_kind),
            text_field("link_input_cause", *cause),
        ],
        Failure::InvalidRequirement {
            name,
            link_kind,
            provenance_kind,
            provenance_identity,
        } => {
            let mut context = vec![
                text_field("link_name", name),
                text_field("link_kind", link_kind),
                text_field("provenance_kind", *provenance_kind),
            ];

            if let Some(identity) = provenance_identity {
                context.push(text_field("provenance_identity", identity));
            }

            context
        }
    }
}
