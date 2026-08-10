use std::fs;
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

    let product = package
        .components
        .iter()
        .find(|component| component.kind == RuntimeArchiveKind::ProductExecution)
        .ok_or(CommandError::MetadataContract)?;

    let executable = compile_smoke(
        SMOKE_SOURCE,
        "runtime-smoke",
        &product.archive,
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

    let executable = compile_smoke(
        SYNC_SMOKE_SOURCE,
        "runtime-sync-smoke",
        &product.archive,
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
    archive: &Path,
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

    let archive = archive.to_str().ok_or(CommandError::NonUtf8Path)?;
    let mut command = Command::new("rustc");

    command.args([
        "--edition",
        "2024",
        "--target",
        target.as_str(),
        "-C",
        &format!("link-arg={archive}"),
    ]);

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
    ];

    if required.iter().any(|symbol| !contents.contains(symbol))
        || forbidden.iter().any(|symbol| contents.contains(symbol))
    {
        return Err(CommandError::SynchronousLinkMapBoundary);
    }

    Ok(())
}

fn audit_runtime_archives(package: &Package) -> Result<(), CommandError> {
    for component in &package.components {
        let symbols = defined_symbols(&component.archive)?;

        let (required, forbidden): (&[&str], &[&str]) = match component.kind {
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
            RuntimeArchiveKind::ProductExecution => (
                &[],
                &[
                    "bray_runtime_memory_",
                    "bray_runtime_string_",
                    "bray_runtime_character_",
                    bray_runtime_abi::TEST_ENTRY_SELECTION_SYMBOL,
                ],
            ),
            RuntimeArchiveKind::TestExecution => (
                &[bray_runtime_abi::TEST_ENTRY_SELECTION_SYMBOL],
                &[
                    "bray_runtime_memory_",
                    "bray_runtime_string_",
                    "bray_runtime_character_",
                ],
            ),
        };

        if required.iter().any(|symbol| !symbols.contains(symbol))
            || forbidden.iter().any(|symbol| symbols.contains(symbol))
        {
            return Err(CommandError::RuntimeComponentBoundary(component.kind));
        }
    }

    Ok(())
}

fn defined_symbols(archive: &Path) -> Result<String, CommandError> {
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

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
