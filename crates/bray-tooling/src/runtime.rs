use std::io;
use std::path::{Path, PathBuf};

use bray_runtime_interface::{
    RuntimeAbiVersion, RuntimeArtifact, RuntimeArtifactBuildError, RuntimeArtifactMetadata,
    RuntimeArtifactMetadataDecodeError,
};
use bray_target::TargetIdentity;

/// A failure to load the exact runtime artifact selected for one compilation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeArtifactLoadError {
    /// The selected metadata path could not be read.
    MetadataRead {
        /// Exact selected metadata path.
        path: PathBuf,
        /// Stable host I/O failure category.
        kind: io::ErrorKind,
    },
    /// The selected metadata document is malformed or unsupported.
    InvalidMetadata {
        /// Exact selected metadata path.
        path: PathBuf,
        /// Typed metadata decoding failure.
        source: RuntimeArtifactMetadataDecodeError,
    },
    /// The selected runtime artifact targets another compilation target.
    IncompatibleTarget {
        /// Exact selected metadata path.
        path: PathBuf,
        /// Target selected by the compilation.
        expected: TargetIdentity,
        /// Target declared by the runtime artifact.
        actual: TargetIdentity,
    },
    /// The selected runtime artifact implements another private runtime ABI.
    IncompatibleRuntimeAbi {
        /// Exact selected metadata path.
        path: PathBuf,
        /// Runtime ABI selected by the compilation.
        expected: RuntimeAbiVersion,
        /// Runtime ABI declared by the runtime artifact.
        actual: RuntimeAbiVersion,
    },
    /// The metadata could not be resolved to its declared archive paths.
    InvalidArtifact {
        /// Exact selected metadata path.
        path: PathBuf,
        /// Typed artifact construction failure.
        source: RuntimeArtifactBuildError,
    },
}

/// Loads and validates one target-compatible runtime artifact from its metadata path.
pub fn load_runtime_artifact(
    metadata_path: &Path,
    expected_target: &TargetIdentity,
    expected_runtime_abi: RuntimeAbiVersion,
) -> Result<RuntimeArtifact, RuntimeArtifactLoadError> {
    let metadata_bytes =
        std::fs::read(metadata_path).map_err(|error| RuntimeArtifactLoadError::MetadataRead {
            path: metadata_path.to_path_buf(),
            kind: error.kind(),
        })?;

    let metadata = RuntimeArtifactMetadata::decode_json(&metadata_bytes).map_err(|source| {
        RuntimeArtifactLoadError::InvalidMetadata {
            path: metadata_path.to_path_buf(),
            source,
        }
    })?;

    if metadata.contract().target() != expected_target {
        return Err(RuntimeArtifactLoadError::IncompatibleTarget {
            path: metadata_path.to_path_buf(),
            expected: expected_target.clone(),
            actual: metadata.contract().target().clone(),
        });
    }

    if metadata.contract().abi_version() != expected_runtime_abi {
        return Err(RuntimeArtifactLoadError::IncompatibleRuntimeAbi {
            path: metadata_path.to_path_buf(),
            expected: expected_runtime_abi,
            actual: metadata.contract().abi_version(),
        });
    }

    let absolute_path = std::path::absolute(metadata_path).map_err(|error| {
        RuntimeArtifactLoadError::MetadataRead {
            path: metadata_path.to_path_buf(),
            kind: error.kind(),
        }
    })?;

    let directory = absolute_path.parent().unwrap_or_else(|| Path::new(""));

    let components = metadata
        .components()
        .iter()
        .map(|component| {
            (
                component.identity().clone(),
                directory.join(component.archive_file_name()),
            )
        })
        .collect::<Vec<_>>();

    RuntimeArtifact::try_new(metadata, components).map_err(|source| {
        RuntimeArtifactLoadError::InvalidArtifact {
            path: metadata_path.to_path_buf(),
            source,
        }
    })
}

#[cfg(test)]
mod tests {
    use bray_runtime_interface::{
        PanicAbiIdentity, ProtectedFrameAbiVersions, RuntimeAbiVersion,
        RuntimeArtifactComponentMetadata, RuntimeArtifactDigest, RuntimeArtifactId,
        RuntimeArtifactMetadata, RuntimeArtifactPurpose, RuntimeCapability, RuntimeContract,
        RuntimeIdentity,
    };
    use bray_target::TargetIdentity;

    use super::{RuntimeArtifactLoadError, load_runtime_artifact};

    #[test]
    fn loader_preserves_metadata_read_and_decode_failures() {
        let directory = bray_testing::unique_temporary_directory();
        let missing = directory.join("missing.brayrt");

        assert_eq!(
            load_runtime_artifact(
                &missing,
                &target("x86_64-unknown-linux-gnu"),
                RuntimeAbiVersion::new(1, 0),
            ),
            Err(RuntimeArtifactLoadError::MetadataRead {
                path: missing,
                kind: std::io::ErrorKind::NotFound,
            })
        );

        std::fs::create_dir_all(&directory)
            .unwrap_or_else(|error| panic!("test runtime directory must be created: {error}"));

        let malformed = directory.join("malformed.brayrt");

        std::fs::write(&malformed, b"not metadata")
            .unwrap_or_else(|error| panic!("malformed metadata must be written: {error}"));

        assert!(matches!(
            load_runtime_artifact(
                &malformed,
                &target("x86_64-unknown-linux-gnu"),
                RuntimeAbiVersion::new(1, 0),
            ),
            Err(RuntimeArtifactLoadError::InvalidMetadata { path, .. }) if path == malformed
        ));

        std::fs::remove_dir_all(directory)
            .unwrap_or_else(|error| panic!("test runtime directory must be removed: {error}"));
    }

