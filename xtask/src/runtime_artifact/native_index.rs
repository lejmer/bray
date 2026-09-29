use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use bray_native_artifact::{
    NativeArtifactIndex, NativeContentDigest, NativeUnit, NativeUnitKind, NativeUnitSummary,
    scan_object_unit_summary,
};
use bray_runtime_interface::RuntimeArtifactPurpose;
use bray_symbols::NativeLinkRequirement;
use bray_target::NativeTarget;
use sha2::{Digest, Sha256};

use super::command::{BuiltComponent, BuiltSupportComponent, CommandError, RuntimeArchiveKind};
use super::partition::extract_members;

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
    let tool = bray_tooling::llvm_tool_path(bray_diagnostics::DiagnosticLlvmToolRole::Archiver)
        .map_err(CommandError::RuntimePartitionToolUnavailable)?;

    let payloads = output.join("native");
    fs::create_dir_all(&payloads).map_err(|error| CommandError::write(&payloads, error))?;

    let published = RuntimeArtifactPurpose::ALL.into_iter().map(|purpose| {
        let mut units = BTreeMap::new();
        let mut lazy_exact = Vec::new();

        for component in components.iter().filter(|component| {
            component.kind != RuntimeArchiveKind::TestHost
                || purpose == RuntimeArtifactPurpose::TestRunner
        }).filter(|component| {
            purpose != RuntimeArtifactPurpose::TestRunner
                || matches!(component.kind,
                    RuntimeArchiveKind::Bootstrap
                    | RuntimeArchiveKind::Observation
                    | RuntimeArchiveKind::TestHost)
        }) {
            let native_links = super::native_link::runtime_native_link_requirements(
                target, &component.native_links,
            );

            collect_archive(
                &tool, target, &component.archive, &native_links,
                &payloads, &mut units, &mut lazy_exact,
            )?;
        }

        for component in support.iter().filter(|component| {
            component.owners.contains(&RuntimeArchiveKind::TestHost)
                == (purpose == RuntimeArtifactPurpose::TestRunner)
        }) {
            collect_archive(
                &tool, target, &component.archive, &[],
                &payloads, &mut units, &mut lazy_exact,
            )?;
        }

        if !lazy_exact.is_empty() {
            // Opaque Rust members can refer to exact Bray objects. A lazy archive
            // lets the linker pull those objects without forcing every one directly.
            let links = units.values()
                .filter(|unit| matches!(unit.summary(), NativeUnitSummary::Exact { .. }))
                .flat_map(NativeUnit::native_links)
                .cloned()
                .collect::<BTreeSet<_>>();

            let bytes = crate::native_archive::archive_bytes(&tool, &lazy_exact, "o")
                .map_err(CommandError::RuntimePartitionTool)?;

            insert_unit(target, &payloads, &mut units, NativeUnitKind::OpaqueArchive,
                NativeUnitSummary::Opaque, &links.into_iter().collect::<Vec<_>>(), &bytes)?;
        }

        let index = NativeArtifactIndex::try_new(
            target, producer,
            units.into_values(), [],
        ).map_err(CommandError::RuntimePartitionIndex)?;

        let bytes = index.encode().map_err(CommandError::RuntimePartitionIndex)?;
        let file_name = format!("runtime-{}-native-index.json", purpose.as_str());
        let path = output.join(&file_name);
        fs::write(&path, &bytes).map_err(|error| CommandError::write(&path, error))?;

        Ok(PublishedNativeIndex {
            purpose,
            file_name,
            digest: NativeContentDigest::new(Sha256::digest(&bytes).into()),
        })
    }).collect::<Result<Vec<_>, CommandError>>()?;

    Ok(published.try_into().expect("one native index per runtime purpose"))
}

fn collect_archive(
    tool: &Path,
    target: NativeTarget,
    archive: &Path,
    native_links: &[NativeLinkRequirement],
    payloads: &Path,
    units: &mut BTreeMap<NativeContentDigest, NativeUnit>,
    lazy_exact: &mut Vec<Vec<u8>>,
) -> Result<(), CommandError> {
    let mut opaque = Vec::new();

    for member in extract_members(tool, archive)? {
        match scan_object_unit_summary(&member.bytes) {
            Ok(summary @ NativeUnitSummary::Exact { .. }) => {
                insert_unit(target, payloads, units, NativeUnitKind::Object,
                    summary, native_links, &member.bytes)?;

                lazy_exact.push(member.bytes);
            }
            Ok(NativeUnitSummary::Opaque) | Err(_) => opaque.push(member.bytes),
        }
    }

    if !opaque.is_empty() {
        let bytes = crate::native_archive::archive_bytes(tool, &opaque, "o")
            .map_err(CommandError::RuntimePartitionTool)?;

        insert_unit(target, payloads, units, NativeUnitKind::OpaqueArchive,
            NativeUnitSummary::Opaque, native_links, &bytes)?;
    }

    Ok(())
}

fn insert_unit(
    target: NativeTarget,
    payloads: &Path,
    units: &mut BTreeMap<NativeContentDigest, NativeUnit>,
    kind: NativeUnitKind,
    summary: NativeUnitSummary,
    native_links: &[NativeLinkRequirement],
    bytes: &[u8],
) -> Result<(), CommandError> {
    let digest = NativeContentDigest::new(Sha256::digest(bytes).into());
    let path = payloads.join(kind.file_name(digest, target));
    fs::write(&path, bytes).map_err(|error| CommandError::write(&path, error))?;

    let links = units.get(&digest)
        .into_iter()
        .flat_map(|unit| unit.native_links())
        .chain(native_links)
        .cloned();

    units.insert(digest, NativeUnit::new(digest, kind, summary, links, []));

    Ok(())
}
