use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_target::NativeTarget;
use sha2::{Digest, Sha256};

use super::command::{CommandError, RuntimeArchiveKind};

pub(super) struct RuntimeArchivePartitioner {
    tool: PathBuf,
    symbol_tool: PathBuf,
    support: BTreeMap<[u8; 32], SupportMember>,
    test_support: BTreeMap<[u8; 32], Vec<u8>>,
    components: BTreeMap<RuntimeArchiveKind, BTreeMap<[u8; 32], Vec<u8>>>,
}

pub(super) struct RuntimeSupportArchive {
    pub(super) owners: BTreeSet<RuntimeArchiveKind>,
    pub(super) name: String,
    pub(super) archive: PathBuf,
}

struct SupportMember {
    bytes: Vec<u8>,
    owners: BTreeSet<RuntimeArchiveKind>,
}

impl RuntimeArchivePartitioner {
    pub(super) fn new(_root: &Path) -> Result<Self, CommandError> {
        let tool = bray_tooling::llvm_tool_path(bray_diagnostics::DiagnosticLlvmToolRole::Archiver)
            .map_err(CommandError::RuntimePartitionToolUnavailable)?;

        let symbol_tool = bray_tooling::llvm_tool_path(
            bray_diagnostics::DiagnosticLlvmToolRole::SymbolInspector,
        )
        .map_err(CommandError::RuntimePartitionToolUnavailable)?;

        Ok(Self {
            tool,
            symbol_tool,
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
        let symbols = archive_member_symbols(&self.symbol_tool, archive)?;
        let mut member_bytes = BTreeMap::new();
        let mut member_digests = BTreeMap::new();
        let mut owned = BTreeMap::new();

        for member in members {
            let digest = Sha256::digest(&member.bytes).into();

            let name = Path::new(&member.name)
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or(CommandError::RuntimePartitionFailed)?
                .to_owned();

            member_digests.insert(name.clone(), digest);
            member_bytes.insert(digest, member.bytes);

            if name.starts_with(crate_member_prefix) {
                owned.insert(digest, Vec::new());
            }
        }

        if owned.is_empty() {
            return Err(CommandError::RuntimePartitionMissingOwner(kind));
        }

        let reachable = reachable_members(&symbols, &member_digests, owned.keys().copied());

        for digest in reachable {
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

            write_archive(&self.tool, &archive, &members)?;

            support.push(RuntimeSupportArchive {
                owners,
                name,
                archive,
            });
        }

        for (kind, members) in self.components {
            write_archive(
                &self.tool,
                &output.join(super::command::archive_file_name(target, kind)),
                &members.into_values().collect::<Vec<_>>(),
            )?;
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

struct ArchiveMember {
    name: String,
    bytes: Vec<u8>,
}

#[derive(Default)]
struct MemberSymbols {
    definitions: BTreeSet<String>,
    references: BTreeSet<String>,
}

fn archive_member_symbols(
    tool: &Path,
    archive: &Path,
) -> Result<BTreeMap<String, MemberSymbols>, CommandError> {
    let output = Command::new(tool)
        .args(["--extern-only", "--print-file-name"])
        .arg(archive)
        .output()
        .map_err(CommandError::RuntimePartitionTool)?;

    if !output.status.success() {
        return Err(CommandError::RuntimePartitionFailed);
    }

    let output = String::from_utf8(output.stdout)
        .map_err(|_| CommandError::RuntimePartitionFailed)?;

    let mut members = BTreeMap::<String, MemberSymbols>::new();

    for line in output.lines() {
        let Some((source, record)) = line.rsplit_once(": ") else {
            continue;
        };

        let Some(member) = archive_member_name(source, archive) else {
            continue;
        };

        let fields = record.split_whitespace().collect::<Vec<_>>();

        let Some(symbol) = fields.last() else {
            continue;
        };

        let kind = fields.get(fields.len().saturating_sub(2)).copied();
        let entry = members.entry(member.to_owned()).or_default();

        if matches!(kind, Some("U" | "u")) {
            entry.references.insert((*symbol).to_owned());
        } else {
            entry.definitions.insert((*symbol).to_owned());
        }
    }

    Ok(members)
}

fn archive_member_name<'a>(source: &'a str, archive: &Path) -> Option<&'a str> {
    let member = source
        .strip_prefix(archive.to_str()?)?
        .strip_prefix(':')?;

    Path::new(member).file_name()?.to_str()
}

fn reachable_members(
    symbols: &BTreeMap<String, MemberSymbols>,
    member_digests: &BTreeMap<String, [u8; 32]>,
    roots: impl IntoIterator<Item = [u8; 32]>,
) -> BTreeSet<[u8; 32]> {
    let definitions = symbols
        .iter()
        .flat_map(|(member, symbols)| {
            symbols
                .definitions
                .iter()
                .filter_map(move |symbol| member_digests.get(member).map(|digest| (symbol, *digest)))
        })
        .collect::<BTreeMap<_, _>>();

    let references = symbols
        .iter()
        .filter_map(|(member, symbols)| {
            member_digests
                .get(member)
                .map(|digest| (*digest, &symbols.references))
        })
        .collect::<BTreeMap<_, _>>();

    let mut reachable = BTreeSet::new();
    let mut pending = roots.into_iter().collect::<Vec<_>>();

    while let Some(member) = pending.pop() {
        if !reachable.insert(member) {
            continue;
        }

        if let Some(references) = references.get(&member) {
            pending.extend(
                references
                    .iter()
                    .filter_map(|symbol| definitions.get(symbol).copied()),
            );
        }
    }

    reachable
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

fn write_archive(
    tool: &Path,
    archive: &Path,
    members: &[Vec<u8>],
) -> Result<(), CommandError> {
    let directory = tempfile::Builder::new()
        .prefix("bray-runtime-partition-")
        .tempdir()
        .map_err(CommandError::TemporaryDirectory)?;

    let mut names = Vec::new();

    for (index, bytes) in members.iter().enumerate() {
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

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    use super::{MemberSymbols, archive_member_name, reachable_members};

    #[test]
    fn archive_member_names_ignore_member_directory_and_drive_separators() {
        assert_eq!(
            archive_member_name("runtime.lib:objects/blake3.o", Path::new("runtime.lib")),
            Some("blake3.o")
        );

        #[cfg(windows)]
        assert_eq!(
            archive_member_name(
                r"runtime.lib:C:\build\objects\blake3.o",
                Path::new("runtime.lib"),
            ),
            Some("blake3.o")
        );
    }

    #[test]
    fn member_reachability_keeps_only_transitive_support() {
        let digest = |value| [value; 32];

        let symbols = BTreeMap::from([
            (
                "owner.o".to_owned(),
                MemberSymbols {
                    definitions: BTreeSet::from(["owner".to_owned()]),
                    references: BTreeSet::from(["first".to_owned()]),
                },
            ),
            (
                "first.o".to_owned(),
                MemberSymbols {
                    definitions: BTreeSet::from(["first".to_owned()]),
                    references: BTreeSet::from(["second".to_owned()]),
                },
            ),
            (
                "second.o".to_owned(),
                MemberSymbols {
                    definitions: BTreeSet::from(["second".to_owned()]),
                    references: BTreeSet::new(),
                },
            ),
            (
                "unrelated.o".to_owned(),
                MemberSymbols {
                    definitions: BTreeSet::from(["unrelated".to_owned()]),
                    references: BTreeSet::new(),
                },
            ),
        ]);

        let digests = BTreeMap::from([
            ("owner.o".to_owned(), digest(1)),
            ("first.o".to_owned(), digest(2)),
            ("second.o".to_owned(), digest(3)),
            ("unrelated.o".to_owned(), digest(4)),
        ]);

        assert_eq!(
            reachable_members(&symbols, &digests, [digest(1)]),
            BTreeSet::from([digest(1), digest(2), digest(3)])
        );
    }
}
