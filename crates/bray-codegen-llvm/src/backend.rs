use std::collections::BTreeMap;

use bray_codegen::{
    ArtifactContent, AssemblySyntaxKind, BackendArtifactContribution, BackendArtifactKind,
    BackendArtifactRequirement, BackendCapabilities, BackendIdentity, BackendTargetPlatform,
    CodeGenerator, CodegenFailure, CodegenOutcome, CodegenRequest, CodegenRuntimeMetadata,
    CodegenTarget, DebugInformationMode,
};
use bray_diagnostics::DiagnosticBag;
use bray_target::{ObjectFormat, TargetArchitecture};
use inkwell::context::Context;
use inkwell::module::Module;

use crate::machine::LlvmTargetMachine;
use crate::mapping::{LlvmTypeMappings, add_debug_metadata, declare_symbols};
use crate::optimization::optimize_module;
use crate::serialization::serialize_artifact;
use crate::translation::{TranslationError, translate_instances};

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

    fn prepare_module<'context>(
        &self,
        request: CodegenRequest<'_>,
        context: &'context Context,
    ) -> Result<Option<(LlvmTargetMachine, Module<'context>)>, CodegenFailure> {
        let machine = LlvmTargetMachine::create_for_codegen(
            request.target(),
            request.options().optimization(),
        )?;

        let module = context.create_module("bray.codegen.unit");

        machine.validate_contract(request.target(), context)?;
        machine.configure_module(&module);

        let target_data = machine.target_data();

        let mut types =
            LlvmTypeMappings::new(context, request.mappings(), request.target(), &target_data);

        declare_symbols(&module, request.mappings(), request.target(), &mut types)?;

        add_debug_metadata(
            context,
            &module,
            request.mappings(),
            request.options().debug_information(),
        );

        match translate_instances(context, &module, request, &mut types) {
            Ok(()) => {}
            Err(TranslationError::Cancelled) => return Ok(None),
            Err(TranslationError::Failed(failure)) => return Err(failure),
        }

        Ok(Some((machine, module)))
    }

    fn generate_artifacts(
        &self,
        request: CodegenRequest<'_>,
    ) -> Result<CodegenOutcome, CodegenFailure> {
        let context = Context::create();

        let Some((machine, module)) = self.prepare_module(request, &context)? else {
            return Ok(CodegenOutcome::cancelled());
        };

        if request.cancellation().is_cancelled() {
            return Ok(CodegenOutcome::cancelled());
        }

        module
            .verify()
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        if request.cancellation().is_cancelled() {
            return Ok(CodegenOutcome::cancelled());
        }

        optimize_module(&module, &machine, *request.options())?;

        if request.cancellation().is_cancelled() {
            return Ok(CodegenOutcome::cancelled());
        }

        module
            .verify()
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        let mut serialized: BTreeMap<BackendArtifactKind, ArtifactContent> = BTreeMap::new();

        let mut contributions = Vec::new();

        for entry in request.artifacts().entries() {
            if request.cancellation().is_cancelled() {
                return Ok(CodegenOutcome::cancelled());
            }

            let kind = entry.id().kind();

            let content = if let Some(content) = serialized.get(&kind) {
                // Artifact content is an Arc-backed immutable handle reused for repeated requests.
                content.clone()
            } else {
                match serialize_artifact(&machine, &module, kind) {
                    Ok(content) => {
                        // The cache retains the immutable bytes for later same-kind entries.
                        serialized.insert(kind, content.clone());

                        content
                    }
                    Err(_) if entry.requirement() == BackendArtifactRequirement::Optional => {
                        continue;
                    }
                    Err(failure) => return Err(failure),
                }
            };

            contributions.push(BackendArtifactContribution::new(
                // Contributions retain Arc-backed request identities after generation returns.
                entry.id().clone(),
                content,
                // Contributions retain Arc-backed backend and target identities.
                self.identity.clone(),
                request.target().identity().clone(),
                None,
            ));
        }

        if request.cancellation().is_cancelled() {
            return Ok(CodegenOutcome::cancelled());
        }

        let runtime_metadata = runtime_metadata(request)?;

        CodegenOutcome::try_complete(
            request,
            contributions,
            runtime_metadata,
            DiagnosticBag::new(),
        )
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)
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

        match self.generate_artifacts(request) {
            Ok(outcome) => outcome,
            Err(failure) => CodegenOutcome::failed(failure, DiagnosticBag::new()),
        }
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
        [AssemblySyntaxKind::TargetDefault],
    )
}

