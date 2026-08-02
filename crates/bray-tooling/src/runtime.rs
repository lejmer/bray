use std::path::Path;

use bray_runtime_interface::{RuntimeArtifact, RuntimeArtifactDigest, RuntimeArtifactMetadata};
use sha2::{Digest, Sha256};

/// Loads and validates one runtime artifact from its metadata path.
pub fn load_runtime_artifact(metadata_path: &Path) -> Option<RuntimeArtifact> {
    let metadata_bytes = std::fs::read(metadata_path).ok()?;
    let metadata = RuntimeArtifactMetadata::decode_json(&metadata_bytes).ok()?;
    let directory = metadata_path.parent()?;
    let archive = directory.join(metadata.archive_file_name());
    let archive_bytes = std::fs::read(&archive).ok()?;
    let digest = RuntimeArtifactDigest::new(Sha256::digest(&archive_bytes).into());

    RuntimeArtifact::try_new(metadata, archive, digest).ok()
}
