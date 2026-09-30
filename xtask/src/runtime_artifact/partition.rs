use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_native_artifact::{
    NativeArtifactIndex, NativeCoRetentionGroup, NativeContentDigest, NativeUnit, NativeUnitKind,
    NativeUnitResolver, NativeUnitSummary, scan_object_unit_summary,
};
use bray_target::NativeTarget;
use sha2::{Digest, Sha256};

use super::command::{CommandError, RuntimeArchiveKind};

pub(super) struct RuntimeArchivePartitioner {
    tool: PathBuf,
    target: NativeTarget,
    support: BTreeMap<NativeContentDigest, SupportMember>,
    test_support: BTreeMap<NativeContentDigest, Vec<u8>>,
    components: BTreeMap<RuntimeArchiveKind, BTreeMap<NativeContentDigest, Vec<u8>>>,
}

pub(super) struct RuntimeSupportArchive {
    pub(super) owners: BTreeSet<RuntimeArchiveKind>,
    pub(super) archive: PathBuf,
}

struct SupportMember {
    bytes: Vec<u8>,
    owners: BTreeSet<RuntimeArchiveKind>,
}

impl RuntimeArchivePartitioner {
    pub(super) fn new(target: NativeTarget) -> Result<Self, CommandError> {
        let tool = bray_tooling::llvm_tool_path(bray_diagnostics::DiagnosticLlvmToolRole::Archiver)
            .map_err(CommandError::RuntimePartitionToolUnavailable)?;

        Ok(Self {
            tool,
            target,
            support: BTreeMap::new(),
            test_support: BTreeMap::new(),
            components: BTreeMap::new(),
        })
    }

    pub(super) fn add(
        &mut self,
        kind: RuntimeArchiveKind,
        archive: &Path,
        crate_member_prefix: &str,
        test_support: bool,
    ) -> Result<(), CommandError> {
        let members = extract_members(&self.tool, archive)?;
        let mut member_bytes = BTreeMap::new();
        let mut units = Vec::new();
        let mut owned = BTreeMap::new();

        for member in members {
            let digest = NativeContentDigest::new(Sha256::digest(&member.bytes).into());

            let name = Path::new(&member.name)
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or(CommandError::RuntimePartitionFailed)?
                .to_owned();

            let summary =
                scan_object_unit_summary(&member.bytes).unwrap_or(NativeUnitSummary::opaque([]));

            units.push(NativeUnit::new(digest, NativeUnitKind::Object, summary, []));
            member_bytes.insert(digest, member.bytes);

            if name.starts_with(crate_member_prefix) {
                owned.insert(digest, Vec::new());
            }
        }

        if owned.is_empty() {
            return Err(CommandError::RuntimePartitionMissingOwner(kind));
        }

        let producer = NativeContentDigest::new(
            bray_base::sha256_file(archive).map_err(|error| CommandError::read(archive, error))?,
        );

        // An opaque member may reference any sibling, including assembly that has an
        // exact summary. Keep that physical archive indivisible during partitioning.
        let groups = units
            .iter()
            .any(|unit| matches!(unit.summary(), NativeUnitSummary::Opaque { .. }))
            .then(|| NativeCoRetentionGroup::try_new(units.iter().map(NativeUnit::digest)))
            .flatten();

        let index = NativeArtifactIndex::try_new(self.target, producer, units, groups)
            .map_err(CommandError::RuntimePartitionIndex)?;

        let reachable =
            NativeUnitResolver::new([index])
                .select_seeded(owned.keys().map(|&digest| {
                    bray_native_artifact::NativeUnitLocation {
                        artifact: 0,
                        digest,
                    }
                }))
                .map_err(CommandError::RuntimePartitionResolution)?;

        for location in reachable.units() {
            let digest = location.digest;

            let bytes = member_bytes
                .remove(&digest)
                .ok_or(CommandError::RuntimePartitionFailed)?;

            if let Some(owner) = owned.get_mut(&digest) {
                *owner = bytes;
            } else if test_support {
                self.test_support.entry(digest).or_insert(bytes);
            } else {
                let support = self.support.entry(digest).or_insert_with(|| SupportMember {
                    bytes,
                    owners: BTreeSet::new(),
                });

                support.owners.insert(kind);
            }
        }

        self.components.insert(kind, owned);

        Ok(())
    }

