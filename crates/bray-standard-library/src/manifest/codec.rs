use std::num::NonZeroU32;

use bray_base::NonEmptySharedStr;
use bray_runtime_interface::{BinarySymbolName, PlatformServiceRole};
use bray_symbols::{NativeLinkKind, NativeLinkRequirement};
use bray_target::{CodeModel, RelocationModel, TargetIdentity};

use crate::{
    PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY, PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY,
    PUBLIC_STANDARD_LIBRARY_SURFACE_IDENTITY,
};

use super::model::{
    StandardLibraryArtifact, StandardLibraryArtifactDigest, StandardLibraryArtifactKind,
    StandardLibraryBundleDigest, StandardLibraryBundleManifest, StandardLibraryManifestError,
    StandardLibraryOptimizationMetadataProblem, StandardLibraryTargetArtifacts,
    invalid_optimization_metadata,
};
use super::optimization::{
    StandardLibraryOptimizationCompatibility, StandardLibraryOptimizationDependency,
    StandardLibraryOptimizationFallback, StandardLibraryOptimizationLifecycleRoot,
    StandardLibraryOptimizationMetadata, StandardLibraryOptimizationProducer,
    StandardLibraryOptimizationProducerKind,
};
use super::wire::{
    MANIFEST_FORMAT_REVISION, OwnedArtifactWire, OwnedNativeLinkWire, OwnedOptimizationWire,
    OwnedPublishedWire, decode_digest, encode_published, runtime_abi,
};

use StandardLibraryOptimizationMetadataProblem as MetadataProblem;

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

    let optimization = wire.optimization.map(decode_optimization).transpose()?;

    StandardLibraryArtifact::try_new(kind, wire.path, wire.byte_len, digest)
        .map(|artifact| artifact.with_platform_services(platform_services))
        .map(|artifact| artifact.with_native_links(native_links))
        .map(|artifact| match optimization {
            Some(optimization) => artifact.with_optimization(optimization),
            None => artifact,
        })
}

