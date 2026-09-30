use super::failure::{DiagnosticEmissionFieldJson, text_field};

pub(in crate::output::diagnostic::json) fn native_link_input_failure_context(
    failure: &bray_diagnostics::DiagnosticNativeLinkInputFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticNativeLinkInputFailure as Failure;

    match failure {
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