    #[test]
    fn loader_validates_target_and_runtime_abi_before_resolving_archives() {
        let directory = bray_testing::unique_temporary_directory();

        std::fs::create_dir_all(&directory)
            .unwrap_or_else(|error| panic!("test runtime directory must be created: {error}"));

        let metadata_path = directory.join("bray-runtime.brayrt");
        let metadata = metadata("x86_64-pc-windows-msvc", RuntimeAbiVersion::new(1, 0));

        let bytes = metadata
            .encode_json()
            .unwrap_or_else(|error| panic!("test runtime metadata must encode: {error:?}"));

        std::fs::write(&metadata_path, bytes)
            .unwrap_or_else(|error| panic!("test runtime metadata must be written: {error}"));

        assert_eq!(
            load_runtime_artifact(
                &metadata_path,
                &target("x86_64-unknown-linux-gnu"),
                RuntimeAbiVersion::new(1, 0),
            ),
            Err(RuntimeArtifactLoadError::IncompatibleTarget {
                path: metadata_path.clone(),
                expected: target("x86_64-unknown-linux-gnu"),
                actual: target("x86_64-pc-windows-msvc"),
            })
        );

        assert_eq!(
            load_runtime_artifact(
                &metadata_path,
                &target("x86_64-pc-windows-msvc"),
                RuntimeAbiVersion::new(2, 0),
            ),
            Err(RuntimeArtifactLoadError::IncompatibleRuntimeAbi {
                path: metadata_path,
                expected: RuntimeAbiVersion::new(2, 0),
                actual: RuntimeAbiVersion::new(1, 0),
            })
        );

        std::fs::remove_dir_all(directory)
            .unwrap_or_else(|error| panic!("test runtime directory must be removed: {error}"));
    }

    #[test]
    #[cfg(feature = "compiler")]
    fn relative_runtime_metadata_resolves_archives_independently_of_linker_working_directory() {
        let directory = tempfile::tempdir_in(".").unwrap();
        let metadata_path = directory.path().join("bray-runtime.brayrt");
        let metadata = metadata("x86_64-pc-windows-msvc", RuntimeAbiVersion::new(1, 0));

        std::fs::write(&metadata_path, metadata.encode_json().unwrap()).unwrap();

        let artifact = load_runtime_artifact(
            &metadata_path,
            &target("x86_64-pc-windows-msvc"),
            RuntimeAbiVersion::new(1, 0),
        )
        .unwrap();

        let directory = std::path::absolute(directory.path()).unwrap();

        for component in artifact.components() {
            assert_eq!(
                component.archive(),
                directory.join(component.metadata().archive_file_name())
            );

            assert!(component.archive().is_absolute());
        }
    }

    fn metadata(target_identity: &str, abi: RuntimeAbiVersion) -> RuntimeArtifactMetadata {
        let digest = RuntimeArtifactDigest::new([0; 32]);

        let contract = RuntimeContract::try_new(
            RuntimeIdentity::try_new("bray.runtime.test")
                .unwrap_or_else(|| panic!("test runtime identity must be valid")),
            RuntimeArtifactId::try_new("bray.runtime.test.artifact")
                .unwrap_or_else(|| panic!("test artifact identity must be valid")),
            abi,
            ProtectedFrameAbiVersions::uniform(abi),
            target(target_identity),
            PanicAbiIdentity::try_new("bray.panic.test")
                .unwrap_or_else(|| panic!("test panic ABI must be valid")),
            [RuntimeCapability::CooperativeExecution],
            [],
        )
        .unwrap_or_else(|error| panic!("test runtime contract must be valid: {error:?}"));

        let components = [
            RuntimeArtifactPurpose::Product,
            RuntimeArtifactPurpose::TestRunner,
        ]
        .into_iter()
        .map(|purpose| {
            let name = match purpose {
                RuntimeArtifactPurpose::Product => "product",
                RuntimeArtifactPurpose::TestRunner => "test",
            };

            RuntimeArtifactComponentMetadata::try_new(
                RuntimeArtifactId::try_new(format!("runtime.{name}"))
                    .unwrap_or_else(|| panic!("test component identity must be valid")),
                purpose,
                [],
                [RuntimeCapability::CooperativeExecution],
                format!("runtime-{name}.a"),
                digest,
            )
            .unwrap_or_else(|error| panic!("test runtime component must be valid: {error:?}"))
        });

        RuntimeArtifactMetadata::try_new(contract, components)
            .unwrap_or_else(|error| panic!("test runtime metadata must be valid: {error:?}"))
    }

    fn target(identity: &str) -> TargetIdentity {
        TargetIdentity::try_new(identity)
            .unwrap_or_else(|| panic!("test target identity must be valid"))
    }
}
