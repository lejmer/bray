use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use bray_native_artifact::{
    NativeArtifactIndex, NativeCoRetentionGroup, NativeContentDigest, NativeUnit, NativeUnitKind,
    scan_object_archive_summary,
};
use bray_runtime_interface::RuntimeArtifactPurpose;
use bray_symbols::{NativeLinkKind, NativeLinkRequirement};
use bray_target::NativeTarget;
use sha2::{Digest, Sha256};

use super::command::{
    BuiltComponent, BuiltSupportComponent, CommandError, RuntimeArchiveKind, runtime_role_archive,
};

#[derive(Debug)]
pub(super) struct PublishedNativeIndex {
    pub(super) purpose: RuntimeArtifactPurpose,
    pub(super) file_name: String,
    pub(super) digest: NativeContentDigest,
}

pub(super) fn publish(
    target: NativeTarget,
    output: &Path,
    producer: NativeContentDigest,
    components: &[BuiltComponent],
    support: &[BuiltSupportComponent],
) -> Result<[PublishedNativeIndex; 2], CommandError> {
    let payloads = output.join("native");
    fs::create_dir_all(&payloads).map_err(|error| CommandError::write(&payloads, error))?;

    let published = RuntimeArtifactPurpose::ALL
        .into_iter()
        .map(|purpose| {
            let mut units = BTreeMap::new();
            let mut groups = BTreeSet::new();

            let components = components
                .iter()
                .filter(|component| {
                    component.kind != RuntimeArchiveKind::TestHost
                        || purpose == RuntimeArtifactPurpose::TestRunner
                })
                .filter(|component| {
                    purpose != RuntimeArtifactPurpose::TestRunner
                        || matches!(
                            component.kind,
                            RuntimeArchiveKind::Bootstrap
                                | RuntimeArchiveKind::Observation
                                | RuntimeArchiveKind::TestHost
                        )
                })
                .collect::<Vec<_>>();

            let bundled_libraries = components
                .iter()
                .flat_map(|component| {
                    // A combined component supplies every archive owner advertised by its roles.
                    component
                        .kind
                        .runtime_roles()
                        .filter_map(runtime_role_archive)
                        .chain([component.kind])
                })
                .map(RuntimeArchiveKind::archive_stem)
                .collect::<BTreeSet<_>>();

            for component in components {
                let native_links = super::native_link::runtime_native_link_requirements(
                    target,
                    &component.native_links,
                );

                if let Some(owner) = super::bootstrap::BrayRuntimeComponent::ALL
                    .into_iter()
                    .find(|owner| owner.archive_kind() == component.kind)
                {
                    collect_package(
                        owner.implementation_path(output, target).as_path(),
                        target,
                        &native_links,
                        &bundled_libraries,
                        &payloads,
                        &mut units,
                        &mut groups,
                    )?;
                } else {
                    collect_archive(
                        target,
                        &component.archive,
                        &native_links,
                        &payloads,
                        &mut units,
                    )?;
                }
            }

            for component in support.iter().filter(|component| {
                component.owners.contains(&RuntimeArchiveKind::TestHost)
                    == (purpose == RuntimeArtifactPurpose::TestRunner)
            }) {
                collect_archive(target, &component.archive, &[], &payloads, &mut units)?;
            }

            let index = NativeArtifactIndex::try_new(target, producer, units.into_values(), groups)
                .expect(
                    "runtime publication must preserve valid package unit and retention contracts",
                );

            let bytes = index
                .encode()
                .map_err(CommandError::RuntimePartitionIndex)?;

            let file_name = format!("runtime-{}-native-index.json", purpose.as_str());
            let path = output.join(&file_name);
            fs::write(&path, &bytes).map_err(|error| CommandError::write(&path, error))?;

            Ok(PublishedNativeIndex {
                purpose,
                file_name,
                digest: NativeContentDigest::new(Sha256::digest(&bytes).into()),
            })
        })
        .collect::<Result<Vec<_>, CommandError>>()?;

    Ok(published
        .try_into()
        .expect("one native index per runtime purpose"))
}

fn collect_package(
    path: &Path,
    target: NativeTarget,
    native_links: &[NativeLinkRequirement],
    bundled_libraries: &BTreeSet<&str>,
    payloads: &Path,
    units: &mut BTreeMap<NativeContentDigest, NativeUnit>,
    groups: &mut BTreeSet<NativeCoRetentionGroup>,
) -> Result<(), CommandError> {
    let bytes = fs::read(path).map_err(|error| CommandError::read(path, error))?;

    let artifact = bray_package_interface::PackageImplementationArtifact::try_from_bytes(
        bytes,
        bray_package_interface::InterfaceValidationLimits::default(),
    )
    .expect("runtime producer must publish a valid package implementation");

    let index = artifact
        .native_artifact()
        .expect("runtime producer native package must authenticate")
        .expect("runtime producer must publish native units");

    assert_eq!(
        index.target(),
        target,
        "runtime package must use its requested target"
    );

    // Retention groups share their immutable digest arrays with the original package.
    groups.extend(index.co_retention_groups().iter().cloned());

    for unit in index.units() {
        let bytes = artifact
            .native_unit_bytes(unit.digest().bytes())
            .expect("runtime producer native payload must authenticate")
            .expect("runtime native unit must have a published payload");

        // Immutable summaries share their underlying metadata with the validated producer.
        let contribution = NativeUnit::new(
            unit.digest(),
            unit.kind(),
            unit.summary().clone(),
            unit.native_links()
                .iter()
                .chain(native_links)
                .filter(|link| {
                    // This bundle supplies its component archives directly.
                    !matches!(link.kind(), NativeLinkKind::Static | NativeLinkKind::System)
                        || !bundled_libraries.contains(link.name())
                })
                .cloned(),
        )
        .with_statics(unit.statics().iter().cloned());

        insert_unit(target, payloads, units, contribution, &bytes)?;
    }

    Ok(())
}

fn collect_archive(
    target: NativeTarget,
    archive: &Path,
    native_links: &[NativeLinkRequirement],
    payloads: &Path,
    units: &mut BTreeMap<NativeContentDigest, NativeUnit>,
) -> Result<(), CommandError> {
    let bytes = fs::read(archive).map_err(|error| CommandError::read(archive, error))?;
    let digest = NativeContentDigest::new(Sha256::digest(&bytes).into());

    let unit = NativeUnit::new(
        digest,
        NativeUnitKind::OpaqueArchive,
        scan_object_archive_summary(&bytes).expect("runtime-produced foreign archive must parse"),
        native_links.iter().cloned(),
    );

    insert_unit(target, payloads, units, unit, &bytes)
}

fn insert_unit(
    target: NativeTarget,
    payloads: &Path,
    units: &mut BTreeMap<NativeContentDigest, NativeUnit>,
    unit: NativeUnit,
    bytes: &[u8],
) -> Result<(), CommandError> {
    let digest = unit.digest();
    let path = payloads.join(unit.kind().file_name(digest, target));

    fs::write(&path, bytes).map_err(|error| CommandError::write(&path, error))?;

    let links = units
        .get(&digest)
        .into_iter()
        .flat_map(|unit| unit.native_links())
        .chain(unit.native_links())
        .cloned();

    // Index metadata is immutable and shared across publication purposes.
    let contribution = NativeUnit::new(digest, unit.kind(), unit.summary().clone(), links)
        .with_statics(unit.statics().iter().cloned());

    units.insert(digest, contribution);

    Ok(())
}
