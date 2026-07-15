use std::sync::Arc;

use crate::{
    InterfaceArtifactHash, InterfaceContentHash, InterfaceLanguageRevision, InterfaceSemanticFacts,
    InterfaceValidationError, InterfaceValidationLimits, PackageInterfaceSurface,
};

/// One validated immutable library surface ready for deterministic interface encoding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageInterfaceExportBundle {
    surface: PackageInterfaceSurface,
    semantic_facts: InterfaceSemanticFacts,
    language_revision: InterfaceLanguageRevision,
}

impl PackageInterfaceExportBundle {
    /// Validates and freezes one structurally complete package-interface export graph.
    pub fn try_new(
        surface: PackageInterfaceSurface,
        semantic_facts: InterfaceSemanticFacts,
        language_revision: InterfaceLanguageRevision,
    ) -> Result<Self, InterfaceValidationError> {
        semantic_facts.validate(&surface, InterfaceValidationLimits::default())?;

        Ok(Self {
            surface,
            semantic_facts,
            language_revision,
        })
    }

    /// Returns the canonical exported package and symbol surface.
    pub const fn surface(&self) -> &PackageInterfaceSurface {
        &self.surface
    }

    /// Returns the complete semantic and private support graph.
    pub const fn semantic_facts(&self) -> &InterfaceSemanticFacts {
        &self.semantic_facts
    }

    /// Returns the language semantic revision used to interpret the surface.
    pub const fn language_revision(&self) -> InterfaceLanguageRevision {
        self.language_revision
    }
}

/// Complete deterministic bytes and identities of one encoded package interface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedPackageInterface {
    bytes: Arc<[u8]>,
    content_hash: InterfaceContentHash,
    artifact_hash: InterfaceArtifactHash,
}

impl EncodedPackageInterface {
    pub(crate) fn new(
        bytes: Vec<u8>,
        content_hash: InterfaceContentHash,
        artifact_hash: InterfaceArtifactHash,
    ) -> Self {
        Self {
            bytes: bytes.into(),
            content_hash,
            artifact_hash,
        }
    }

    /// Returns the complete canonical artifact bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the semantic content identity of the encoded interface.
    pub const fn content_hash(&self) -> InterfaceContentHash {
        self.content_hash
    }

    /// Returns the identity of the exact encoded artifact bytes.
    pub const fn artifact_hash(&self) -> InterfaceArtifactHash {
        self.artifact_hash
    }
}

impl AsRef<[u8]> for EncodedPackageInterface {
    fn as_ref(&self) -> &[u8] {
        self.bytes()
    }
}

/// Encodes one validated export bundle without consulting mutable compiler state.
pub fn encode_package_interface(
    bundle: &PackageInterfaceExportBundle,
) -> Result<EncodedPackageInterface, InterfaceValidationError> {
    crate::artifact::encode_interface_artifact(bundle)
}

#[cfg(test)]
mod tests {
    use crate::test_support::package_interface_export_bundle;
    use crate::{
        InterfaceLanguageRevision, InterfaceValidationError, InterfaceValidationPolicy,
        PackageInterfaceExportBundle, ValidatedPackageInterface, encode_package_interface,
    };

    #[test]
    fn equivalent_bundles_encode_to_identical_bytes_and_hashes() {
        let first = encode(&package_interface_export_bundle());
        let second = encode(&package_interface_export_bundle());

        assert_eq!(first, second);
    }

    #[test]
    fn encoded_artifacts_publish_the_hashes_validated_from_their_bytes() {
        let encoded = encode(&package_interface_export_bundle());

        let validated = ValidatedPackageInterface::try_new(
            encoded.bytes(),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        )
        .unwrap_or_else(|error| panic!("encoded interface must validate: {error:?}"));

        assert_eq!(encoded.content_hash(), validated.header().content_hash());
        assert_eq!(encoded.artifact_hash(), validated.header().artifact_hash());
    }

    #[test]
    fn bundles_reject_incomplete_support_graphs_before_encoding() {
        let complete = package_interface_export_bundle();

        let facts = complete.semantic_facts().clone().with_templates(
            complete.semantic_facts().checked_templates.iter().cloned(),
            [],
            [],
        );

        assert_eq!(
            PackageInterfaceExportBundle::try_new(
                complete.surface().clone(),
                facts,
                InterfaceLanguageRevision::new(0),
            ),
            Err(InterfaceValidationError::Malformed)
        );
    }

    #[test]
    fn export_contracts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<PackageInterfaceExportBundle>();
        assert_send_sync::<crate::EncodedPackageInterface>();
    }

    fn encode(bundle: &PackageInterfaceExportBundle) -> crate::EncodedPackageInterface {
        encode_package_interface(bundle)
            .unwrap_or_else(|error| panic!("valid export bundle must encode: {error:?}"))
    }
}
