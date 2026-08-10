use std::path::Path;

use bray_runtime_interface::{RuntimeArtifact, RuntimeArtifactMetadata};

/// Loads and validates one runtime artifact from its metadata path.
pub fn load_runtime_artifact(metadata_path: &Path) -> Option<RuntimeArtifact> {
    let metadata_bytes = std::fs::read(metadata_path).ok()?;
    let metadata = RuntimeArtifactMetadata::decode_json(&metadata_bytes).ok()?;
    let directory = metadata_path.parent()?;
    let mut components = Vec::with_capacity(metadata.components().len());

    for component in metadata.components() {
        let archive = directory.join(component.archive_file_name());

        components.push((component.identity().clone(), archive));
    }

    RuntimeArtifact::try_new(metadata, components).ok()
}
