use crate::{InterfaceArtifact, InterfaceValidationError, PackageInterfaceExportBundle};

/// Encodes one validated export bundle without consulting mutable compiler state.
pub fn encode_package_interface(
    bundle: &PackageInterfaceExportBundle,
) -> Result<InterfaceArtifact, InterfaceValidationError> {
    crate::artifact::encode_interface_artifact(bundle)
}

#[cfg(test)]
mod tests {
    use crate::test_support::package_interface_export_bundle;
    use crate::{
        InterfaceLanguageRevision, InterfaceValidationPolicy, PackageInterfaceExportBundle,
        ValidatedPackageInterface, encode_package_interface,
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
    fn complete_artifacts_decode_and_reencode_to_identical_bytes() {
        let encoded = encode(&package_interface_export_bundle());

        let validated = ValidatedPackageInterface::try_new(
            encoded.shared_bytes(),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        )
        .unwrap_or_else(|error| panic!("encoded interface must validate: {error:?}"));

        let surface = validated
            .decode_identity_surface()
            .unwrap_or_else(|error| panic!("identity surface must decode: {error:?}"));

        let facts = validated
            .decode_semantic_facts(&surface)
            .unwrap_or_else(|error| panic!("semantic facts must decode: {error:?}"));

        let decoded = PackageInterfaceExportBundle::try_new(
            surface,
            facts,
            InterfaceLanguageRevision::new(0),
        )
        .unwrap_or_else(|error| panic!("decoded export bundle must validate: {error:?}"));

        assert_eq!(encode(&decoded), encoded);
    }

    fn encode(bundle: &PackageInterfaceExportBundle) -> crate::InterfaceArtifact {
        encode_package_interface(bundle)
            .unwrap_or_else(|error| panic!("valid export bundle must encode: {error:?}"))
    }
}
