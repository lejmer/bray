use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use bray_native_artifact::{
    NativeArtifactIndex, NativeContentDigest, NativeUnit, NativeUnitKind, NativeUnitSummary,
};
use bray_package_interface::{InterfaceValidationLimits, PackageImplementationArtifact};
use bray_standard_library::{StandardLibraryArtifact, StandardLibraryArtifactKind};
use bray_target::NativeTarget;

use super::command::{BuildError, BuiltPlatformArchive, write_bundle_artifact};
use super::optimization::BuiltNativeModules;

pub(super) fn publish(
    root: &Path,
    bundle: &Path,
    target_path: &str,
    target: NativeTarget,
    implementation_bytes: &[u8],
    fallback: &StandardLibraryArtifact,
    compiler_support: &StandardLibraryArtifact,
    built: &BuiltNativeModules,
    platforms: &[BuiltPlatformArchive],
) -> Result<[StandardLibraryArtifact; 2], BuildError> {
    let implementation = PackageImplementationArtifact::try_from_bytes(
        Arc::<[u8]>::from(implementation_bytes),
        InterfaceValidationLimits::default(),
    ).map_err(|error| BuildError::NativeArchive(format!(
        "standard library implementation is invalid: {error:?}",
    )))?;

    let published = implementation.native_artifact()
        .map_err(|error| BuildError::NativeArchive(format!(
            "standard library native publication is invalid: {error:?}",
        )))?
        .expect("standard library native implementation must have an index");

    if published.target() != target {
        return Err(BuildError::NativeArchive(
            "standard library native publication has another target".to_owned(),
        ));
    }

    let mut units = Vec::with_capacity(built.units.len() + 2);
    let mut opaque = Vec::new();

    for (summary, bytes) in &built.units {
        if matches!(summary, NativeUnitSummary::Opaque) {
            opaque.push(bytes.clone());
            continue;
        }

        let digest = content_digest(bytes);

        let unit = NativeUnit::new(
            digest,
            NativeUnitKind::Bitcode,
            summary.clone(),
            [],
            [],
        );

        write_unit(bundle, target_path, target, &unit, bytes)?;
        units.push(unit);
    }

    let mut archives = Vec::new();

    if !opaque.is_empty() {
        let bytes = opaque_bitcode_archive(root, &opaque)?;
        archives.push(archive_unit(bundle, target_path, target, &bytes, &[])?);
    }

    for artifact in [fallback, compiler_support] {
        let path = artifact.beneath(bundle);

        let bytes = std::fs::read(&path)
            .map_err(|error| BuildError::read(&path, error))?;

        archives.push(archive_unit(bundle, target_path, target, &bytes, artifact.native_links())?);
    }

    for platform in platforms {
        if let Some(optimization) = &platform.optimization {
            if optimization.triple != built.triple
                || optimization.data_layout != built.data_layout
            {
                return Err(BuildError::NativeArchive(format!(
                    "platform provider {} has an incompatible bitcode target contract",
                    platform.name,
                )));
            }

            let mut opaque = Vec::new();

            for (summary, bytes) in &optimization.units {
                if matches!(summary, NativeUnitSummary::Opaque) {
                    opaque.push(bytes.clone());
                    continue;
                }

                let unit = NativeUnit::new(
                    content_digest(bytes),
                    NativeUnitKind::Bitcode,
                    summary.clone(),
                    platform.native_links.iter().cloned(),
                    [],
                );

                write_unit(bundle, target_path, target, &unit, bytes)?;
                units.push(unit);
            }

            if !opaque.is_empty() {
                let bytes = opaque_bitcode_archive(root, &opaque)?;
                archives.push(archive_unit(bundle, target_path, target, &bytes, &platform.native_links)?);
            }
        }

        archives.push(archive_unit(bundle, target_path, target, &platform.bytes, &platform.native_links)?);
    }

    units.extend(archives.iter().cloned());
    let bitcode = write_index(bundle, target_path, target, published.producer(), units, "native-index.json", StandardLibraryArtifactKind::NativeIndex)?;

    let grouped = published.co_retention_groups().iter()
        .flat_map(|group| group.members().iter().copied())
        .collect::<BTreeSet<_>>();

    let mut objects = Vec::new();

    for unit in published.units() {
        if matches!(unit.summary(), NativeUnitSummary::Opaque) || grouped.contains(&unit.digest()) {
            continue;
        }

        let bytes = implementation.native_unit_bytes(unit.digest().bytes())
            .map_err(|error| BuildError::NativeArchive(format!(
                "standard library object unit is invalid: {error:?}",
            )))?
            .expect("validated standard library native unit must have payload bytes");

        write_unit(bundle, target_path, target, unit, &bytes)?;
        objects.push(unit.clone());
    }

    objects.extend(archives);
    let object = write_index(bundle, target_path, target, published.producer(), objects, "native-object-index.json", StandardLibraryArtifactKind::NativeObjectIndex)?;

    Ok([bitcode, object])
}

fn opaque_bitcode_archive(root: &Path, modules: &[Vec<u8>]) -> Result<Vec<u8>, BuildError> {
    let tool = bray_llvm_toolchain::tool_path(root, "llvm-ar");

    crate::native_archive::archive_bytes(&tool, modules, "bc")
        .map_err(|error| BuildError::NativeArchive(format!(
            "opaque optimization modules cannot be archived: {error}",
        )))
}

fn write_index(
    bundle: &Path,
    target_path: &str,
    target: NativeTarget,
    producer: NativeContentDigest,
    units: Vec<NativeUnit>,
    name: &str,
    kind: StandardLibraryArtifactKind,
) -> Result<StandardLibraryArtifact, BuildError> {
    let index = NativeArtifactIndex::try_new(target, producer, units, [])
        .map_err(|error| BuildError::NativeArchive(format!(
            "standard library native index is invalid: {error:?}",
        )))?;

    let bytes = index.encode().map_err(|error| BuildError::NativeArchive(format!(
        "standard library native index cannot encode: {error:?}",
    )))?;

    let path = format!("{target_path}/{name}");
    write_bundle_artifact(bundle, &path, &bytes)?;

    StandardLibraryArtifact::try_for_bytes(
        kind,
        path,
        &bytes,
    ).map_err(|error| BuildError::Manifest(format!("{error:?}")))
}

fn archive_unit(
    bundle: &Path,
    target_path: &str,
    target: NativeTarget,
    bytes: &[u8],
    native_links: &[bray_symbols::NativeLinkRequirement],
) -> Result<NativeUnit, BuildError> {
    let unit = NativeUnit::new(
        content_digest(bytes),
        NativeUnitKind::OpaqueArchive,
        NativeUnitSummary::Opaque,
        native_links.iter().cloned(),
        [],
    );

    write_unit(bundle, target_path, target, &unit, bytes)?;

    Ok(unit)
}

fn write_unit(
    bundle: &Path,
    target_path: &str,
    target: NativeTarget,
    unit: &NativeUnit,
    bytes: &[u8],
) -> Result<(), BuildError> {
    let path = format!(
        "{target_path}/native/{}",
        unit.kind().file_name(unit.digest(), target),
    );

    write_bundle_artifact(bundle, &path, bytes)
}

fn content_digest(bytes: &[u8]) -> NativeContentDigest {
    NativeContentDigest::new(
        bray_base::sha256_reader(bytes)
            .expect("hashing an in-memory native unit cannot fail"),
    )
}