fn runtime_metadata(
    request: CodegenRequest<'_>,
) -> Result<CodegenRuntimeMetadata, CodegenFailure> {
    let mut hosts = request.unit().mir_units().filter_map(|unit| {
        let bray_ir::MirUnitKind::ExecutableHost(host) = unit.kind() else {
            return None;
        };

        Some(host)
    });

    // Runtime metadata owns the small immutable host contract after the MIR borrow ends.
    let host = hosts.next().cloned();

    if hosts.next().is_some() {
        return Err(CodegenFailure::GeneratedModuleInvariant);
    }

    CodegenRuntimeMetadata::try_new(request.unit(), [], host)
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)
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
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_base::Cancellation;
    use bray_codegen::ArtifactContentSource;
    use bray_codegen::test_support::{
        codegen_request_for_backend, codegen_request_for_seed_and_backend, codegen_target,
        codegen_target_with_profile,
    };
    use bray_codegen::{BackendArtifactKind, CodeGenerator, CodegenFailure, CodegenStatus};
    use bray_target::test_support::test_target_profile;
    use inkwell::OptimizationLevel;
    use inkwell::targets::{CodeModel, RelocMode, Target, TargetTriple};

    use super::{LLVM_REVISION, LlvmCodeGenerator, representative_triple};
    use crate::initialization;
    use crate::serialization::serialize_artifact;

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

        assert_eq!(
            backend.capabilities().assembly_syntax_kinds(),
            &[bray_codegen::AssemblySyntaxKind::TargetDefault]
        );

        assert_eq!(backend.validate_target(&codegen_target()), Ok(()));

        let mismatched =
            codegen_target_with_profile(test_target_profile(), "x86_64-pc-windows-msvc");

        assert_eq!(
            backend.validate_target(&mismatched),
            Err(CodegenFailure::UnsupportedTarget)
        );
    }

    #[test]
    fn generation_publishes_only_immutable_requested_contributions() {
        let Ok(backend) = LlvmCodeGenerator::try_new() else {
            panic!("LLVM backend constants must be valid");
        };

        let fixture = codegen_request_for_backend(backend.identity().clone());
        let outcome = backend.generate(fixture.request());

        assert!(matches!(outcome.status(), CodegenStatus::Complete(_)));

        let Some(artifacts) = outcome.artifacts() else {
            panic!("successful generation must publish requested artifacts");
        };

        assert_eq!(artifacts.contributions().len(), 2);

        for contribution in artifacts.contributions() {
            assert_eq!(
                contribution.id().kind(),
                BackendArtifactKind::RelocatableObject
            );

            assert_ne!(contribution.content().byte_len(), 0);
        }
    }

    #[test]
    fn requested_llvm_artifact_formats_serialize_from_the_verified_module() {
        let Ok(backend) = LlvmCodeGenerator::try_new() else {
            panic!("LLVM backend constants must be valid");
        };

        let fixture = codegen_request_for_backend(backend.identity().clone());
        let request = fixture.request();
        let context = inkwell::context::Context::create();

        let Ok(Some((machine, module))) = backend.prepare_module(request, &context) else {
            panic!("test mappings must produce a valid LLVM module");
        };

        for kind in [
            BackendArtifactKind::RelocatableObject,
            BackendArtifactKind::Assembly,
            BackendArtifactKind::BackendIr,
            BackendArtifactKind::BackendBitcode,
        ] {
            let Ok(content) = serialize_artifact(&machine, &module, kind) else {
                panic!("{kind:?} must serialize");
            };

            assert_ne!(content.byte_len(), 0, "{kind:?}");
        }
    }

    #[test]
    fn repeated_generation_is_byte_for_byte_deterministic() {
        let Ok(backend) = LlvmCodeGenerator::try_new() else {
            panic!("LLVM backend constants must be valid");
        };

        let fixture = codegen_request_for_backend(backend.identity().clone());
        let first = backend.generate(fixture.request());
        let second = backend.generate(fixture.request());

        assert_eq!(artifact_bytes(&first), artifact_bytes(&second));
    }

    #[test]
    fn serial_and_parallel_reversed_demand_produce_identical_artifacts() {
        let Ok(backend) = LlvmCodeGenerator::try_new() else {
            panic!("LLVM backend constants must be valid");
        };

        let backend = Arc::new(backend);
        let identity = backend.identity().clone();
        let mut serial = BTreeMap::new();

        for seed in [1, 2] {
            let fixture = codegen_request_for_seed_and_backend(seed, identity.clone());

            serial.insert(seed, artifact_bytes(&backend.generate(fixture.request())));
        }

        let workers = [2, 1].map(|seed| {
            let backend = Arc::clone(&backend);
            let identity = identity.clone();

            std::thread::spawn(move || {
                let fixture = codegen_request_for_seed_and_backend(seed, identity);
                let bytes = artifact_bytes(&backend.generate(fixture.request()));

                (seed, bytes)
            })
        });

        let parallel = workers
            .into_iter()
            .map(|worker| {
                worker
                    .join()
                    .unwrap_or_else(|_| panic!("parallel code generation must finish"))
            })
            .collect::<BTreeMap<_, _>>();

        assert_eq!(parallel, serial);
    }

    #[test]
    fn unsupported_artifact_kinds_fail_with_typed_capability_errors() {
        let Ok(backend) = LlvmCodeGenerator::try_new() else {
            panic!("LLVM backend constants must be valid");
        };

        let fixture = codegen_request_for_backend(backend.identity().clone());
        let context = inkwell::context::Context::create();

        let Ok(Some((machine, module))) = backend.prepare_module(fixture.request(), &context) else {
            panic!("test mappings must produce a valid LLVM module");
        };

        for kind in [
            BackendArtifactKind::ExecutableModule,
            BackendArtifactKind::DebugCompanion,
        ] {
            assert_eq!(
                serialize_artifact(&machine, &module, kind),
                Err(CodegenFailure::UnsupportedArtifact(kind))
            );
        }
    }

    #[test]
    fn cancellation_during_serialization_discards_completed_contributions() {
        let Ok(backend) = LlvmCodeGenerator::try_new() else {
            panic!("LLVM backend constants must be valid");
        };

        let fixture = codegen_request_for_backend(backend.identity().clone());
        let cancellation = CancelAfter::new(6);
        let outcome = backend.generate(fixture.request_with_cancellation(&cancellation));

        assert!(matches!(outcome.status(), CodegenStatus::Cancelled));
        assert!(outcome.artifacts().is_none());
    }

    #[test]
    fn cancellation_during_translation_is_not_reported_as_backend_failure() {
        let Ok(backend) = LlvmCodeGenerator::try_new() else {
            panic!("LLVM backend constants must be valid");
        };

        let fixture = codegen_request_for_backend(backend.identity().clone());
        let cancellation = CancelAfter::new(1);
        let outcome = backend.generate(fixture.request_with_cancellation(&cancellation));

        assert!(matches!(outcome.status(), CodegenStatus::Cancelled));
    }

    #[test]
    fn mapped_symbols_use_exact_names_linkage_abi_and_source_locations() {
        let Ok(backend) = LlvmCodeGenerator::try_new() else {
            panic!("LLVM backend constants must be valid");
        };

        let fixture = codegen_request_for_backend(backend.identity().clone());
        let request = fixture.request();
        let context = inkwell::context::Context::create();

        let Ok(Some((_, module))) = backend.prepare_module(request, &context) else {
            panic!("test mappings must produce a valid LLVM module");
        };

        let Some(function) = module.get_function("bray_test_0") else {
            panic!("exact mapped symbol name must be declared");
        };

        assert_eq!(function.get_linkage(), inkwell::module::Linkage::External);

        assert_eq!(
            function.as_global_value().get_visibility(),
            inkwell::GlobalVisibility::Hidden,
        );

        assert_eq!(function.get_call_conventions(), 0);
        assert_ne!(function.count_basic_blocks(), 0);

        let source = request
            .unit()
            .mir_units()
            .next()
            .map(|mir| mir.blocks()[0].source());

        assert!(source.is_some_and(|source| request.mappings().debug_location(source).is_some()));
        assert!(module.verify().is_ok());
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

    fn artifact_bytes(outcome: &bray_codegen::CodegenOutcome) -> Vec<Vec<u8>> {
        let Some(artifacts) = outcome.artifacts() else {
            panic!("test generation must complete");
        };

        artifacts
            .contributions()
            .iter()
            .map(|contribution| match contribution.content().source() {
                ArtifactContentSource::Memory(bytes) => bytes.to_vec(),
                ArtifactContentSource::CompilerSpool(_) => {
                    panic!("small test artifacts must remain memory-backed");
                }
            })
            .collect()
    }

    struct CancelAfter {
        remaining_checks: AtomicUsize,
    }

    impl CancelAfter {
        const fn new(checks: usize) -> Self {
            Self {
                remaining_checks: AtomicUsize::new(checks),
            }
        }
    }

    impl Cancellation for CancelAfter {
        fn is_cancelled(&self) -> bool {
            self.remaining_checks
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                    remaining.checked_sub(1)
                })
                .is_err()
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
