use bray_diagnostics::{
    DiagnosticFailureField, DiagnosticFailureValue, DiagnosticNativeProductFailureKind,
};

use super::context::{failure_detail, path_failure_field, text_failure_field};

pub(super) fn codegen_backend_failure_kind(
    error: &bray_codegen::CodegenFailure,
) -> DiagnosticNativeProductFailureKind {
    use DiagnosticNativeProductFailureKind as Kind;
    use bray_codegen::CodegenFailure as Error;

    match error {
        Error::UnsupportedTarget => Kind::CodegenBackendUnsupportedTarget,
        Error::UnsupportedTargetReport { report } => {
            Kind::CodegenBackendUnsupportedTargetDetail(failure_detail(
                "codegen_backend_unsupported_target_report",
                [text_failure_field("report", report.as_ref())],
            ))
        }
        Error::UnsupportedTargetValue { constraint, actual } => {
            Kind::CodegenBackendUnsupportedTargetDetail(failure_detail(
                "codegen_backend_unsupported_target_value",
                [
                    text_failure_field("constraint", *constraint),
                    text_failure_field("actual", actual.as_ref()),
                ],
            ))
        }
        Error::UnsupportedArtifact(artifact) => {
            Kind::CodegenBackendUnsupportedArtifact(failure_detail(
                "codegen_backend_unsupported_artifact",
                [text_failure_field(
                    "artifact_kind",
                    artifact.as_str(),
                )],
            ))
        }
        Error::InvalidConfiguration => Kind::CodegenBackendInvalidConfiguration,
        Error::InvalidConfigurationReport { report } => {
            Kind::CodegenBackendInvalidConfigurationDetail(failure_detail(
                "codegen_backend_invalid_configuration_report",
                [text_failure_field("report", report.as_ref())],
            ))
        }
        Error::ResourceExhausted => Kind::CodegenBackendResourceExhausted,
        Error::ResourceLimit { resource, actual } => {
            Kind::CodegenBackendResourceLimit(failure_detail(
                "codegen_backend_resource_limit",
                [
                    text_failure_field("resource", *resource),
                    text_failure_field("actual", actual.as_ref()),
                ],
            ))
        }
        Error::BackendLibrary { report } => Kind::CodegenBackendLibraryFailure(failure_detail(
            "codegen_backend_library_failure",
            [text_failure_field("report", report.as_ref())],
        )),
        Error::BackendToolExited { program, exit } => {
            Kind::CodegenBackendToolFailure(failure_detail(
                "codegen_backend_tool_failure",
                [
                    // The diagnostic must retain the exact executable path after this borrowed
                    // backend failure is released.
                    path_failure_field("program", program.clone()),
                    DiagnosticFailureField::new(
                        "exit",
                        // The diagnostic owns the captured process result after this borrowed
                        // planning error is released.
                        DiagnosticFailureValue::ExternalToolExit(exit.clone()),
                    ),
                ],
            ))
        }
        Error::ArtifactConstruction(artifact) => {
            Kind::CodegenBackendArtifactConstruction(failure_detail(
                "codegen_backend_artifact_construction",
                [text_failure_field(
                    "artifact_kind",
                    artifact.as_str(),
                )],
            ))
        }
        Error::ArtifactSerialization { artifact, report } => {
            Kind::CodegenBackendArtifactConstruction(failure_detail(
                "codegen_backend_artifact_serialization",
                [
                    text_failure_field("artifact_kind", artifact.as_str()),
                    text_failure_field("report", report.as_ref()),
                ],
            ))
        }
        Error::InvalidArtifactContent { artifact, cause } => {
            Kind::CodegenBackendArtifactConstruction(failure_detail(
                "codegen_backend_invalid_artifact_content",
                [
                    text_failure_field("artifact_kind", artifact.as_str()),
                    text_failure_field("cause", artifact_content_failure(*cause)),
                ],
            ))
        }
    }
}

const fn artifact_content_failure(error: bray_codegen::ArtifactContentBuildError) -> &'static str {
    match error {
        bray_codegen::ArtifactContentBuildError::LengthExceeded => "length_exceeded",
    }
}
