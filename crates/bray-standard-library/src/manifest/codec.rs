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
    use bray_base::NonEmptySharedStr;
    use bray_runtime_interface::{PlatformServiceRole, RuntimeAbiVersion};
    use bray_symbols::{NativeLinkKind, NativeLinkRequirement};
    use bray_target::TargetIdentity;

    use super::{decode_standard_library_manifest, encode_standard_library_manifest};
    use crate::{
        StandardLibraryArtifact, StandardLibraryArtifactKind, StandardLibraryBundleManifest,
        StandardLibraryManifestError, StandardLibraryTargetArtifacts,
    };

    #[test]
    fn canonical_manifests_round_trip_and_reproduce_bundle_identity() {
        let manifest = manifest();

        let first = encode_standard_library_manifest(&manifest)
            .unwrap_or_else(|error| panic!("manifest must encode: {error:?}"));

        let decoded = decode_standard_library_manifest(&first)
            .unwrap_or_else(|error| panic!("manifest must decode: {error:?}"));

        let second = encode_standard_library_manifest(&decoded)
            .unwrap_or_else(|error| panic!("decoded manifest must encode: {error:?}"));

        assert_eq!(decoded, manifest);
        assert_eq!(second, first);
    }

    #[test]
    fn manifest_decoder_rejects_noncanonical_and_tampered_input() {
        let canonical = encode_standard_library_manifest(&manifest())
            .unwrap_or_else(|error| panic!("manifest must encode: {error:?}"));

        let mut whitespace = canonical.clone();
        whitespace.push(b'\n');

        assert_eq!(
            decode_standard_library_manifest(&whitespace),
            Err(StandardLibraryManifestError::NonCanonicalEncoding)
        );

        let tampered = String::from_utf8(canonical)
            .unwrap_or_else(|error| panic!("manifest must be UTF-8: {error}"))
            .replace("\"byte_len\":9", "\"byte_len\":8")
            .into_bytes();

        assert_eq!(
            decode_standard_library_manifest(&tampered),
            Err(StandardLibraryManifestError::BundleDigestMismatch)
        );
    }

    #[test]
    fn platform_archives_require_disjoint_capability_inventories() {
        let provider = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::PlatformServiceLibrary,
            "targets/x86_64-unknown-linux-gnu/1.0/libplatform.a",
            b"provider",
        )
        .map(|artifact| artifact.with_platform_services([PlatformServiceRole::StandardOutputWrite]))
        .unwrap_or_else(|error| panic!("platform archive must be valid: {error:?}"));

        let target = manifest().targets()[0].clone();

        let duplicate_provider = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::PlatformServiceLibrary,
            "targets/x86_64-unknown-linux-gnu/1.0/libplatform-duplicate.a",
            b"duplicate provider",
        )
        .map(|artifact| artifact.with_platform_services([PlatformServiceRole::StandardOutputWrite]))
        .unwrap_or_else(|error| panic!("platform archive must be valid: {error:?}"));

        let artifacts = target
            .artifacts()
            .iter()
            .cloned()
            .chain([provider, duplicate_provider]);

        assert_eq!(
            StandardLibraryTargetArtifacts::try_new(
                target.target().clone(),
                target.runtime_abi(),
                artifacts,
            ),
            Err(StandardLibraryManifestError::DuplicatePlatformService)
        );

        let error = StandardLibraryTargetArtifacts::try_new(
            target.target().clone(),
            target.runtime_abi(),
            target
                .artifacts()
                .iter()
                .cloned()
                .chain([StandardLibraryArtifact::try_for_bytes(
                    StandardLibraryArtifactKind::PlatformServiceLibrary,
                    "targets/x86_64-unknown-linux-gnu/1.0/libplatform-empty.a",
                    b"provider",
                )
                .unwrap_or_else(|error| panic!("platform archive must be valid: {error:?}"))]),
        )
        .expect_err("platform archives must declare capabilities");

        assert_eq!(error, StandardLibraryManifestError::InvalidPlatformServices);

        let ordinary_native_link = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::StaticLibrary,
            "targets/x86_64-unknown-linux-gnu/1.0/libinvalid-native-link.a",
            b"invalid native link owner",
        )
        .map(|artifact| {
            artifact.with_native_links([NativeLinkRequirement::new(
                NonEmptySharedStr::try_new("c")
                    .unwrap_or_else(|| panic!("native library name must be valid")),
                NativeLinkKind::System,
            )])
        })
        .unwrap_or_else(|error| panic!("artifact metadata must be valid: {error:?}"));

        assert_eq!(
            StandardLibraryTargetArtifacts::try_new(
                target.target().clone(),
                target.runtime_abi(),
                target
                    .artifacts()
                    .iter()
                    .cloned()
                    .chain([ordinary_native_link]),
            ),
            Err(StandardLibraryManifestError::InvalidNativeLink)
        );
    }

    fn manifest() -> StandardLibraryBundleManifest {
        let interface = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::PackageInterface,
            "targets/x86_64-unknown-linux-gnu/1.0/std.brayi",
            b"interface",
        )
        .unwrap_or_else(|error| panic!("interface must be valid: {error:?}"));

        let implementation = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::PackageImplementation,
            "targets/x86_64-unknown-linux-gnu/1.0/std.brayimpl",
            b"implementation",
        )
        .unwrap_or_else(|error| panic!("implementation must be valid: {error:?}"));

        let archive = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::StaticLibrary,
            "targets/x86_64-unknown-linux-gnu/1.0/libstd.a",
            b"archive",
        )
        .unwrap_or_else(|error| panic!("archive must be valid: {error:?}"));

        let provider = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::PlatformServiceLibrary,
            "targets/x86_64-unknown-linux-gnu/1.0/libplatform-core.a",
            b"provider",
        )
        .map(|artifact| artifact.with_platform_services([PlatformServiceRole::ClockMonotonicNow]))
        .map(|artifact| {
            artifact.with_native_links([NativeLinkRequirement::new(
                NonEmptySharedStr::try_new("c")
                    .unwrap_or_else(|| panic!("native library name must be valid")),
                NativeLinkKind::System,
            )])
        })
        .unwrap_or_else(|error| panic!("provider must be valid: {error:?}"));

        let target = TargetIdentity::try_new("x86_64-unknown-linux-gnu")
            .unwrap_or_else(|| panic!("target identity must be valid"));

        let target = StandardLibraryTargetArtifacts::try_new(
            target,
            RuntimeAbiVersion::new(1, 0),
            [interface, implementation, archive, provider],
        )
        .unwrap_or_else(|error| panic!("target artifacts must be valid: {error:?}"));

        StandardLibraryBundleManifest::try_new([target])
            .unwrap_or_else(|error| panic!("manifest must be valid: {error:?}"))
    }
}
