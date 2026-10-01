use bray_base::NonEmptySharedStr;
use bray_runtime_interface::PlatformServiceRole;
use bray_symbols::{NativeLinkKind, NativeLinkRequirement};
use bray_target::TargetIdentity;

use crate::{
    PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY, PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY,
    PUBLIC_STANDARD_LIBRARY_SURFACE_IDENTITY,
};

use super::model::{
    StandardLibraryArtifact, StandardLibraryArtifactDigest, StandardLibraryArtifactKind,
    StandardLibraryBundleDigest, StandardLibraryBundleManifest, StandardLibraryManifestError,
    StandardLibraryTargetArtifacts,
};
use super::wire::{
    MANIFEST_FORMAT_REVISION, OwnedArtifactWire, OwnedNativeLinkWire, OwnedPublishedWire,
    decode_digest, encode_published, runtime_abi,
};

/// Encodes a manifest using its canonical compact UTF-8 JSON representation.
pub fn encode_standard_library_manifest(
    manifest: &StandardLibraryBundleManifest,
) -> Result<Vec<u8>, StandardLibraryManifestError> {
    encode_published(manifest.targets(), manifest.bundle_digest())
}

/// Decodes and validates one canonical standard library bundle manifest.
pub fn decode_standard_library_manifest(
    bytes: &[u8],
) -> Result<StandardLibraryBundleManifest, StandardLibraryManifestError> {
    let wire: OwnedPublishedWire =
        serde_json::from_slice(bytes).map_err(|_| StandardLibraryManifestError::Malformed)?;

    validate_identity(&wire)?;

    let published_digest = StandardLibraryBundleDigest::new(decode_digest(wire.bundle_digest)?);

    let targets = wire
        .targets
        .into_iter()
        .map(|target| {
            let identity = TargetIdentity::try_new(target.target)
                .ok_or(StandardLibraryManifestError::InvalidIdentity)?;

            let artifacts = target
                .artifacts
                .into_iter()
                .map(decode_artifact)
                .collect::<Result<Vec<_>, _>>()?;

            StandardLibraryTargetArtifacts::try_new(
                identity,
                runtime_abi(target.runtime_abi),
                artifacts,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    let manifest = StandardLibraryBundleManifest::try_new(targets)?;

    if manifest.bundle_digest() != published_digest {
        return Err(StandardLibraryManifestError::BundleDigestMismatch);
    }

    let canonical = encode_standard_library_manifest(&manifest)?;

    if canonical != bytes {
        return Err(StandardLibraryManifestError::NonCanonicalEncoding);
    }

    Ok(manifest)
}

fn validate_identity(wire: &OwnedPublishedWire) -> Result<(), StandardLibraryManifestError> {
    if wire.format != MANIFEST_FORMAT_REVISION
        || wire.package != PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY
        || wire.product != PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY
        || wire.product_kind != "library"
        || wire.public_surface != PUBLIC_STANDARD_LIBRARY_SURFACE_IDENTITY
    {
        return Err(StandardLibraryManifestError::InvalidIdentity);
    }

    Ok(())
}

fn decode_artifact(
    wire: OwnedArtifactWire,
) -> Result<StandardLibraryArtifact, StandardLibraryManifestError> {
    let kind = StandardLibraryArtifactKind::for_str(&wire.kind)
        .ok_or(StandardLibraryManifestError::Malformed)?;

    let digest = StandardLibraryArtifactDigest::new(decode_digest(wire.digest)?);

    let metadata_digest = wire
        .metadata_digest
        .map(decode_digest)
        .transpose()?
        .map(StandardLibraryArtifactDigest::new);

    let platform_services = wire
        .platform_services
        .iter()
        .map(|role| {
            PlatformServiceRole::from_name(role)
                .ok_or(StandardLibraryManifestError::InvalidPlatformServices)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let native_links = wire
        .native_links
        .into_iter()
        .map(decode_native_link)
        .collect::<Result<Vec<_>, _>>()?;

    StandardLibraryArtifact::try_new(kind, wire.path, wire.byte_len, digest)
        .map(|artifact| artifact.with_metadata_digest(metadata_digest))
        .map(|artifact| artifact.with_platform_services(platform_services))
        .map(|artifact| artifact.with_native_links(native_links))
}

fn decode_native_link(
    wire: OwnedNativeLinkWire,
) -> Result<NativeLinkRequirement, StandardLibraryManifestError> {
    let name = NonEmptySharedStr::try_new(wire.name)
        .ok_or(StandardLibraryManifestError::InvalidNativeLink)?;

    let kind = NativeLinkKind::for_name(&wire.kind)
        .ok_or(StandardLibraryManifestError::InvalidNativeLink)?;

    Ok(NativeLinkRequirement::new(name, kind))
}

#[cfg(test)]
mod tests {
    use bray_runtime_interface::RuntimeAbiVersion;
    use bray_target::TargetIdentity;

    use super::{decode_standard_library_manifest, encode_standard_library_manifest};
    use crate::{
        StandardLibraryArtifact, StandardLibraryArtifactKind, StandardLibraryBundleManifest,
        StandardLibraryManifestError, target_artifacts_for_test,
    };

    #[test]
    fn implementation_round_trips_and_manifest_bytes_are_canonical() {
        let target = TargetIdentity::try_new("x86_64-pc-windows-msvc").unwrap();
        let prefix = format!("targets/{}/1.0", target.as_str());

        let artifacts = [
            (StandardLibraryArtifactKind::PackageInterface, "std.brayi"),
            (
                StandardLibraryArtifactKind::PackageImplementation,
                "std.brayimpl",
            ),
            (
                StandardLibraryArtifactKind::NativeImplementation,
                "std-native.brayimpl",
            ),
            (
                StandardLibraryArtifactKind::NativeDependency,
                "bray_compiler_support.brayimpl",
            ),
        ]
        .into_iter()
        .map(|(kind, name)| {
            StandardLibraryArtifact::try_for_bytes(
                kind,
                format!("{prefix}/{name}"),
                name.as_bytes(),
            )
            .unwrap()
            .with_metadata_digest(
                (kind != StandardLibraryArtifactKind::PackageInterface)
                    .then(|| crate::StandardLibraryArtifactDigest::new([7; 32])),
            )
        })
        .collect();

        let target =
            target_artifacts_for_test(target, RuntimeAbiVersion::new(1, 0), artifacts).unwrap();

        let manifest = StandardLibraryBundleManifest::try_new([target]).unwrap();
        let bytes = encode_standard_library_manifest(&manifest).unwrap();

        assert_eq!(decode_standard_library_manifest(&bytes), Ok(manifest));

        let mut whitespace = bytes.clone();

        whitespace.push(b'\n');

        assert_eq!(
            decode_standard_library_manifest(&whitespace),
            Err(StandardLibraryManifestError::NonCanonicalEncoding),
        );

        let mut tampered: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        tampered["targets"][0]["artifacts"][0]["byte_len"] = serde_json::json!(1);

        assert_eq!(
            decode_standard_library_manifest(&serde_json::to_vec(&tampered).unwrap()),
            Err(StandardLibraryManifestError::BundleDigestMismatch),
        );

        let mut metadata_tampered: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        metadata_tampered["targets"][0]["artifacts"][1]["metadata_digest"]["bytes"] =
            serde_json::json!("00".repeat(32));

        assert_eq!(
            decode_standard_library_manifest(&serde_json::to_vec(&metadata_tampered).unwrap()),
            Err(StandardLibraryManifestError::BundleDigestMismatch)
        );

        let mut wrong_revision: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        wrong_revision["format"] = serde_json::json!(2);

        assert_eq!(
            decode_standard_library_manifest(&serde_json::to_vec(&wrong_revision).unwrap()),
            Err(StandardLibraryManifestError::InvalidIdentity),
        );
    }
}
