use std::path::{Path, PathBuf};

use bray_runtime_interface::RuntimeArtifactMetadata;
use sha2::{Digest as _, Sha256};

pub(super) fn content_identity(input: &str, paths: &[PathBuf]) -> Result<Option<String>, String> {
    let mut identity = Sha256::new();

    identity.update(input);

    for path in paths {
        let name = path
            .file_name()
            .expect("published runtime artifacts have file names")
            .to_string_lossy();

        identity.update(name.len().to_le_bytes());
        identity.update(name.as_bytes());

        match bray_base::sha256_file(path) {
            Ok(artifact) => identity.update(artifact),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("could not hash {}: {error}", path.display())),
        }
    }

    Ok(Some(bray_base::lowercase_hex(&identity.finalize())))
}

pub(super) fn current(output: &Path, metadata_path: &Path, input: &str) -> Result<bool, String> {
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

    if bray_tooling::load_runtime_artifact(
        metadata_path,
        metadata.contract().target(),
        metadata.contract().abi_version(),
    )
    .is_err()
    {
        return Ok(false);
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{content_identity, current};

    #[test]
    fn runtime_identity_authenticates_archives_and_package_artifacts() {
        let directory = tempfile::tempdir().expect("runtime cache");
        let archive = directory.path().join("runtime.a");
        let implementation = directory.path().join("runtime.brayi");
        let metadata = directory.path().join("bray-runtime.brayrt");
        let paths = [archive.clone(), implementation.clone(), metadata.clone()];

        assert_eq!(content_identity("inputs", &paths).expect("identity"), None);

        fs::write(&archive, b"archive").expect("archive");
        fs::write(&implementation, b"implementation").expect("implementation");
        fs::write(&metadata, b"metadata").expect("metadata");

        let identity = content_identity("inputs", &paths)
            .expect("identity")
            .expect("complete runtime");

        crate::input_identity::write_digest(directory.path(), &identity).expect("publish identity");

        assert!(
            crate::input_identity::stored_digest_matches(directory.path(), &identity).expect("hit")
        );

        for path in &paths {
            let bytes = fs::read(path).expect("original bytes");

            fs::write(path, b"corrupt").expect("corrupt artifact");

            let changed = content_identity("inputs", &paths)
                .expect("identity")
                .expect("files");

            assert!(
                !crate::input_identity::stored_digest_matches(directory.path(), &changed)
                    .expect("miss")
            );

            fs::write(path, bytes).expect("restore");
        }

        assert_ne!(
            Some(identity),
            content_identity("changed inputs", &paths).expect("changed identity")
        );
    }

    #[test]
    fn runtime_reuse_rejects_missing_and_malformed_metadata() {
        let directory = tempfile::tempdir().expect("runtime cache");
        let metadata = directory.path().join("bray-runtime.brayrt");

        crate::input_identity::write_digest(directory.path(), "inputs").expect("identity");

        assert!(!current(directory.path(), &metadata, "inputs").expect("missing metadata"));

        for bytes in [&b"{"[..], &[255][..]] {
            fs::write(&metadata, bytes).expect("metadata");
            assert!(!current(directory.path(), &metadata, "inputs").expect("malformed metadata"));
        }
    }
}
