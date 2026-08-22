use std::path::{Path, PathBuf};

use bray_base::sha256_file;
use bray_runtime_interface::RuntimeArtifactMetadata;

pub(super) fn current(
    output: &Path,
    metadata_path: &Path,
    expected_archives: &[PathBuf],
    input: &str,
) -> Result<bool, String> {
    if !crate::input_identity::stored_digest_matches(output, input)? {
        return Ok(false);
    }

    let bytes = match std::fs::read(metadata_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "could not read {}: {error}",
                metadata_path.display()
            ));
        }
    };

    let Ok(metadata) = RuntimeArtifactMetadata::decode_json(&bytes) else {
        return Ok(false);
    };

    for component in metadata.components() {
        let archive = output.join(component.archive_file_name());

        let Ok(digest) = sha256_file(&archive) else {
            return Ok(false);
        };

        if digest != component.archive_digest().bytes() {
            return Ok(false);
        }
    }

    Ok(expected_archives.iter().all(|archive| archive.is_file()))
}
