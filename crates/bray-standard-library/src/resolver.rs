use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use bray_runtime_interface::RuntimeAbiVersion;
use bray_target::TargetIdentity;

use crate::{
    STANDARD_LIBRARY_MANIFEST_FILE_NAME, StandardLibraryArtifact, StandardLibraryArtifactDigest,
    StandardLibraryBundleManifest, StandardLibraryManifestError, StandardLibraryRoot,
    decode_standard_library_manifest,
};

/// One digest-validated standard library artifact selected from a bundle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedStandardLibraryArtifact {
    metadata: StandardLibraryArtifact,
    path: Arc<Path>,
    bytes: Arc<[u8]>,
}

impl ResolvedStandardLibraryArtifact {
    /// Returns the manifest metadata that selected this artifact.
    pub const fn metadata(&self) -> &StandardLibraryArtifact {
        &self.metadata
    }

    /// Returns the exact resolved host path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the validated immutable artifact bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Shares the validated immutable artifact bytes.
    pub fn shared_bytes(&self) -> Arc<[u8]> {
        Arc::clone(&self.bytes)
    }
}

/// Demand-driven resolver for one explicitly selected immutable bundle root.
#[derive(Clone, Debug)]
pub struct StandardLibraryResolver {
    root: StandardLibraryRoot,
    manifest: Arc<OnceLock<Result<Arc<StandardLibraryBundleManifest>, StandardLibraryLoadError>>>,
    artifacts: Arc<Mutex<ArtifactCache>>,
}

impl PartialEq for StandardLibraryResolver {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
    }
}

impl Eq for StandardLibraryResolver {}

type ArtifactCache = BTreeMap<
    Arc<str>,
    Arc<OnceLock<Result<ResolvedStandardLibraryArtifact, StandardLibraryLoadError>>>,
>;