fn decode_optimization(
    wire: OwnedOptimizationWire,
) -> Result<StandardLibraryOptimizationMetadata, StandardLibraryManifestError> {
    if wire.semantics != "thin_lto" {
        return Err(invalid_optimization_metadata(
            MetadataProblem::UnsupportedSemantics,
        ));
    }

    let producer_kind = match wire.producer.kind.as_str() {
        "bray" => StandardLibraryOptimizationProducerKind::Bray,
        "pinned_native" => StandardLibraryOptimizationProducerKind::PinnedNative,
        _ => {
            return Err(invalid_optimization_metadata(
                MetadataProblem::UnsupportedProducerKind,
            ));
        }
    };

    let producer = StandardLibraryOptimizationProducer::try_new(
        producer_kind,
        wire.producer.implementation,
        wire.producer.implementation_revision,
        wire.producer.toolchain,
        wire.producer.toolchain_revision,
    )?;

    let compatibility = StandardLibraryOptimizationCompatibility::try_new(
        wire.compatibility.triple,
        wire.compatibility.data_layout,
        decode_relocation_model(&wire.compatibility.relocation_model)?,
        decode_code_model(&wire.compatibility.code_model)?,
        runtime_abi(wire.compatibility.runtime_abi),
    )?;

    let fallback = StandardLibraryOptimizationFallback::try_new(
        wire.fallback.path,
        StandardLibraryArtifactDigest::new(decode_digest(wire.fallback.digest)?),
    )?;

    let module_count = NonZeroU32::new(wire.module_count)
        .ok_or(invalid_optimization_metadata(MetadataProblem::ZeroModuleCount))?;

    let preservation_roots = wire
        .preservation_roots
        .into_iter()
        .map(|name| {
            BinarySymbolName::try_new(name)
                .ok_or(invalid_optimization_metadata(
                    MetadataProblem::InvalidPreservationRoot,
                ))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let lifecycle_roots = wire
        .lifecycle_roots
        .into_iter()
        .map(|root| match root.as_str() {
            "global_constructors" => {
                Ok(StandardLibraryOptimizationLifecycleRoot::GlobalConstructors)
            }
            "global_destructors" => Ok(StandardLibraryOptimizationLifecycleRoot::GlobalDestructors),
            "exit_registration" => Ok(StandardLibraryOptimizationLifecycleRoot::ExitRegistration),
            _ => Err(invalid_optimization_metadata(
                MetadataProblem::UnsupportedLifecycleRoot,
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;

    let platform_services = wire
        .platform_services
        .into_iter()
        .map(|role| {
            PlatformServiceRole::from_name(&role)
                .ok_or(invalid_optimization_metadata(
                    MetadataProblem::UnknownPlatformService,
                ))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let dependencies = wire
        .dependencies
        .into_iter()
        .map(|dependency| {
            StandardLibraryOptimizationDependency::try_new(
                dependency.path,
                StandardLibraryArtifactDigest::new(decode_digest(dependency.digest)?),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    StandardLibraryOptimizationMetadata::try_new(
        wire.partition,
        producer,
        compatibility,
        fallback,
        module_count,
    )
    .map(|metadata| metadata.with_preservation_roots(preservation_roots))
    .map(|metadata| metadata.with_lifecycle_roots(lifecycle_roots))
    .map(|metadata| metadata.with_platform_services(platform_services))
    .map(|metadata| metadata.with_dependencies(dependencies))
}

fn decode_relocation_model(value: &str) -> Result<RelocationModel, StandardLibraryManifestError> {
    match value {
        "default" => Ok(RelocationModel::Default),
        "static" => Ok(RelocationModel::Static),
        "position_independent" => Ok(RelocationModel::PositionIndependent),
        "dynamic_no_pic" => Ok(RelocationModel::DynamicNoPic),
        _ => Err(invalid_optimization_metadata(
            MetadataProblem::UnsupportedRelocationModel,
        )),
    }
}

fn decode_code_model(value: &str) -> Result<CodeModel, StandardLibraryManifestError> {
    match value {
        "default" => Ok(CodeModel::Default),
        "tiny" => Ok(CodeModel::Tiny),
        "small" => Ok(CodeModel::Small),
        "medium" => Ok(CodeModel::Medium),
        "large" => Ok(CodeModel::Large),
        "kernel" => Ok(CodeModel::Kernel),
        _ => Err(invalid_optimization_metadata(
            MetadataProblem::UnsupportedCodeModel,
        )),
    }
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
    use std::num::NonZeroU32;

    use bray_base::NonEmptySharedStr;
    use bray_runtime_interface::{PlatformServiceRole, RuntimeAbiVersion};
    use bray_symbols::{NativeLinkKind, NativeLinkRequirement};
    use bray_target::{CodeModel, RelocationModel, TargetIdentity};

    use super::{decode_standard_library_manifest, encode_standard_library_manifest};
    use crate::{
        StandardLibraryArtifact, StandardLibraryArtifactKind, StandardLibraryBundleManifest,
        StandardLibraryManifestError, StandardLibraryOptimizationCompatibility,
        StandardLibraryOptimizationDependency, StandardLibraryOptimizationFallback,
        StandardLibraryOptimizationLifecycleRoot, StandardLibraryOptimizationMetadata,
        StandardLibraryOptimizationMetadataProblem,
        StandardLibraryOptimizationProducer, StandardLibraryOptimizationProducerKind,
        StandardLibraryTargetArtifacts,
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
    fn optimization_dependencies_resolve_to_exact_packaged_metadata() {
        let manifest = manifest();
        let target = &manifest.targets()[0];

        let result = StandardLibraryTargetArtifacts::try_new(
            target.target().clone(),
            target.runtime_abi(),
            target
                .artifacts()
                .iter()
                .filter(|artifact| {
                    artifact.kind() != StandardLibraryArtifactKind::DependencyMetadata
                })
                .cloned(),
        );

        assert_eq!(
            result,
            Err(StandardLibraryManifestError::InvalidOptimizationMetadata(
                StandardLibraryOptimizationMetadataProblem::MissingDependencyArtifact,
            ))
        );
    }

    #[test]
    fn target_artifacts_require_the_bray_optimization_partition() {
        let manifest = manifest();
        let target = &manifest.targets()[0];

        let without_optimization = StandardLibraryTargetArtifacts::try_new(
            target.target().clone(),
            target.runtime_abi(),
            target
                .artifacts()
                .iter()
                .filter(|artifact| {
                    artifact.kind() != StandardLibraryArtifactKind::OptimizationArchive
                })
                .cloned(),
        );

        assert_eq!(
            without_optimization,
            Err(StandardLibraryManifestError::InvalidOptimizationMetadata(
                StandardLibraryOptimizationMetadataProblem::MissingBrayPartition,
            ))
        );

        let native_only = StandardLibraryTargetArtifacts::try_new(
            target.target().clone(),
            target.runtime_abi(),
            target
                .artifacts()
                .iter()
                .filter(|artifact| {
                    artifact.optimization().is_none_or(|optimization| {
                        optimization.producer().kind()
                            != StandardLibraryOptimizationProducerKind::Bray
                    })
                })
                .cloned(),
        );

        assert_eq!(
            native_only,
            Err(StandardLibraryManifestError::InvalidOptimizationMetadata(
                StandardLibraryOptimizationMetadataProblem::MissingBrayPartition,
            ))
        );
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

        let optimization_native_link = NativeLinkRequirement::new(
            NonEmptySharedStr::try_new("c")
                .unwrap_or_else(|| panic!("native library name must be valid")),
            NativeLinkKind::System,
        );

        let optimization_artifacts = target.artifacts().iter().cloned().map(|artifact| {
            if artifact.optimization().is_some_and(|optimization| {
                optimization.producer().kind()
                    == StandardLibraryOptimizationProducerKind::PinnedNative
            }) {
                artifact.with_native_links([optimization_native_link.clone()])
            } else {
                artifact
            }
        });

        assert!(
            StandardLibraryTargetArtifacts::try_new(
                target.target().clone(),
                target.runtime_abi(),
                optimization_artifacts,
            )
            .is_ok()
        );

        let bray_optimization_native_link = target.artifacts().iter().cloned().map(|artifact| {
            if artifact.optimization().is_some_and(|optimization| {
                optimization.producer().kind() == StandardLibraryOptimizationProducerKind::Bray
            }) {
                artifact.with_native_links([optimization_native_link.clone()])
            } else {
                artifact
            }
        });

        assert_eq!(
            StandardLibraryTargetArtifacts::try_new(
                target.target().clone(),
                target.runtime_abi(),
                bray_optimization_native_link,
            ),
            Err(StandardLibraryManifestError::InvalidNativeLink)
        );

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

        let dependency = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::DependencyMetadata,
            "targets/x86_64-unknown-linux-gnu/1.0/native-provider.json",
            b"dependency",
        )
        .unwrap_or_else(|error| panic!("dependency metadata must be valid: {error:?}"));

        let producer = StandardLibraryOptimizationProducer::try_new(
            StandardLibraryOptimizationProducerKind::Bray,
            "llvm",
            "1",
            "llvm",
            "22.1.8",
        )
        .unwrap_or_else(|error| panic!("optimization producer must be valid: {error:?}"));

        let compatibility = StandardLibraryOptimizationCompatibility::try_new(
            "x86_64-unknown-linux-gnu",
            "e-m:e-p:64:64",
            RelocationModel::PositionIndependent,
            CodeModel::Small,
            RuntimeAbiVersion::new(1, 0),
        )
        .unwrap_or_else(|error| panic!("optimization compatibility must be valid: {error:?}"));

        let fallback =
            StandardLibraryOptimizationFallback::try_new(archive.path(), archive.digest())
                .unwrap_or_else(|error| panic!("optimization fallback must be valid: {error:?}"));

        let optimization = StandardLibraryOptimizationMetadata::try_new(
            "std",
            producer,
            compatibility.clone(),
            fallback,
            NonZeroU32::MIN,
        )
        .unwrap_or_else(|error| panic!("optimization metadata must be valid: {error:?}"));

        let optimization = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::OptimizationArchive,
            "targets/x86_64-unknown-linux-gnu/1.0/libstd_optimization.a",
            b"optimization",
        )
        .map(|artifact| artifact.with_optimization(optimization))
        .unwrap_or_else(|error| panic!("optimization artifact must be valid: {error:?}"));

        let provider_producer = StandardLibraryOptimizationProducer::try_new(
            StandardLibraryOptimizationProducerKind::PinnedNative,
            "platform-core",
            "1",
            "llvm",
            "22.1.8",
        )
        .unwrap_or_else(|error| panic!("provider producer must be valid: {error:?}"));

        let provider_fallback =
            StandardLibraryOptimizationFallback::try_new(provider.path(), provider.digest())
                .unwrap_or_else(|error| panic!("provider fallback must be valid: {error:?}"));

        let provider_optimization = StandardLibraryOptimizationMetadata::try_new(
            "platform-core",
            provider_producer,
            compatibility,
            provider_fallback,
            NonZeroU32::MIN,
        )
        .map(|metadata| metadata.with_platform_services([PlatformServiceRole::ClockMonotonicNow]))
        .map(|metadata| {
            metadata.with_lifecycle_roots([
                StandardLibraryOptimizationLifecycleRoot::GlobalConstructors,
                StandardLibraryOptimizationLifecycleRoot::ExitRegistration,
            ])
        })
        .map(|metadata| {
            metadata.with_dependencies([StandardLibraryOptimizationDependency::try_new(
                dependency.path(),
                dependency.digest(),
            )
            .unwrap_or_else(|error| panic!("optimization dependency must be valid: {error:?}"))])
        })
        .unwrap_or_else(|error| panic!("provider optimization metadata must be valid: {error:?}"));

        let provider_optimization = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::OptimizationArchive,
            "targets/x86_64-unknown-linux-gnu/1.0/libplatform-core-optimization.a",
            b"provider optimization",
        )
        .map(|artifact| artifact.with_optimization(provider_optimization))
        .unwrap_or_else(|error| panic!("provider optimization must be valid: {error:?}"));

        let target = TargetIdentity::try_new("x86_64-unknown-linux-gnu")
            .unwrap_or_else(|| panic!("target identity must be valid"));

        let target = StandardLibraryTargetArtifacts::try_new(
            target,
            RuntimeAbiVersion::new(1, 0),
            [
                interface,
                implementation,
                archive,
                provider,
                dependency,
                optimization,
                provider_optimization,
            ],
        )
        .unwrap_or_else(|error| panic!("target artifacts must be valid: {error:?}"));

        StandardLibraryBundleManifest::try_new([target])
            .unwrap_or_else(|error| panic!("manifest must be valid: {error:?}"))
    }
}
