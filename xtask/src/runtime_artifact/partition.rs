use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_target::NativeTarget;
use sha2::{Digest, Sha256};

use super::command::{CommandError, RuntimeArchiveKind};

pub(super) struct RuntimeArchivePartitioner {
    tool: PathBuf,
    common: BTreeMap<[u8; 32], Vec<u8>>,
    components: BTreeMap<RuntimeArchiveKind, BTreeMap<[u8; 32], Vec<u8>>>,
}

impl RuntimeArchivePartitioner {
    pub(super) fn new(_root: &Path) -> Result<Self, CommandError> {
        let tool = bray_tooling::llvm_tool_path("llvm-ar")
            .ok_or(CommandError::RuntimePartitionToolUnavailable)?;

        Ok(Self {
            tool,
            common: BTreeMap::new(),
            components: BTreeMap::new(),
        })
    }

    pub(super) fn add(
        &mut self,
        kind: RuntimeArchiveKind,
        archive: &Path,
        crate_member_prefix: &str,
        isolate_unique_support: bool,
    ) -> Result<(), CommandError> {
        let members = extract_members(&self.tool, archive)?;
        let mut owned = BTreeMap::new();

        for member in members {
            let digest = Sha256::digest(&member.bytes).into();

            if member.name.starts_with(crate_member_prefix)
                || (isolate_unique_support && !self.common.contains_key(&digest))
            {
                owned.entry(digest).or_insert(member.bytes);
            } else {
                self.common.entry(digest).or_insert(member.bytes);
            }
        }

        if owned.is_empty() {
            return Err(CommandError::RuntimePartitionMissingOwner(kind));
        }

        self.components.insert(kind, owned);

        Ok(())
    }

    pub(super) fn write(self, target: NativeTarget, output: &Path) -> Result<(), CommandError> {
        write_archive(
            &self.tool,
            &output.join(super::command::archive_file_name(
                target,
                RuntimeArchiveKind::Common,
            )),
            self.common.values(),
        )?;

        for (kind, members) in self.components {
            write_archive(
                &self.tool,
                &output.join(super::command::archive_file_name(target, kind)),
                members.values(),
            )?;
        }

        Ok(())
    }
}

struct ArchiveMember {
    name: String,
    bytes: Vec<u8>,
}

fn extract_members(tool: &Path, archive: &Path) -> Result<Vec<ArchiveMember>, CommandError> {
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

fn write_archive<'member>(
    tool: &Path,
    archive: &Path,
    members: impl IntoIterator<Item = &'member Vec<u8>>,
) -> Result<(), CommandError> {
    let directory = tempfile::Builder::new()
        .prefix("bray-runtime-partition-")
        .tempdir()
        .map_err(CommandError::TemporaryDirectory)?;

    let mut names = Vec::new();

    for (index, bytes) in members.into_iter().enumerate() {
        let path = directory.path().join(format!("{index:04}.o"));
        fs::write(&path, bytes).map_err(|error| CommandError::write(&path, error))?;
        names.push(format!("{index:04}.o"));
    }

    if names.is_empty() {
        return Err(CommandError::RuntimePartitionFailed);
    }

    let status = Command::new(tool)
        .current_dir(directory.path())
        .arg("crsD")
        .arg(archive)
        .args(&names)
        .status()
        .map_err(CommandError::RuntimePartitionTool)?;

    if !status.success() {
        return Err(CommandError::RuntimePartitionFailed);
    }

    Ok(())
}
