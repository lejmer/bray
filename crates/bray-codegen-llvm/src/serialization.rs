use bray_codegen::{ArtifactContent, BackendArtifactKind, CodegenFailure};
use inkwell::module::Module;
use inkwell::targets::FileType;

use crate::machine::LlvmTargetMachine;

pub(crate) fn serialize_artifact(
    machine: &LlvmTargetMachine,
    module: &Module<'_>,
    kind: BackendArtifactKind,
) -> Result<ArtifactContent, CodegenFailure> {
    let bytes = match kind {
        BackendArtifactKind::RelocatableObject => machine
            .serialize(module, FileType::Object)
            .map_err(|_| CodegenFailure::ArtifactConstruction(kind))?,
        BackendArtifactKind::Assembly => machine
            .serialize(module, FileType::Assembly)
            .map_err(|_| CodegenFailure::ArtifactConstruction(kind))?,
        BackendArtifactKind::BackendIr => module.print_to_string().to_bytes().to_vec(),
        BackendArtifactKind::BackendBitcode => {
            module.write_bitcode_to_memory().as_slice().to_vec()
        }
        BackendArtifactKind::ExecutableModule | BackendArtifactKind::DebugCompanion => {
            return Err(CodegenFailure::UnsupportedArtifact(kind));
        }
    };

    ArtifactContent::try_memory(bytes).map_err(|_| CodegenFailure::ArtifactConstruction(kind))
}
