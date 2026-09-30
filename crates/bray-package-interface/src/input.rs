use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

static NEXT_FILE_SNAPSHOT: AtomicU64 = AtomicU64::new(1);

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind, SeverityKind,
};

use crate::{
    InterfaceArtifactHash, InterfaceValidationError, InterfaceValidationLimits,
    PackageImplementationArtifact,
};

/// One immutable package artifact input, loaded only when its consumer demands it.
#[derive(Clone, Debug)]
pub struct PackageArtifactInput {
    path: Arc<Path>,
    expected_digest: Option<[u8; 32]>,
    file_snapshot: Option<u64>,
    supplied_bytes: Option<Arc<[u8]>>,
    bytes: Arc<OnceLock<Result<Arc<[u8]>, PackageArtifactLoadError>>>,
    implementation: Arc<OnceLock<Result<PackageImplementationArtifact, PackageArtifactLoadError>>>,
}

impl PartialEq for PackageArtifactInput {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
            && self.expected_digest == other.expected_digest
            && self.file_snapshot == other.file_snapshot
            && self.supplied_bytes == other.supplied_bytes
    }
}

impl Eq for PackageArtifactInput {}

impl std::hash::Hash for PackageArtifactInput {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(
            &(
                &self.path,
                self.expected_digest,
                self.file_snapshot,
                &self.supplied_bytes,
            ),
            state,
        );
    }
}

impl PackageArtifactInput {
    /// Selects a file with an optional BLAKE3 content digest without opening it.
    /// Clones retain the same lazy read snapshot.
    /// Without a content digest, a new handle establishes a new cache identity.
    pub fn file(path: impl Into<PathBuf>, expected_digest: Option<[u8; 32]>) -> Self {
        Self {
            path: Arc::from(path.into()),
            expected_digest,
            supplied_bytes: None,
            file_snapshot: expected_digest.is_none().then(|| {
                NEXT_FILE_SNAPSHOT
                    .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                        current.checked_add(1)
                    })
                    .expect("package artifact snapshot identities exhausted")
            }),
            bytes: Arc::new(OnceLock::new()),
            implementation: Arc::new(OnceLock::new()),
        }
    }

    /// Supplies immutable bytes directly, as used by in-process publication and tests.
    pub fn memory(path: impl Into<PathBuf>, bytes: impl Into<Arc<[u8]>>) -> Self {
        let mut input = Self::file(path, None);
        input.file_snapshot = None;
        input.supplied_bytes = Some(bytes.into());

        input
    }

    /// Supplies an implementation that has already passed container validation.
    pub fn implementation(
        path: impl Into<PathBuf>,
        artifact: PackageImplementationArtifact,
    ) -> Self {
        let input = Self::memory(path, artifact.shared_bytes());

        input
            .implementation
            .set(Ok(artifact))
            .expect("new artifact input cache must be empty");

        input
    }

    /// Returns the selected path for provenance and diagnostics.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns bytes supplied by the request, without loading a file.
    pub fn supplied_bytes(&self) -> Option<&[u8]> {
        self.supplied_bytes.as_deref()
    }

    /// Returns the optional digest supplied by immutable package discovery.
    pub const fn expected_digest(&self) -> Option<[u8; 32]> {
        self.expected_digest
    }

    /// Reads and authenticates this input at most once within its immutable request.
    pub fn read(&self) -> Result<Arc<[u8]>, PackageArtifactLoadError> {
        self.bytes
            .get_or_init(|| {
                let bytes = match &self.supplied_bytes {
                    Some(bytes) => Arc::clone(bytes),
                    None => Arc::from(
                        std::fs::read(&self.path)
                            .map_err(|error| PackageArtifactLoadError::Read(error.kind()))?,
                    ),
                };

                if let Some(expected) = self.expected_digest {
                    let actual = *blake3::hash(&bytes).as_bytes();

                    if expected != actual {
                        return Err(PackageArtifactLoadError::Validation(
                            InterfaceValidationError::ArtifactHashMismatch {
                                expected: InterfaceArtifactHash::from_bytes(expected),
                                actual: InterfaceArtifactHash::from_bytes(actual),
                            },
                        ));
                    }
                }

                Ok(bytes)
            })
            .clone()
    }

    /// Loads and validates an implementation container through the common package path.
    pub fn load_implementation(
        &self,
    ) -> Result<PackageImplementationArtifact, PackageArtifactLoadError> {
        self.implementation
            .get_or_init(|| {
                PackageImplementationArtifact::try_from_bytes(
                    self.read()?,
                    InterfaceValidationLimits::default(),
                )
                .map_err(PackageArtifactLoadError::Validation)
            })
            .clone()
    }
}

/// An exact external failure while loading a selected package artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageArtifactLoadError {
    /// The selected artifact could not be read.
    Read(std::io::ErrorKind),
    /// The artifact failed integrity or container validation.
    Validation(InterfaceValidationError),
}

impl PackageArtifactLoadError {
    /// Preserves the exact external cause as a structured compiler diagnostic.
    pub fn into_diagnostic(self, id: DiagnosticId) -> Diagnostic {
        match self {
            Self::Read(kind) => Diagnostic::new(
                id,
                DiagnosticKind::PackageArtifactReadFailed,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(
                kind,
            ))),
            Self::Validation(error) => error.into_diagnostic(id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PackageArtifactInput, PackageArtifactLoadError};
    use crate::InterfaceValidationError;
    use std::hash::{Hash, Hasher};

    fn fingerprint(input: &PackageArtifactInput) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        input.hash(&mut hasher);

        hasher.finish()
    }

    #[test]
    fn file_handles_load_on_demand_and_isolate_replaced_files() {
        let directory = tempfile::tempdir().expect("temporary artifact directory");
        let path = directory.path().join("library.brayimpl");
        let first = PackageArtifactInput::file(&path, None);
        let clone = first.clone();
        let identity = fingerprint(&first);
        std::fs::write(&path, b"first").expect("write first artifact");
        assert_eq!(first.read().expect("lazy read").as_ref(), b"first");
        std::fs::write(&path, b"second").expect("replace artifact");
        let second = PackageArtifactInput::file(&path, None);
        assert_eq!(clone.read().expect("same snapshot").as_ref(), b"first");
        assert_eq!(second.read().expect("new snapshot").as_ref(), b"second");
        assert_eq!(identity, fingerprint(&first));
        assert_eq!(first, clone);
        assert_ne!(first, second);
        assert_ne!(identity, fingerprint(&second));
    }

    #[test]
    fn known_content_identity_is_reusable_and_verified() {
        let directory = tempfile::tempdir().expect("temporary artifact directory");
        let path = directory.path().join("library.brayimpl");
        let digest = *blake3::hash(b"expected").as_bytes();
        let first = PackageArtifactInput::file(&path, Some(digest));
        let second = PackageArtifactInput::file(&path, Some(digest));
        assert_eq!(first, second);
        std::fs::write(&path, b"wrong").expect("write mismatched artifact");

        assert!(matches!(
            first.read(),
            Err(PackageArtifactLoadError::Validation(
                InterfaceValidationError::ArtifactHashMismatch { .. }
            ))
        ));

        std::fs::write(&path, b"expected").expect("write expected artifact");

        assert_eq!(
            second.read().expect("authenticated read").as_ref(),
            b"expected"
        );
    }
}
