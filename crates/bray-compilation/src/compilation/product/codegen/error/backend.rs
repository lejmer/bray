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
                    backend_artifact_kind(*artifact),
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
        Error::GeneratedModuleInvariant => Kind::CodegenBackendGeneratedModuleInvariant,
        Error::GeneratedModuleInvariantDetail { report } => {
            Kind::CodegenBackendGeneratedModuleInvariantDetail(failure_detail(
                "codegen_backend_generated_module_invariant_detail",
                [text_failure_field("report", report.as_ref())],
            ))
        }
        Error::CompilerOwnedRuntimeRole(role) => {
            Kind::CodegenBackendGeneratedModuleInvariantDetail(failure_detail(
                "codegen_backend_compiler_owned_runtime_role",
                [text_failure_field("role", role.as_str())],
            ))
        }
        Error::InvalidRuntimeMetadata(error) => {
            Kind::CodegenBackendInvalidRuntimeMetadata(runtime_metadata_failure_detail(*error))
        }
        Error::InvalidOutcome(error) => {
            Kind::CodegenBackendInvalidOutcome(outcome_failure_detail(error))
        }
        Error::BackendRejectedModule { stage, report } => {
            Kind::CodegenBackendRejectedModule(failure_detail(
                "codegen_backend_rejected_module",
                [
                    text_failure_field("verification_stage", stage.as_str()),
                    text_failure_field("report", report.as_ref()),
                ],
            ))
        }
        Error::ArtifactConstruction(artifact) => {
            Kind::CodegenBackendArtifactConstruction(failure_detail(
                "codegen_backend_artifact_construction",
                [text_failure_field(
                    "artifact_kind",
                    backend_artifact_kind(*artifact),
                )],
            ))
        }
        Error::ArtifactSerialization { artifact, report } => {
            Kind::CodegenBackendArtifactConstruction(failure_detail(
                "codegen_backend_artifact_serialization",
                [
                    text_failure_field("artifact_kind", backend_artifact_kind(*artifact)),
                    text_failure_field("report", report.as_ref()),
                ],
            ))
        }
        Error::InvalidArtifactContent { artifact, cause } => {
            Kind::CodegenBackendArtifactConstruction(failure_detail(
                "codegen_backend_invalid_artifact_content",
                [
                    text_failure_field("artifact_kind", backend_artifact_kind(*artifact)),
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

fn runtime_metadata_failure_detail(
    error: bray_codegen::CodegenRuntimeMetadataBuildError,
) -> bray_diagnostics::DiagnosticNativeProductFailureDetail {
    use bray_codegen::CodegenRuntimeMetadataBuildError as Error;

    match error {
        Error::MultipleExecutableHosts => {
            failure_detail("codegen_backend_runtime_metadata_multiple_hosts", [])
        }
        Error::DuplicateFrame(frame) => failure_detail(
            "codegen_backend_runtime_metadata_duplicate_frame",
            [text_failure_field("frame", format!("{frame:?}"))],
        ),
        Error::FrameCoverageMismatch => {
            failure_detail("codegen_backend_runtime_metadata_frame_coverage", [])
        }
        Error::FrameAbiMismatch(frame) => failure_detail(
            "codegen_backend_runtime_metadata_frame_abi",
            [text_failure_field("frame", format!("{frame:?}"))],
        ),
        Error::ExecutableHostMismatch => {
            failure_detail("codegen_backend_runtime_metadata_host_mismatch", [])
        }
    }
}

fn outcome_failure_detail(
    error: &bray_codegen::CodegenOutcomeBuildError,
) -> bray_diagnostics::DiagnosticNativeProductFailureDetail {
    use bray_codegen::{BackendArtifactSetBuildError as ArtifactError, CodegenOutcomeBuildError};

    match error {
        CodegenOutcomeBuildError::InvalidArtifacts(error) => match error {
            ArtifactError::RuntimeMetadataMismatch => {
                failure_detail("codegen_backend_outcome_runtime_metadata_mismatch", [])
            }
            ArtifactError::DuplicateArtifact(artifact) => artifact_failure_detail(
                "codegen_backend_outcome_duplicate_artifact",
                artifact,
            ),
            ArtifactError::MissingRequired(artifact) => artifact_failure_detail(
                "codegen_backend_outcome_missing_required_artifact",
                artifact,
            ),
            ArtifactError::UnrequestedArtifact(artifact) => artifact_failure_detail(
                "codegen_backend_outcome_unrequested_artifact",
                artifact,
            ),
            ArtifactError::BackendMismatch(artifact) => artifact_failure_detail(
                "codegen_backend_outcome_backend_mismatch",
                artifact,
            ),
            ArtifactError::CapabilityRevisionMismatch(artifact) => artifact_failure_detail(
                "codegen_backend_outcome_capability_revision_mismatch",
                artifact,
            ),
            ArtifactError::TargetMismatch(artifact) => artifact_failure_detail(
                "codegen_backend_outcome_target_mismatch",
                artifact,
            ),
        },
        CodegenOutcomeBuildError::ErrorDiagnostics(diagnostics) => failure_detail(
            "codegen_backend_outcome_error_diagnostics",
            [text_failure_field(
                "diagnostic_count",
                diagnostics.len().to_string(),
            )],
        ),
    }
}

fn artifact_failure_detail(
    reason: &'static str,
    artifact: &bray_codegen::BackendArtifactId,
) -> bray_diagnostics::DiagnosticNativeProductFailureDetail {
    failure_detail(
        reason,
        [text_failure_field("artifact", format!("{artifact:?}"))],
    )
}

const fn backend_artifact_kind(kind: bray_codegen::BackendArtifactKind) -> &'static str {
    use bray_codegen::BackendArtifactKind as Kind;

    match kind {
        Kind::RelocatableObject => "relocatable_object",
        Kind::Assembly => "assembly",
        Kind::BackendIr => "backend_ir",
        Kind::BackendBitcode => "backend_bitcode",
        Kind::ExecutableModule => "executable_module",
        Kind::DebugCompanion => "debug_companion",
    }
}
