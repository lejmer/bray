use std::collections::BTreeMap;
use std::path::Path;

use bray_base::sha256_file;
use bray_runtime_interface::{RuntimeArtifact, RuntimeArtifactDigest, RuntimeArtifactMetadata};

/// Loads and validates one runtime artifact from its metadata path.
pub fn load_runtime_artifact(metadata_path: &Path) -> Option<RuntimeArtifact> {
    let metadata_bytes = std::fs::read(metadata_path).ok()?;
    let metadata = RuntimeArtifactMetadata::decode_json(&metadata_bytes).ok()?;
    let directory = metadata_path.parent()?;
    let mut archives = BTreeMap::new();
    let mut components = Vec::with_capacity(metadata.components().len());

    for component in metadata.components() {
        let archive = directory.join(component.archive_file_name());

        let digest = match archives.get(component.archive_file_name()) {
            Some(digest) => *digest,
            None => {
                let digest = digest_file(&archive)?;

                archives.insert(component.archive_file_name().to_owned(), digest);

                digest
            }
        };

        components.push((component.identity().clone(), archive, digest));
    }

    RuntimeArtifact::try_new(metadata, components).ok()
}

fn digest_file(path: &Path) -> Option<RuntimeArtifactDigest> {
    sha256_file(path).ok().map(RuntimeArtifactDigest::new)
}
