use crate::{ArtifactContent, ArtifactDigest, BackendArtifactId};

/// One immutable logically identified artifact contribution produced by a backend.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BackendArtifactContribution {
    id: BackendArtifactId,
    content: ArtifactContent,
    digest: Option<ArtifactDigest>,
    native_unit: Option<bray_native_artifact::NativeUnit>,
}

impl BackendArtifactContribution {
    /// Creates one complete backend artifact contribution.
    pub const fn new(
        id: BackendArtifactId,
        content: ArtifactContent,
        digest: Option<ArtifactDigest>,
    ) -> Self {
        Self {
            id,
            content,
            digest,
            native_unit: None,
        }
    }

    /// Attaches native selection and retention obligations for these exact artifact bytes.
    /// The unit must describe this contribution's final bytes and physical artifact kind.
    pub fn with_native_unit(mut self, unit: bray_native_artifact::NativeUnit) -> Self {
        self.digest = Some(
            ArtifactDigest::try_new(
                crate::ArtifactDigestAlgorithm::Sha256,
                unit.digest().bytes(),
            )
            .expect("native content digest must be SHA-256"),
        );

        self.native_unit = Some(unit);

        self
    }

    /// Returns final-byte native selection and retention obligations when supplied.
    pub const fn native_unit(&self) -> Option<&bray_native_artifact::NativeUnit> {
        self.native_unit.as_ref()
    }

    /// Returns the planned logical contribution identity.
    pub const fn id(&self) -> &BackendArtifactId {
        &self.id
    }

    /// Returns the immutable artifact content.
    pub const fn content(&self) -> &ArtifactContent {
        &self.content
    }

    /// Returns the deterministic content digest when one was requested.
    pub const fn digest(&self) -> Option<&ArtifactDigest> {
        self.digest.as_ref()
    }
}
