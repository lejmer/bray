use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_base::StableDigestHasher;
use bray_native_artifact::{
    NativeArtifactIndex, NativeContentDigest, NativeUnit, NativeUnitKind, NativeUnitLocation,
    NativeUnitResolver,
};
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceProductKind, InterfaceSemantics,
    InterfaceValidationLimits, PackageImplementationArtifact, PackageImplementationConfiguration,
    PackageInterfaceExportBundle, PackageInterfaceIdentity, PackageInterfaceSurface,
    encode_package_interface,
};
use bray_symbols::{NativeLinkRequirement, PackageIdentity, PackageVersion};
use bray_target::NativeTarget;

/// Publishes a foreign producer as an ordinary native-only library dependency.
pub(crate) fn publish(
    name: &str,
    configuration: PackageImplementationConfiguration,
    toolchain: &str,
    archive: &[u8],
    bitcode: Option<&[u8]>,
    links: &[NativeLinkRequirement],
) -> Result<Vec<u8>, String> {
    let target = NativeTarget::for_identity(configuration.target())
        .ok_or("foreign library target is unsupported")?;

    let package = PackageIdentity::try_new(format!("native.{name}"))
        .ok_or("foreign package identity is invalid")?;

    let identity = PackageInterfaceIdentity::try_new(
        package,
        PackageVersion::try_new("0.0.0").expect("native package version must be valid"),
        InterfaceProductIdentity::try_new("library")
            .expect("native product identity must be valid"),
        InterfaceProductKind::Library,
        "native",
    )
    .expect("native public surface identity must be valid");

    let root_symbol = bray_symbols::ImportedSymbolIdentityInput::new(
        bray_symbols::InterfaceSymbolId::new(0),
        bray_symbols::ExternalSymbolKey::package(identity.package().clone()),
        bray_symbols::SymbolKind::Package,
        None,
    );

    let surface = PackageInterfaceSurface::try_new(identity, [], [root_symbol], [], [])
        .map_err(|error| format!("foreign package surface is invalid: {error:?}"))?;

    let mut producer = StableDigestHasher::new();

    "foreign native package".hash(&mut producer);
    name.hash(&mut producer);
    configuration.hash(&mut producer);
    toolchain.hash(&mut producer);
    links.hash(&mut producer);

    producer.write(archive);

    if let Some(modules) = bitcode {
        modules.hash(&mut producer);
    }

    let producer = NativeContentDigest::new(producer.finalize());

    let bundle = PackageInterfaceExportBundle::try_new(
        surface,
        InterfaceSemantics::new(),
        InterfaceLanguageRevision::new(0),
        configuration,
    )
    .map_err(|error| format!("foreign package interface is invalid: {error:?}"))?;

    let interface = encode_package_interface(&bundle)
        .map_err(|error| format!("foreign interface encoding failed: {error:?}"))?;

    let implementation = PackageImplementationArtifact::try_from_export_bundle(
        &interface,
        &bundle,
        InterfaceValidationLimits::default(),
    )
    .map_err(|error| format!("foreign implementation encoding failed: {error:?}"))?;

    let archive_digest = digest(archive);

    let object_unit = NativeUnit::new(
        archive_digest,
        NativeUnitKind::OpaqueArchive,
        bray_native_artifact::scan_object_archive_summary(archive)
            .expect("foreign producer native archive must parse"),
        links.iter().cloned(),
    );

    let mut payloads = vec![(archive_digest.bytes(), Arc::<[u8]>::from(archive))];
    let object = index(target, producer, None, vec![object_unit.clone()])?;
    let mut variants = vec![(NativeUnitKind::Object, object)];

    if let Some(archive) = bitcode {
        let archive_digest = digest(archive);

        // Foreign archives retain their lazy extraction boundary in every representation.
        let optimized = NativeUnit::new(
            archive_digest,
            NativeUnitKind::OpaqueArchive,
            object_unit.summary().clone(),
            links.iter().cloned(),
        );

        if archive_digest != object_unit.digest() {
            payloads.push((archive_digest.bytes(), Arc::from(archive)));
        }

        variants.push((
            NativeUnitKind::Bitcode,
            index(target, producer, Some(toolchain), vec![optimized])?,
        ));
    }

    let variants = variants
        .iter()
        .map(|(kind, bytes)| (*kind, bytes.as_slice()))
        .collect::<Vec<_>>();

    implementation
        .try_native_only_artifact(&variants, &payloads)
        .and_then(|artifact| {
            artifact
                .verify_all()
                .expect("foreign producer package must pass complete publication authentication");

            artifact.shared_bytes().map(|bytes| bytes.to_vec()).map_err(
                bray_package_interface::PackageImplementationArtifactBuildError::InvalidArtifact,
            )
        })
        .map_err(|error| format!("foreign native package {name} encoding failed: {error:?}"))
}

