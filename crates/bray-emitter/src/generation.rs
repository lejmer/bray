use std::path::{Path, PathBuf};

use bray_base::lowercase_hex;

use crate::{ArtifactId, EmittedArtifactSet, OutputSink};

/// Stable content identity of one complete managed product generation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProductGenerationIdentity([u8; 32]);

impl ProductGenerationIdentity {
    pub(crate) const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the fixed-width identity bytes.
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }

    /// Returns the canonical lowercase hexadecimal identity.
    pub fn to_hex(self) -> String {
        lowercase_hex(&self.0)
    }
}

/// One complete product generation made visible through an atomic reference update.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedProductGeneration {
    identity: ProductGenerationIdentity,
    manifest_digest: [u8; 32],
    root: PathBuf,
    reference: PathBuf,
    artifacts: EmittedArtifactSet,
}

impl PublishedProductGeneration {
    pub(crate) fn new(
        identity: ProductGenerationIdentity,
        manifest_digest: [u8; 32],
        root: PathBuf,
        reference: PathBuf,
        artifacts: EmittedArtifactSet,
    ) -> Self {
        Self {
            identity,
            manifest_digest,
            root,
            reference,
            artifacts,
        }
    }

    /// Returns the content identity of the complete generation.
    pub const fn identity(&self) -> ProductGenerationIdentity {
        self.identity
    }

    /// Returns the digest of the canonical generation manifest.
    pub const fn manifest_digest(&self) -> &[u8; 32] {
        &self.manifest_digest
    }

    /// Returns the managed product root that owns this generation.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the single publication reference that exposes this generation.
    pub fn reference(&self) -> &Path {
        &self.reference
    }

    /// Returns the complete canonical artifact records from the published manifest.
    pub const fn artifacts(&self) -> &EmittedArtifactSet {
        &self.artifacts
    }

    /// Resolves one managed artifact to its immutable generation path.
    pub fn artifact_path(&self, id: &ArtifactId) -> Option<PathBuf> {
        let artifact = self.artifacts.artifact(id)?;

        let OutputSink::ManagedFilesystem {
            root,
            artifact: relative,
        } = artifact.sink()
        else {
            return None;
        };

        if root != &self.root {
            return None;
        }

        Some(
            self.root
                .join(".bray")
                .join("generations")
                .join(self.identity.to_hex())
                .join(relative.to_path_buf()),
        )
    }
}
