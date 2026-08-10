use std::fs;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_target::NativeTarget;

use super::command::{CommandError, Package, RuntimeArchiveKind};

const SMOKE_SOURCE: &str = include_str!("../../fixtures/runtime-smoke.rs");
const SYNC_SMOKE_SOURCE: &str = include_str!("../../fixtures/runtime-sync-smoke.rs");

pub(super) fn smoke_test(
    package: &Package,
    target: NativeTarget,
    directory: &Path,
) -> Result<(), CommandError> {
    audit_runtime_archives(package)?;

    let archives = component_archives(
        package,
        &[
            RuntimeArchiveKind::Host,
            RuntimeArchiveKind::Scheduler,
            RuntimeArchiveKind::Cancellation,
            RuntimeArchiveKind::Event,
            RuntimeArchiveKind::Common,
        ],
    )?;

    let executable = compile_smoke(
        SMOKE_SOURCE,
        "runtime-smoke",
        &archives,
        target,
        directory,
        None,
    )?;

    let output = Command::new(&executable)
        .output()
        .map_err(CommandError::SmokeExecution)?;

    if !output.status.success() {
        return Err(CommandError::SmokeExecutionFailed);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stderr.contains("cleanup_incident ordinal=0 ") {
        return Err(CommandError::CleanupReportMissing);
    }

    let map = directory.join("runtime-sync-smoke.map");

    let synchronous_archives = component_archives(
        package,
        &[RuntimeArchiveKind::Host, RuntimeArchiveKind::Common],
    )?;

    let executable = compile_smoke(
        SYNC_SMOKE_SOURCE,
        "runtime-sync-smoke",
        &synchronous_archives,
        target,
        directory,
        Some(&map),
    )?;

    let status = Command::new(&executable)
        .status()
        .map_err(CommandError::SmokeExecution)?;

    if !status.success() {
        return Err(CommandError::SmokeExecutionFailed);
    }

    audit_synchronous_link_map(&map)?;

    Ok(())
}

fn compile_smoke(
    source_text: &str,
    name: &str,
    archives: &[&Path],
    target: NativeTarget,
    directory: &Path,
    map: Option<&Path>,
) -> Result<PathBuf, CommandError> {
    let source = directory.join(format!("{name}.rs"));

    let executable = directory.join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    });

    fs::write(&source, source_text).map_err(|error| CommandError::write(&source, error))?;

    let mut command = Command::new("rustc");

    command.args([
        "--edition",
        "2024",
        "--target",
        target.as_str(),
    ]);

    for archive in archives {
        let archive = archive.to_str().ok_or(CommandError::NonUtf8Path)?;
        command.arg("-C").arg(format!("link-arg={archive}"));
    }

    if let Some(map) = map {
        command
            .arg("-C")
            .arg(format!("link-arg={}", linker_map_argument(target, map)?));
    }

    let status = command
        .arg("-o")
        .arg(&executable)
        .arg(&source)
        .status()
        .map_err(CommandError::Rustc)?;

    if !status.success() {
        return Err(CommandError::SmokeLinkFailed);
    }

    Ok(executable)
}

fn component_archives<'package>(
    package: &'package Package,
    kinds: &[RuntimeArchiveKind],
) -> Result<Vec<&'package Path>, CommandError> {
    kinds
        .iter()
        .map(|kind| {
            package
                .components
                .iter()
                .find(|component| component.kind == *kind)
                .map(|component| component.archive.as_path())
                .ok_or(CommandError::MetadataContract)
        })
        .collect()
}

fn linker_map_argument(target: NativeTarget, map: &Path) -> Result<String, CommandError> {
    let map = map.to_str().ok_or(CommandError::NonUtf8Path)?;

    Ok(match target.object_format() {
        bray_target::ObjectFormat::Coff => format!("/MAP:{map}"),
        bray_target::ObjectFormat::Elf => format!("-Wl,-Map={map}"),
        bray_target::ObjectFormat::MachO => format!("-Wl,-map,{map}"),
        bray_target::ObjectFormat::WebAssembly | bray_target::ObjectFormat::Xcoff => {
            return Err(CommandError::HostTarget);
        }
    })
}

fn audit_synchronous_link_map(map: &Path) -> Result<(), CommandError> {
    let contents = fs::read_to_string(map).map_err(|error| CommandError::read(map, error))?;
    let required = [bray_runtime_abi::SYNCHRONOUS_ROOT_EXECUTION_SYMBOL];

    let forbidden = [
        bray_runtime_abi::ROOT_EXECUTION_SYMBOL,
        bray_runtime_abi::TASK_ALLOCATION_SYMBOL,
        bray_runtime_abi::TASK_START_SYMBOL,
        bray_runtime_abi::WAKE_SYMBOL,
        bray_runtime_abi::TEST_ENTRY_SELECTION_SYMBOL,
        bray_runtime_abi::MEMORY_ALLOCATION_SYMBOL,
        bray_runtime_abi::STRING_SCALAR_COUNT_SYMBOL,
        bray_runtime_abi::CHARACTER_SCALAR_VALUE_SYMBOL,
        "blake3",
    ];

    if let Some(symbol) = required
        .iter()
        .find(|symbol| !contains_link_symbol(&contents, symbol))
    {
        return Err(CommandError::SynchronousLinkMapBoundary(format!(
            "missing {symbol}"
        )));
    }

    if let Some(symbol) = forbidden.iter().find(|symbol| {
        if **symbol == "blake3" {
            contents.contains(*symbol)
        } else {
            contains_link_symbol(&contents, symbol)
        }
    }) {
        return Err(CommandError::SynchronousLinkMapBoundary(format!(
            "retained {symbol}"
        )));
    }

    Ok(())
}