/// Loads the standard-library closure used by foreign hosts of Bray package archives.
pub(crate) fn standard_library_inputs(
    owned: &[PackageImplementationArtifact],
    target: NativeTarget,
    standard_library: &Path,
    output: &Path,
) -> Result<(Vec<(NativeUnitKind, PathBuf)>, Vec<NativeLinkRequirement>), String> {
    let selected = bray_compilation::SelectedTarget::for_native(target);

    let resolver = bray_standard_library::StandardLibraryResolver::new(
        bray_standard_library::StandardLibraryRoot::try_new(standard_library)
            .expect("foreign host must have an absolute standard-library root"),
    );

    let inventory = resolver
        .target_inventory(selected.profile().identity(), selected.runtime_abi())
        .map_err(|error| format!("foreign host dependency inventory could not load: {error:?}"))?;

    let mut dependencies = vec![
        inventory
            .package_implementation()
            .input(resolver.root().path())
            .load_implementation()
            .map_err(|error| format!("foreign host dependency could not load: {error:?}"))?,
    ];

    for record in inventory.native_dependencies() {
        dependencies.push(
            record
                .input(resolver.root().path())
                .load_implementation()
                .map_err(|error| {
                    format!("foreign host native dependency could not load: {error:?}")
                })?,
        );
    }

    dependency_inputs(owned, &dependencies, output)
}

