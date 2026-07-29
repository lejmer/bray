use bray_codegen::{
    AssemblySyntaxKind, BackendArtifactKind, BackendCapabilities, BackendIdentity,
    BackendTargetPlatform, CodeGenerator, CodegenFailure, CodegenOutcome, CodegenRequest,
    CodegenTarget, DebugInformationMode,
};
use bray_diagnostics::DiagnosticBag;
use bray_target::{ObjectFormat, TargetArchitecture};
use inkwell::context::Context;

use crate::machine::LlvmTargetMachine;

const BACKEND_NAME: &str = "llvm";
const BACKEND_REVISION: &str = "1";
const LLVM_REVISION: &str = "22.1.8";

/// LLVM implementation of Bray's coarse code generation contract.
pub struct LlvmCodeGenerator {
    identity: BackendIdentity,
    capabilities: BackendCapabilities,
}

impl LlvmCodeGenerator {
    /// Creates the LLVM backend with its stable identity and declared capabilities.
    pub fn try_new() -> Result<Self, CodegenFailure> {
        let Some(identity) =
            BackendIdentity::try_new(BACKEND_NAME, BACKEND_REVISION, LLVM_REVISION)
        else {
            return Err(CodegenFailure::InvalidConfiguration);
        };

        Ok(Self {
            identity,
            capabilities: capabilities(),
        })
    }

    fn prepare_module(&self, request: CodegenRequest<'_>) -> Result<(), CodegenFailure> {
        let machine = LlvmTargetMachine::create(request.target())?;
        let context = Context::create();
        let module = context.create_module("bray.codegen.unit");

        machine.configure_module(&module);

        Ok(())
    }
}

impl CodeGenerator for LlvmCodeGenerator {
    fn identity(&self) -> &BackendIdentity {
        &self.identity
    }

    fn capabilities(&self) -> &BackendCapabilities {
        &self.capabilities
    }

    fn validate_target(&self, target: &CodegenTarget) -> Result<(), CodegenFailure> {
        if !self.capabilities.supports_platform(target) {
            return Err(CodegenFailure::UnsupportedTarget);
        }

        LlvmTargetMachine::create(target).map(|_| ())
    }

    fn generate(&self, request: CodegenRequest<'_>) -> CodegenOutcome {
        if request.cancellation().is_cancelled() {
            return CodegenOutcome::cancelled();
        }

        if request.backend() != self.identity() {
            return CodegenOutcome::failed(
                CodegenFailure::InvalidConfiguration,
                DiagnosticBag::new(),
            );
        }

        if let Err(failure) = self.prepare_module(request) {
            return CodegenOutcome::failed(failure, DiagnosticBag::new());
        }

        // TODO(BRA-161): Translate every MIR definition before completing artifact generation.
        CodegenOutcome::failed(
            CodegenFailure::GeneratedModuleInvariant,
            DiagnosticBag::new(),
        )
    }
}

fn capabilities() -> BackendCapabilities {
    BackendCapabilities::new(
        target_platforms(),
        [
            BackendArtifactKind::RelocatableObject,
            BackendArtifactKind::Assembly,
            BackendArtifactKind::BackendIr,
            BackendArtifactKind::BackendBitcode,
        ],
        [
            DebugInformationMode::None,
            DebugInformationMode::LineTables,
            DebugInformationMode::Full,
        ],
        [
            AssemblySyntaxKind::TargetDefault,
            AssemblySyntaxKind::Intel,
            AssemblySyntaxKind::Att,
        ],
    )
}

fn target_platforms() -> impl Iterator<Item = BackendTargetPlatform> {
    [
        platform(TargetArchitecture::X86, ObjectFormat::Coff),
        platform(TargetArchitecture::X86, ObjectFormat::Elf),
        platform(TargetArchitecture::X86, ObjectFormat::MachO),
        platform(TargetArchitecture::X86_64, ObjectFormat::Coff),
        platform(TargetArchitecture::X86_64, ObjectFormat::Elf),
        platform(TargetArchitecture::X86_64, ObjectFormat::MachO),
        platform(TargetArchitecture::Arm, ObjectFormat::Coff),
        platform(TargetArchitecture::Arm, ObjectFormat::Elf),
        platform(TargetArchitecture::Arm, ObjectFormat::MachO),
        platform(TargetArchitecture::Aarch64, ObjectFormat::Coff),
        platform(TargetArchitecture::Aarch64, ObjectFormat::Elf),
        platform(TargetArchitecture::Aarch64, ObjectFormat::MachO),
        platform(TargetArchitecture::Riscv32, ObjectFormat::Elf),
        platform(TargetArchitecture::Riscv64, ObjectFormat::Elf),
        platform(TargetArchitecture::PowerPc64, ObjectFormat::Elf),
        platform(TargetArchitecture::PowerPc64, ObjectFormat::Xcoff),
        platform(TargetArchitecture::Wasm32, ObjectFormat::WebAssembly),
        platform(TargetArchitecture::Wasm64, ObjectFormat::WebAssembly),
    ]
    .into_iter()
}

const fn platform(
    architecture: TargetArchitecture,
    object_format: ObjectFormat,
) -> BackendTargetPlatform {
    BackendTargetPlatform::new(architecture, object_format)
}

#[cfg(test)]
mod tests {
    use bray_codegen::test_support::{codegen_request_for_backend, codegen_target};
    use bray_codegen::{
        BackendArtifactKind, CodeGenerator, CodegenFailure, CodegenStatus,
    };

    use super::{LLVM_REVISION, LlvmCodeGenerator};

    #[test]
    fn backend_identity_and_capabilities_are_stable() {
        let Ok(backend) = LlvmCodeGenerator::try_new() else {
            panic!("LLVM backend constants must be valid");
        };

        assert_eq!(backend.identity().name(), "llvm");
        assert_eq!(backend.identity().revision(), "1");
        assert_eq!(backend.identity().toolchain_revision(), LLVM_REVISION);
        assert!(backend.capabilities().supports_platform(&codegen_target()));

        assert!(
            backend
                .capabilities()
                .supports_artifact(BackendArtifactKind::RelocatableObject)
        );

        assert_eq!(backend.validate_target(&codegen_target()), Ok(()));
    }

    #[test]
    fn generation_uses_task_local_backend_state_without_publishing_it() {
        let Ok(backend) = LlvmCodeGenerator::try_new() else {
            panic!("LLVM backend constants must be valid");
        };

        let fixture = codegen_request_for_backend(backend.identity().clone());
        let outcome = backend.generate(fixture.request());

        assert_eq!(
            outcome.status(),
            &CodegenStatus::Failed(CodegenFailure::GeneratedModuleInvariant)
        );

        assert!(outcome.artifacts().is_none());
    }
}