fn contains_link_symbol(contents: &str, symbol: &str) -> bool {
    contents
        .split_whitespace()
        .any(|token| token == symbol || token.strip_prefix('_') == Some(symbol))
}

fn audit_runtime_archives(package: &Package) -> Result<(), CommandError> {
    for component in &package.components {
        let symbols = defined_symbols(&component.archive)?;

        let (required, forbidden): (&[&str], &[&str]) = match component.kind {
            RuntimeArchiveKind::Common => (
                &[],
                &[
                    "bray_runtime_memory_",
                    "bray_runtime_string_",
                    "bray_runtime_character_",
                    "bray_runtime_root_",
                    "bray_runtime_task_",
                    bray_runtime_abi::TEST_ENTRY_SELECTION_SYMBOL,
                ],
            ),
            RuntimeArchiveKind::Memory => (
                &[
                    bray_runtime_abi::MEMORY_ALLOCATION_SYMBOL,
                    bray_runtime_abi::MEMORY_DEALLOCATION_SYMBOL,
                ],
                &["bray_runtime_string_", "bray_runtime_character_"],
            ),
            RuntimeArchiveKind::String => (
                &[
                    bray_runtime_abi::STRING_SCALAR_COUNT_SYMBOL,
                    bray_runtime_abi::STRING_EQUALS_SYMBOL,
                    bray_runtime_abi::STRING_SCALAR_AT_SYMBOL,
                    bray_runtime_abi::STRING_SCALAR_SLICE_SYMBOL,
                    bray_runtime_abi::STRING_FROM_UTF8_SYMBOL,
                ],
                &["bray_runtime_memory_", "bray_runtime_character_"],
            ),
            RuntimeArchiveKind::Character => (
                &[
                    bray_runtime_abi::CHARACTER_SCALAR_VALUE_SYMBOL,
                    bray_runtime_abi::CHARACTER_FROM_SCALAR_VALUE_SYMBOL,
                    bray_runtime_abi::CHARACTER_UTF8_LENGTH_SYMBOL,
                    bray_runtime_abi::CHARACTER_UTF8_BYTE_SYMBOL,
                    bray_runtime_abi::CHARACTER_IS_ALPHABETIC_SYMBOL,
                    bray_runtime_abi::CHARACTER_IS_NUMERIC_SYMBOL,
                    bray_runtime_abi::CHARACTER_IS_WHITESPACE_SYMBOL,
                ],
                &["bray_runtime_memory_", "bray_runtime_string_"],
            ),
            RuntimeArchiveKind::Host => (
                &[
                    bray_runtime_abi::SYNCHRONOUS_ROOT_EXECUTION_SYMBOL,
                    bray_runtime_abi::STRUCTURED_SHUTDOWN_SYMBOL,
                ],
                &[
                    "bray_runtime_memory_",
                    "bray_runtime_string_",
                    "bray_runtime_character_",
                    bray_runtime_abi::ROOT_EXECUTION_SYMBOL,
                    bray_runtime_abi::TASK_START_SYMBOL,
                    bray_runtime_abi::TEST_ENTRY_SELECTION_SYMBOL,
                ],
            ),
            RuntimeArchiveKind::Scheduler => (
                &[
                    bray_runtime_abi::ROOT_EXECUTION_SYMBOL,
                    bray_runtime_abi::TASK_START_SYMBOL,
                ],
                &[
                    bray_runtime_abi::SYNCHRONOUS_ROOT_EXECUTION_SYMBOL,
                    bray_runtime_abi::TEST_ENTRY_SELECTION_SYMBOL,
                    "bray_runtime_memory_",
                    "bray_runtime_string_",
                    "bray_runtime_character_",
                ],
            ),
            RuntimeArchiveKind::Cancellation => (
                &[bray_runtime_abi::ROOT_CANCELLATION_REQUEST_SYMBOL],
                &[
                    bray_runtime_abi::ROOT_EXECUTION_SYMBOL,
                    bray_runtime_abi::TEST_ENTRY_SELECTION_SYMBOL,
                ],
            ),
            RuntimeArchiveKind::Event => (
                &[bray_runtime_abi::RUNTIME_EVENT_SYMBOL],
                &[
                    bray_runtime_abi::ROOT_EXECUTION_SYMBOL,
                    bray_runtime_abi::TEST_ENTRY_SELECTION_SYMBOL,
                ],
            ),
            RuntimeArchiveKind::TestHost => (
                &[bray_runtime_abi::TEST_ENTRY_SELECTION_SYMBOL],
                &[
                    "bray_runtime_memory_",
                    "bray_runtime_string_",
                    "bray_runtime_character_",
                ],
            ),
        };

        if required.iter().any(|symbol| !symbols.contains(*symbol))
            || forbidden
                .iter()
                .any(|prefix| symbols.iter().any(|symbol| symbol.starts_with(prefix)))
        {
            return Err(CommandError::RuntimeComponentBoundary(component.kind));
        }
    }

    Ok(())
}

fn defined_symbols(archive: &Path) -> Result<BTreeSet<String>, CommandError> {
    let tool =
        bray_tooling::llvm_tool_path("llvm-nm").ok_or(CommandError::NativeSymbolToolUnavailable)?;

    let output = Command::new(tool)
        .args(["--defined-only", "--extern-only"])
        .arg(archive)
        .output()
        .map_err(CommandError::NativeSymbolInspection)?;

    if !output.status.success() {
        return Err(CommandError::NativeSymbolInspectionFailed);
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_whitespace().last())
        .map(str::to_owned)
        .collect())
}
