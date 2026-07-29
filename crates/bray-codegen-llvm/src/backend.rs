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
const LLVM_REVISION: &str = env!("BRAY_LLVM_REVISION");
const LLVM_TARGETS: &str = env!("BRAY_LLVM_TARGETS");

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

        machine.validate_contract(request.target(), &context)?;
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

        let machine = LlvmTargetMachine::create(target)?;
        let context = Context::create();

        machine.validate_contract(target, &context)
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
    .filter(|platform| llvm_target_is_built(platform.architecture()))
}

const fn platform(
    architecture: TargetArchitecture,
    object_format: ObjectFormat,
) -> BackendTargetPlatform {
    BackendTargetPlatform::new(architecture, object_format)
}

fn llvm_target_is_built(architecture: TargetArchitecture) -> bool {
    let family = match architecture {
        TargetArchitecture::X86 | TargetArchitecture::X86_64 => "X86",
        TargetArchitecture::Arm => "ARM",
        TargetArchitecture::Aarch64 => "AArch64",
        TargetArchitecture::Riscv32 | TargetArchitecture::Riscv64 => "RISCV",
        TargetArchitecture::PowerPc64 => "PowerPC",
        TargetArchitecture::Wasm32 | TargetArchitecture::Wasm64 => "WebAssembly",
    };

    LLVM_TARGETS.split(',').any(|target| target == family)
}

#[cfg(test)]
mod tests {
    use bray_codegen::test_support::{
        codegen_request_for_backend, codegen_target, codegen_target_with_profile,
    };
    use bray_codegen::{BackendArtifactKind, CodeGenerator, CodegenFailure, CodegenStatus};
    use bray_target::test_support::test_target_profile;
    use inkwell::OptimizationLevel;
    use inkwell::targets::{
        CodeModel, RelocMode, Target, TargetTriple,
    };

    use super::{LLVM_REVISION, LlvmCodeGenerator, representative_triple};
    use crate::initialization;

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

        let mismatched = codegen_target_with_profile(
            test_target_profile(),
            "x86_64-pc-windows-msvc",
        );

        assert_eq!(
            backend.validate_target(&mismatched),
            Err(CodegenFailure::UnsupportedTarget)
        );
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

    #[test]
    fn every_advertised_platform_has_a_compiled_llvm_target() {
        initialization::initialize();

        let Ok(backend) = LlvmCodeGenerator::try_new() else {
            panic!("LLVM backend constants must be valid");
        };

        for platform in backend.capabilities().target_platforms() {
            let triple = TargetTriple::create(representative_triple(platform));

            let Ok(target) = Target::from_triple(&triple) else {
                panic!("{platform:?} must have a compiled LLVM target");
            };

            assert!(
                target
                    .create_target_machine(
                        &triple,
                        "",
                        "",
                        OptimizationLevel::None,
                        RelocMode::Default,
                        CodeModel::Default,
                    )
                    .is_some(),
                "{platform:?} must construct an LLVM target machine"
            );
        }
    }
}

#[cfg(test)]
fn representative_triple(platform: &BackendTargetPlatform) -> &'static str {
    match (platform.architecture(), platform.object_format()) {
        (TargetArchitecture::X86, ObjectFormat::Coff) => "i686-pc-windows-msvc",
        (TargetArchitecture::X86, ObjectFormat::Elf) => "i686-unknown-linux-gnu",
        (TargetArchitecture::X86, ObjectFormat::MachO) => "i686-apple-darwin",
        (TargetArchitecture::X86_64, ObjectFormat::Coff) => "x86_64-pc-windows-msvc",
        (TargetArchitecture::X86_64, ObjectFormat::Elf) => "x86_64-unknown-linux-gnu",
        (TargetArchitecture::X86_64, ObjectFormat::MachO) => "x86_64-apple-darwin",
        (TargetArchitecture::Arm, ObjectFormat::Coff) => "armv7-pc-windows-msvc",
        (TargetArchitecture::Arm, ObjectFormat::Elf) => "armv7-unknown-linux-gnueabihf",
        (TargetArchitecture::Arm, ObjectFormat::MachO) => "armv7-apple-darwin",
        (TargetArchitecture::Aarch64, ObjectFormat::Coff) => "aarch64-pc-windows-msvc",
        (TargetArchitecture::Aarch64, ObjectFormat::Elf) => "aarch64-unknown-linux-gnu",
        (TargetArchitecture::Aarch64, ObjectFormat::MachO) => "aarch64-apple-darwin",
        (TargetArchitecture::Riscv32, ObjectFormat::Elf) => "riscv32-unknown-linux-gnu",
        (TargetArchitecture::Riscv64, ObjectFormat::Elf) => "riscv64-unknown-linux-gnu",
        (TargetArchitecture::PowerPc64, ObjectFormat::Elf) => "powerpc64-unknown-linux-gnu",
        (TargetArchitecture::PowerPc64, ObjectFormat::Xcoff) => "powerpc64-ibm-aix",
        (TargetArchitecture::Wasm32, ObjectFormat::WebAssembly) => "wasm32-unknown-unknown",
        (TargetArchitecture::Wasm64, ObjectFormat::WebAssembly) => "wasm64-unknown-unknown",
        _ => "unknown-unknown-unknown",
    }
}
