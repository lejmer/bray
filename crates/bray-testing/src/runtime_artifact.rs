use std::path::Path;

use bray_base::{NonEmptySharedStr, sha256_file, sha256_reader};
use bray_native_artifact::{
    NativeArtifactIndex, NativeContentDigest, NativeDefinition, NativeDefinitionSelection,
    NativeUnit, NativeUnitKind, NativeUnitSummary, ValidatedNativeArtifact,
};
use bray_runtime_interface::{RuntimeNativeIndexMetadata, RuntimeArtifactPurpose};
use bray_symbols::NativeSymbolContract;
use bray_target::NativeTarget;

/// Publish an authenticated test index with exact role symbols and a lazy archive.
pub fn test_runtime_native_index<S: AsRef<str>>(
    directory: &Path,
    target: NativeTarget,
    purpose: RuntimeArtifactPurpose,
    role_symbols: impl IntoIterator<Item = S>,
    archive: &Path,
) -> (RuntimeNativeIndexMetadata, ValidatedNativeArtifact) {
    let native = directory.join("native");
    std::fs::create_dir_all(&native).expect("test native directory must exist");

    let object_bytes = b"test runtime role definitions";
    let object_digest = NativeContentDigest::new(sha256_reader(&object_bytes[..]).expect("test bytes must hash"));

    std::fs::write(native.join(NativeUnitKind::Object.file_name(object_digest, target)), object_bytes)
        .expect("test role unit must be written");

    let definitions = role_symbols.into_iter().map(|name| {
        let name = NonEmptySharedStr::try_new(name.as_ref()).expect("test role symbol must be nonempty");

        NativeDefinition::new(NativeSymbolContract::required_name(name), NativeDefinitionSelection::Ordinary)
    }).collect::<Vec<_>>();

    let object = NativeUnit::new(object_digest, NativeUnitKind::Object, NativeUnitSummary::Exact {
        definitions: definitions.into(), references: [].into(), roots: [].into(),
    }, [], []);

    let archive_digest = NativeContentDigest::new(sha256_file(archive).expect("test archive must hash"));

    std::fs::copy(archive, native.join(NativeUnitKind::OpaqueArchive.file_name(archive_digest, target)))
        .expect("test archive must be copied");

    let archive = NativeUnit::new(archive_digest, NativeUnitKind::OpaqueArchive, NativeUnitSummary::Opaque, [], []);
    let producer = NativeContentDigest::new([9; 32]);

    let index = NativeArtifactIndex::try_new(target, producer, [object, archive], [])
        .expect("test native index must be valid");

    let bytes = index.encode().expect("test native index must encode");
    let digest = NativeContentDigest::new(sha256_reader(&bytes[..]).expect("test index bytes must hash"));
    let file_name = format!("runtime-{}-native-index.json", purpose.as_str());
    std::fs::write(directory.join(&file_name), &bytes).expect("test native index must be written");

    let metadata = RuntimeNativeIndexMetadata::try_new(purpose, file_name, digest)
        .expect("test index reference must be valid");

    let validated = NativeArtifactIndex::import(&bytes, digest, target, producer, &native)
        .expect("test native index must authenticate");

    (metadata, validated)
}