    pub(super) fn write(
        self,
        target: NativeTarget,
        output: &Path,
    ) -> Result<Vec<RuntimeSupportArchive>, CommandError> {
        let mut grouped = BTreeMap::<BTreeSet<RuntimeArchiveKind>, Vec<Vec<u8>>>::new();

        for member in self.support.into_values() {
            grouped.entry(member.owners).or_default().push(member.bytes);
        }

        grouped.insert(
            BTreeSet::from([RuntimeArchiveKind::TestHost]),
            self.test_support.into_values().collect(),
        );

        let mut support = Vec::with_capacity(grouped.len());

        for (owners, members) in grouped {
            let name = support_name(&owners);
            let archive = output.join(support_archive_file_name(target, &name));

            let bytes = crate::native_archive::archive_bytes(&self.tool, &members, "o")
                .map_err(CommandError::RuntimePartitionTool)?;

            fs::write(&archive, bytes).map_err(|error| CommandError::write(&archive, error))?;

            support.push(RuntimeSupportArchive { owners, archive });
        }

        for (kind, members) in self.components {
            let archive = output.join(super::command::archive_file_name(target, kind));

            let bytes = crate::native_archive::archive_bytes(
                &self.tool,
                &members.into_values().collect::<Vec<_>>(),
                "o",
            )
            .map_err(CommandError::RuntimePartitionTool)?;

            fs::write(&archive, bytes).map_err(|error| CommandError::write(&archive, error))?;
        }

        Ok(support)
    }
}

fn support_name(owners: &BTreeSet<RuntimeArchiveKind>) -> String {
    owners
        .iter()
        .map(|owner| owner.component_name())
        .collect::<Vec<_>>()
        .join("_")
}

fn support_archive_file_name(target: NativeTarget, name: &str) -> String {
    bray_target::TargetOutputName::for_native(
        target.object_format(),
        bray_target::TargetOutputKind::StaticLibrary,
    )
    .file_name(&format!("bray_runtime_support_{name}"))
    .unwrap_or_else(|| panic!("runtime support identity must form a native archive name"))
}

pub(super) struct ArchiveMember {
    pub(super) name: String,
    pub(super) bytes: Vec<u8>,
}

pub(super) fn extract_members(
    tool: &Path,
    archive: &Path,
) -> Result<Vec<ArchiveMember>, CommandError> {
    let listing = Command::new(tool)
        .arg("t")
        .arg(archive)
        .output()
        .map_err(CommandError::RuntimePartitionTool)?;

    if !listing.status.success() {
        return Err(CommandError::RuntimePartitionFailed);
    }

    let names = String::from_utf8(listing.stdout)
        .map_err(|_| CommandError::RuntimePartitionFailed)?
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();

    let directory = tempfile::Builder::new()
        .prefix("bray-runtime-members-")
        .tempdir()
        .map_err(CommandError::TemporaryDirectory)?;

    let extraction = Command::new(tool)
        .current_dir(directory.path())
        .arg("x")
        .arg(archive)
        .output()
        .map_err(CommandError::RuntimePartitionTool)?;

    if !extraction.status.success() {
        return Err(CommandError::RuntimePartitionFailed);
    }

    let mut seen = BTreeSet::new();
    let mut members = Vec::with_capacity(names.len());

    for name in names {
        let file_name = Path::new(&name)
            .file_name()
            .ok_or(CommandError::RuntimePartitionFailed)?;

        if !seen.insert(file_name.to_owned()) {
            continue;
        }

        let path = directory.path().join(file_name);
        let bytes = fs::read(&path).map_err(|error| CommandError::read(&path, error))?;

        members.push(ArchiveMember { name, bytes });
    }

    Ok(members)
}