impl StandardLibraryResolver {
    /// Creates an I/O-free resolver for one configured root.
    pub fn new(root: StandardLibraryRoot) -> Self {
        Self {
            root,
            manifest: Arc::new(OnceLock::new()),
            artifacts: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// Returns the exact configured root.
    pub const fn root(&self) -> &StandardLibraryRoot {
        &self.root
    }

    /// Returns the canonical manifest, reading and validating it at most once.
    pub fn manifest(&self) -> Result<Arc<StandardLibraryBundleManifest>, StandardLibraryLoadError> {
        self.manifest
            .get_or_init(|| load_manifest(&self.root))
            .clone()
    }

    /// Returns the public package interface selected by the manifest.
    pub fn interface(&self) -> Result<ResolvedStandardLibraryArtifact, StandardLibraryLoadError> {
        let manifest = self.manifest()?;

        self.resolve(manifest.interface())
    }

    /// Returns the implementation payload companion selected by the manifest.
    pub fn implementation(
        &self,
    ) -> Result<ResolvedStandardLibraryArtifact, StandardLibraryLoadError> {
        let manifest = self.manifest()?;

        self.resolve(manifest.implementation())
    }

    /// Returns the exact artifacts selected for a target and runtime ABI.
    pub fn target_artifacts(
        &self,
        target: &TargetIdentity,
        runtime_abi: RuntimeAbiVersion,
    ) -> Result<Arc<[ResolvedStandardLibraryArtifact]>, StandardLibraryLoadError> {
        let manifest = self.manifest()?;

        let selected = manifest
            .targets()
            .binary_search_by(|candidate| candidate.target().cmp(target))
            .ok()
            .map(|index| &manifest.targets()[index])
            .ok_or_else(|| StandardLibraryLoadError::TargetUnavailable(target.clone()))?;

        if selected.runtime_abi() != runtime_abi {
            return Err(StandardLibraryLoadError::RuntimeAbiMismatch {
                target: target.clone(),
                expected: runtime_abi,
                actual: selected.runtime_abi(),
            });
        }

        selected
            .artifacts()
            .iter()
            .map(|artifact| self.resolve(artifact))
            .collect::<Result<Vec<_>, _>>()
            .map(Arc::from)
    }

    fn resolve(
        &self,
        artifact: &StandardLibraryArtifact,
    ) -> Result<ResolvedStandardLibraryArtifact, StandardLibraryLoadError> {
        let cell = {
            let mut artifacts = self
                .artifacts
                .lock()
                .map_err(|_| StandardLibraryLoadError::Infrastructure)?;

            Arc::clone(
                artifacts
                    .entry(Arc::from(artifact.path()))
                    .or_insert_with(|| Arc::new(OnceLock::new())),
            )
        };

        cell.get_or_init(|| load_artifact(&self.root, artifact))
            .clone()
    }
}

fn load_manifest(
    root: &StandardLibraryRoot,
) -> Result<Arc<StandardLibraryBundleManifest>, StandardLibraryLoadError> {
    let path = root.path().join(STANDARD_LIBRARY_MANIFEST_FILE_NAME);

    let bytes = fs::read(&path).map_err(|error| StandardLibraryLoadError::Read {
        path: path.clone(),
        kind: error.kind(),
    })?;

    decode_standard_library_manifest(&bytes)
        .map(Arc::new)
        .map_err(StandardLibraryLoadError::Manifest)
}

fn load_artifact(
    root: &StandardLibraryRoot,
    artifact: &StandardLibraryArtifact,
) -> Result<ResolvedStandardLibraryArtifact, StandardLibraryLoadError> {
    let path = artifact.beneath(root.path());

    let bytes = fs::read(&path).map_err(|error| StandardLibraryLoadError::Read {
        path: path.clone(),
        kind: error.kind(),
    })?;

    let byte_len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);

    if byte_len != artifact.byte_len() {
        return Err(StandardLibraryLoadError::ArtifactLengthMismatch {
            path,
            expected: artifact.byte_len(),
            actual: byte_len,
        });
    }

    let actual = StandardLibraryArtifactDigest::for_bytes(&bytes);

    if actual != artifact.digest() {
        return Err(StandardLibraryLoadError::ArtifactDigestMismatch {
            path,
            expected: artifact.digest(),
            actual,
        });
    }

    Ok(ResolvedStandardLibraryArtifact {
        metadata: artifact.clone(),
        path: Arc::from(path),
        bytes: Arc::from(bytes),
    })
}

/// A configured standard library root cannot satisfy an exact artifact request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StandardLibraryLoadError {
    /// A required file could not be read.
    Read {
        /// Exact selected path.
        path: PathBuf,
        /// Host I/O error category.
        kind: ErrorKind,
    },
    /// The root manifest is malformed or violates its canonical contract.
    Manifest(StandardLibraryManifestError),
    /// An artifact has a different byte length than its manifest entry.
    ArtifactLengthMismatch {
        /// Exact selected path.
        path: PathBuf,
        /// Manifest byte length.
        expected: u64,
        /// Observed byte length.
        actual: u64,
    },
    /// An artifact has a different digest than its manifest entry.
    ArtifactDigestMismatch {
        /// Exact selected path.
        path: PathBuf,
        /// Manifest digest.
        expected: StandardLibraryArtifactDigest,
        /// Observed digest.
        actual: StandardLibraryArtifactDigest,
    },
    /// The manifest does not contain the selected target.
    TargetUnavailable(TargetIdentity),
    /// The selected target exists but requires another runtime ABI.
    RuntimeAbiMismatch {
        /// Exact selected target.
        target: TargetIdentity,
        /// ABI required by the compilation request.
        expected: RuntimeAbiVersion,
        /// ABI recorded by the bundle.
        actual: RuntimeAbiVersion,
    },
    /// Resolver cache coordination failed.
    Infrastructure,
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use bray_runtime_interface::RuntimeAbiVersion;
    use bray_target::TargetIdentity;
    use tempfile::TempDir;

    use super::{StandardLibraryLoadError, StandardLibraryResolver};
    use crate::{
        StandardLibraryArtifact, StandardLibraryArtifactKind, StandardLibraryBundleManifest,
        StandardLibraryRoot, StandardLibraryTargetArtifacts, encode_standard_library_manifest,
    };

    #[test]
    fn resolver_is_demand_driven_and_caches_validated_artifacts() {
        let fixture = Fixture::new();
        let resolver = StandardLibraryResolver::new(fixture.root());

        assert!(!fixture.manifest_path().exists());

        fixture.write();

        let first = resolver
            .interface()
            .unwrap_or_else(|error| panic!("interface must resolve: {error:?}"));

        fs::write(first.path(), b"changed")
            .unwrap_or_else(|error| panic!("fixture artifact must change: {error}"));

        let second = resolver
            .interface()
            .unwrap_or_else(|error| panic!("cached interface must resolve: {error:?}"));

        assert_eq!(first, second);
    }

    #[test]
    fn resolver_selects_exact_target_and_runtime_abi() {
        let fixture = Fixture::new();

        fixture.write();

        let resolver = StandardLibraryResolver::new(fixture.root());
        let target = fixture.target();

        let artifacts = resolver
            .target_artifacts(&target, RuntimeAbiVersion::new(1, 0))
            .unwrap_or_else(|error| panic!("target artifacts must resolve: {error:?}"));

        let [archive] = artifacts.as_ref() else {
            panic!("fixture must contain one target artifact");
        };

        assert_eq!(archive.bytes(), b"archive");

        assert!(matches!(
            resolver.target_artifacts(&target, RuntimeAbiVersion::new(2, 0)),
            Err(StandardLibraryLoadError::RuntimeAbiMismatch { .. })
        ));

        let missing = TargetIdentity::try_new("aarch64-unknown-linux-gnu")
            .unwrap_or_else(|| panic!("missing target identity must be valid"));

        assert_eq!(
            resolver.target_artifacts(&missing, RuntimeAbiVersion::new(1, 0)),
            Err(StandardLibraryLoadError::TargetUnavailable(missing))
        );
    }

