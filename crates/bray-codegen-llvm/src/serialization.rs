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

pub(crate) fn native_unit(
    content: &ArtifactContent,
    kind: BackendArtifactKind,
    target: &bray_codegen::CodegenTarget,
) -> Result<Option<bray_native_artifact::NativeUnit>, CodegenFailure> {
    use bray_native_artifact::{
        NativeContentDigest, NativeUnit, NativeUnitKind, scan_object_unit_summary,
    };

    let artifact = kind;

    let kind = match kind {
        BackendArtifactKind::BackendBitcode => NativeUnitKind::Bitcode,
        BackendArtifactKind::RelocatableObject => NativeUnitKind::Object,
        _ => return Ok(None),
    };

    let bytes = content
        .read_shared()
        .map_err(|error| CodegenFailure::ArtifactRead {
            artifact,
            operation: error.operation(),
            kind: error
                .source()
                .map_or(std::io::ErrorKind::Other, std::io::Error::kind),
        })?;

    let digest =
        bray_base::sha256_reader(bytes.as_ref()).expect("hashing immutable memory cannot fail");

    let summary = match kind {
        NativeUnitKind::Bitcode => crate::summary::inspect_bitcode_unit_summary(
            &bytes,
            bray_target::NativeTarget::for_identity(target.identity())
                .expect("LLVM native target must be supported"),
        )
        .unwrap_or_else(|error| {
            panic!("final compiler-produced bitcode must be readable: {error:?}")
        }),
        NativeUnitKind::Object => scan_object_unit_summary(&bytes).unwrap_or_else(|error| {
            panic!("final compiler-produced object must be readable: {error}")
        }),
        NativeUnitKind::OpaqueArchive => {
            unreachable!("backend serializes native units, not archives")
        }
    };

    // Native targets use hosted C ABIs. LLVM can introduce runtime calls during
    // optimization and object emission, after the IR's declarations are known.
    // Retain the target runtime when this unit is imported independently.
    let runtime_library = match target.machine().object_format() {
        bray_target::ObjectFormat::Elf => Some("c"),
        bray_target::ObjectFormat::MachO => Some("System"),
        bray_target::ObjectFormat::Coff => Some("msvcrt"),
        bray_target::ObjectFormat::WebAssembly | bray_target::ObjectFormat::Xcoff => None,
    };

    let native_link = runtime_library.map(|library| {
        bray_symbols::NativeLinkRequirement::new(
            bray_base::NonEmptySharedStr::try_new(library).expect("hosted runtime library name is nonempty"),
            bray_symbols::NativeLinkKind::System,
        )
    });

    Ok(Some(NativeUnit::new(
        NativeContentDigest::new(digest),
        kind,
        summary,
        native_link,
    )))
}

fn artifact_serialization_failure(
    artifact: BackendArtifactKind,
    failure: CodegenFailure,
) -> CodegenFailure {
    match failure {
        CodegenFailure::BackendLibrary { report } => {
            CodegenFailure::ArtifactSerialization { artifact, report }
        }
        failure => failure,
    }
}

/// Parses complete bitcode bytes, preserving LLVM's rejection report for external inputs.
pub fn parse_bitcode<'context>(
    bytes: &[u8],
    context: &'context inkwell::context::Context,
) -> Result<Module<'context>, CodegenFailure> {
    let terminated = bytes.iter().copied().chain([0]).collect::<Vec<_>>();

    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
        &terminated,
        "bitcode module",
    );

    Module::parse_bitcode_from_buffer(&buffer, context).map_err(CodegenFailure::backend_library)
}

/// Serializes an LLVM module to valid bitcode bytes without the memory-buffer terminator.
pub fn bitcode_bytes(module: &Module<'_>) -> Vec<u8> {
    let buffer = module.write_bitcode_to_memory();
    let bytes = buffer.as_slice();

    bytes.strip_suffix(&[0]).unwrap_or(bytes).to_vec()
}

#[cfg(test)]
mod tests {
    use bray_codegen::{BackendArtifactKind, CodegenTarget};
    use bray_target::NativeTarget;
    use bray_testing::TemporaryFile;
    use inkwell::context::Context;
    use inkwell::memory_buffer::MemoryBuffer;
    use inkwell::module::Module;

    use super::{bitcode_bytes, native_unit, serialize_artifact};

    #[test]
    fn emitted_native_units_retain_their_hosted_runtime_dependency() {
        for (target, library, tls_symbol) in [
            (NativeTarget::X86_64LinuxGnu, "c", Some("__tls_get_addr")),
            (NativeTarget::Aarch64LinuxGnu, "c", None),
            (NativeTarget::X86_64WindowsMsvc, "msvcrt", Some("_tls_index")),
            (NativeTarget::Aarch64WindowsMsvc, "msvcrt", Some("_tls_index")),
            (NativeTarget::X86_64MacOs, "System", Some("__tlv_bootstrap")),
            (NativeTarget::Aarch64MacOs, "System", Some("__tlv_bootstrap")),
        ] {
            for thread_local in [false, true] {
                let context = Context::create();
                let qualifier = if thread_local { "thread_local " } else { "" };

                let ir = format!(r#"
                    @storage = {qualifier}global i32 0

                    define i32 @read_storage() {{
                        %value = load i32, ptr @storage
                        ret i32 %value
                    }}
                "#);

                let terminated = format!("{ir}\0");
                let buffer = MemoryBuffer::create_from_memory_range(terminated.as_bytes(), "TLS dependency");
                let module = context.create_module_from_ir(buffer).expect("fixture IR is valid");
                let target = CodegenTarget::for_native(target);
                let machine = crate::machine::LlvmTargetMachine::create(&target).expect("native target is supported");

                machine.configure_module(&module);

                let content = serialize_artifact(&machine, &module, BackendArtifactKind::RelocatableObject)
                    .expect("LLVM emits the native object");

                let unit = native_unit(&content, BackendArtifactKind::RelocatableObject, &target)
                    .expect("emitted object is readable").expect("native object has a dependency summary");

                let expected = tls_symbol.filter(|_| thread_local);

                if let Some(symbol) = expected {
                    assert!(unit.summary().references().iter().any(|reference| {
                        reference.identity().name() == Some(symbol)
                    }));
                }

                let libraries = unit.native_links().iter()
                    .map(|link| (link.name(), link.kind())).collect::<Vec<_>>();

                assert_eq!(libraries, [(library, bray_symbols::NativeLinkKind::System)]);
            }
        }
    }

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
