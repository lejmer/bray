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
        Error::ResourceExhausted => Kind::CodegenBackendResourceExhausted,
        Error::BackendLibrary { report } => Kind::CodegenBackendLibraryFailure(failure_detail(
            "codegen_backend_library_failure",
            [text_failure_field("report", report.as_ref())],
        )),
        Error::BackendToolExited { program, exit } => {
            Kind::CodegenBackendToolFailure(failure_detail(
                "codegen_backend_tool_failure",
                [
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
    }
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