/// Stages selected packed native dependencies for a foreign host linking owned package archives.
pub(crate) fn dependency_inputs(
    owned: &[PackageImplementationArtifact],
    dependencies: &[PackageImplementationArtifact],
    output: &Path,
) -> Result<(Vec<(NativeUnitKind, PathBuf)>, Vec<NativeLinkRequirement>), String> {
    let (artifacts, indexes): (Vec<_>, Vec<_>) = owned
        .iter()
        .chain(dependencies)
        .enumerate()
        .filter_map(
            |(ordinal, artifact)| match artifact.native_representation(None) {
                Ok(Some((_, index))) => Some(Ok((artifact, index))),
                Ok(None) => {
                    assert!(
                        ordinal >= owned.len(),
                        "foreign host owned packages must publish native objects"
                    );

                    None
                }
                Err(error) => Some(Err(format!(
                    "foreign host package index is invalid: {error:?}"
                ))),
            },
        )
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .unzip();

    let seeds = indexes
        .iter()
        .take(owned.len())
        .enumerate()
        .flat_map(|(artifact, index)| {
            index.units().iter().map(move |unit| NativeUnitLocation {
                artifact,
                digest: unit.digest(),
            })
        })
        .collect::<Vec<_>>();

    let resolver = NativeUnitResolver::new(indexes);

    let selection = resolver
        .select_seeded(seeds)
        .map_err(|error| format!("foreign host package dependencies do not resolve: {error:?}"))?;

    let mut links = BTreeSet::new();

    let inputs = selection
        .units()
        .iter()
        .map(|location| {
            let index = &resolver.artifacts()[location.artifact];

            let unit = index
                .units()
                .iter()
                .find(|unit| unit.digest() == location.digest)
                .expect("selected dependency must belong to its native index");

            links.extend(unit.native_links().iter().cloned());

            if location.artifact < owned.len() {
                return Ok(None);
            }

            assert_ne!(
                unit.kind(),
                NativeUnitKind::Bitcode,
                "foreign host requires linkable package inputs"
            );

            let bytes = artifacts[location.artifact]
                .native_unit_bytes(location.digest.bytes())
                .map_err(|error| format!("foreign host dependency payload is invalid: {error:?}"))?
                .expect("validated dependency index must have its selected payload");

            let path = output.join(unit.kind().file_name(location.digest, index.target()));

            std::fs::write(&path, &bytes).map_err(|error| {
                format!(
                    "could not stage foreign host dependency {}: {error}",
                    path.display()
                )
            })?;

            Ok(Some((unit.kind(), path)))
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok((
        inputs.into_iter().flatten().collect(),
        links.into_iter().collect(),
    ))
}

fn index(
    target: NativeTarget,
    producer: NativeContentDigest,
    toolchain: Option<&str>,
    units: Vec<NativeUnit>,
) -> Result<Vec<u8>, String> {
    NativeArtifactIndex::try_new(target, producer, units, [])
        .map(|index| match toolchain {
            Some(revision) => index.with_bitcode_toolchain(revision),
            None => index,
        })
        .and_then(|index| index.encode())
        .map_err(|error| format!("foreign native index is invalid: {error:?}"))
}

fn digest(bytes: &[u8]) -> NativeContentDigest {
    NativeContentDigest::new(bray_base::sha256_reader(bytes).expect("hashing memory cannot fail"))
}

#[cfg(test)]
mod tests {
    use super::publish;
    use bray_native_artifact::NativeUnitKind;
    use bray_package_interface::{InterfaceValidationLimits, PackageImplementationArtifact};

    #[test]
    fn foreign_archive_publishes_with_its_own_identity_and_packed_payload() {
        let bytes = publish(
            "provider",
            bray_package_interface::test_support::implementation_configuration(),
            "llvm-test-revision",
            b"!<arch>\n",
            Some(b"!<arch>\n"),
            &[],
        )
        .unwrap();

        let artifact = PackageImplementationArtifact::try_from_bytes(
            bytes,
            InterfaceValidationLimits::default(),
        )
        .unwrap();

        assert_eq!(
            artifact.identity().interface().package().as_str(),
            "native.provider"
        );

        let index = artifact
            .native_variant(NativeUnitKind::Object)
            .unwrap()
            .unwrap();

        assert_eq!(index.units().len(), 1);

        let payload = artifact
            .native_unit_bytes(index.units()[0].digest().bytes())
            .unwrap()
            .unwrap();

        assert_eq!(payload.as_ref(), b"!<arch>\n");

        let optimized = artifact
            .native_variant(NativeUnitKind::Bitcode)
            .unwrap()
            .unwrap();

        assert_eq!(optimized.units().len(), 1);
        assert_eq!(optimized.units()[0].kind(), NativeUnitKind::OpaqueArchive);
        assert_eq!(optimized.units()[0].digest(), index.units()[0].digest());
        assert_eq!(index.bitcode_toolchain(), None);
        assert_eq!(optimized.bitcode_toolchain(), Some("llvm-test-revision"));

        assert_eq!(
            artifact.native_representation(None).unwrap().unwrap().0,
            NativeUnitKind::Object
        );

        assert_eq!(
            artifact
                .native_representation(Some("llvm-test-revision"))
                .unwrap()
                .unwrap()
                .0,
            NativeUnitKind::Bitcode
        );

        assert_eq!(
            artifact
                .native_representation(Some("other-llvm"))
                .unwrap()
                .unwrap()
                .0,
            NativeUnitKind::Object
        );
    }
}
