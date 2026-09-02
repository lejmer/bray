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
            .map_err(|failure| artifact_serialization_failure(kind, failure))?,
        BackendArtifactKind::Assembly => machine
            .serialize(module, FileType::Assembly)
            .map_err(|failure| artifact_serialization_failure(kind, failure))?,
        BackendArtifactKind::BackendIr => module.print_to_string().to_bytes().to_vec(),
        BackendArtifactKind::BackendBitcode => bitcode_bytes(module),
        BackendArtifactKind::ExecutableModule | BackendArtifactKind::DebugCompanion => {
            return Err(CodegenFailure::UnsupportedArtifact(kind));
        }
    };

    ArtifactContent::try_memory(bytes).map_err(|cause| CodegenFailure::InvalidArtifactContent {
        artifact: kind,
        cause,
    })
}

fn artifact_serialization_failure(
    artifact: BackendArtifactKind,
    failure: CodegenFailure,
) -> CodegenFailure {
    match failure {
        CodegenFailure::BackendLibrary { report } => CodegenFailure::ArtifactSerialization {
            artifact,
            report,
        },
        failure => failure,
    }
}

fn bitcode_bytes(module: &Module<'_>) -> Vec<u8> {
    let buffer = module.write_bitcode_to_memory();
    let bytes = buffer.as_slice();

    bytes.strip_suffix(&[0]).unwrap_or(bytes).to_vec()
}

#[cfg(test)]
mod tests {
    use bray_testing::TemporaryFile;
    use inkwell::context::Context;
    use inkwell::memory_buffer::MemoryBuffer;
    use inkwell::module::Module;

    use super::bitcode_bytes;

    #[test]
    fn serialized_bitcode_excludes_the_memory_buffer_terminator() {
        let context = Context::create();
        let module = context.create_module("bitcode.serialization");
        let bytes = bitcode_bytes(&module);

        assert_eq!(bytes.len() % 4, 0);

        let file = TemporaryFile::write("serialization.bc", &bytes);

        let buffer = MemoryBuffer::create_from_file(file.path())
            .unwrap_or_else(|error| panic!("serialized bitcode must be readable: {error}"));

        assert!(Module::parse_bitcode_from_buffer(&buffer, &context).is_ok());
    }
}