    #[test]
    fn resolver_rejects_artifact_bytes_that_disagree_with_the_manifest() {
        let fixture = Fixture::new();

        fixture.write();

        fs::write(fixture.archive_path(), b"changed")
            .unwrap_or_else(|error| panic!("fixture artifact must change: {error}"));

        let resolver = StandardLibraryResolver::new(fixture.root());

        assert!(matches!(
            resolver.target_artifacts(&fixture.target(), RuntimeAbiVersion::new(1, 0)),
            Err(StandardLibraryLoadError::ArtifactDigestMismatch { .. })
        ));
    }

    struct Fixture {
        directory: TempDir,
        manifest: StandardLibraryBundleManifest,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir()
                .unwrap_or_else(|error| panic!("fixture directory must exist: {error}"));

            let interface = StandardLibraryArtifact::try_for_bytes(
                StandardLibraryArtifactKind::PackageInterface,
                "interfaces/std.brayi",
                b"interface",
            )
            .unwrap_or_else(|error| panic!("interface metadata must be valid: {error:?}"));

            let implementation = StandardLibraryArtifact::try_for_bytes(
                StandardLibraryArtifactKind::PackageImplementation,
                "interfaces/std.brayimpl",
                b"implementation",
            )
            .unwrap_or_else(|error| panic!("implementation metadata must be valid: {error:?}"));

            let archive = StandardLibraryArtifact::try_for_bytes(
                StandardLibraryArtifactKind::StaticLibrary,
                "targets/x86_64-unknown-linux-gnu/1.0/libstd.a",
                b"archive",
            )
            .unwrap_or_else(|error| panic!("archive metadata must be valid: {error:?}"));

            let target = bray_target::TargetIdentity::try_new("x86_64-unknown-linux-gnu")
                .unwrap_or_else(|| panic!("target identity must be valid"));

            let target = StandardLibraryTargetArtifacts::try_new(
                target,
                bray_runtime_interface::RuntimeAbiVersion::new(1, 0),
                [archive],
            )
            .unwrap_or_else(|error| panic!("target metadata must be valid: {error:?}"));

            let manifest =
                StandardLibraryBundleManifest::try_new(interface, implementation, [target])
                    .unwrap_or_else(|error| panic!("manifest must be valid: {error:?}"));

            Self {
                directory,
                manifest,
            }
        }

        fn root(&self) -> StandardLibraryRoot {
            StandardLibraryRoot::try_new(self.directory.path())
                .unwrap_or_else(|| panic!("temporary root must be absolute"))
        }

        fn manifest_path(&self) -> PathBuf {
            self.directory.path().join("manifest.json")
        }

        fn archive_path(&self) -> PathBuf {
            self.manifest.targets()[0].artifacts()[0].beneath(self.directory.path())
        }

        fn target(&self) -> TargetIdentity {
            self.manifest.targets()[0].target().clone()
        }

        fn write(&self) {
            let interface = self.manifest.interface().beneath(self.directory.path());

            let implementation = self
                .manifest
                .implementation()
                .beneath(self.directory.path());

            let archive = self.archive_path();

            fs::create_dir_all(
                interface
                    .parent()
                    .unwrap_or_else(|| panic!("interface must have a parent")),
            )
            .unwrap_or_else(|error| panic!("interface directory must exist: {error}"));

            fs::create_dir_all(
                archive
                    .parent()
                    .unwrap_or_else(|| panic!("archive must have a parent")),
            )
            .unwrap_or_else(|error| panic!("archive directory must exist: {error}"));

            fs::write(interface, b"interface")
                .unwrap_or_else(|error| panic!("interface must be written: {error}"));

            fs::write(implementation, b"implementation")
                .unwrap_or_else(|error| panic!("implementation must be written: {error}"));

            fs::write(archive, b"archive")
                .unwrap_or_else(|error| panic!("archive must be written: {error}"));

            let bytes = encode_standard_library_manifest(&self.manifest)
                .unwrap_or_else(|error| panic!("manifest must encode: {error:?}"));

            fs::write(self.manifest_path(), bytes)
                .unwrap_or_else(|error| panic!("manifest must be written: {error}"));
        }
    }
}
