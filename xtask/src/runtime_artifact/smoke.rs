use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_runtime_interface::native_platform_service_role_symbol;
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

    command.args(["--edition", "2024", "--target", target.as_str()]);

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
        bray_runtime_abi::FOREIGN_CALLBACK_EXECUTION_SYMBOL,
        bray_runtime_abi::ROOT_EXECUTION_SYMBOL,
        bray_runtime_abi::TASK_ALLOCATION_SYMBOL,
        bray_runtime_abi::TASK_START_SYMBOL,
        bray_runtime_abi::WAKE_SYMBOL,
        bray_runtime_abi::TEST_ENTRY_SELECTION_SYMBOL,
        "bray_runtime_memory_allocation",
        "bray_runtime_string_scalar_count",
        "bray_runtime_character_scalar_value",
        "blake3",
    ];

    if let Some(symbol) = required
        .iter()
        .find(|symbol| !crate::link_map::contains_symbol(&contents, symbol))
    {
        return Err(CommandError::SynchronousLinkMapBoundary(format!(
            "missing {symbol}"
        )));
    }

    if let Some(symbol) = forbidden.iter().find(|symbol| {
        if **symbol == "blake3" {
            contents.contains(*symbol)
        } else {
            crate::link_map::contains_symbol(&contents, symbol)
        }
    }) {
        return Err(CommandError::SynchronousLinkMapBoundary(format!(
            "retained {symbol}"
        )));
    }

    Ok(())
}

fn audit_runtime_archives(package: &Package) -> Result<(), CommandError> {
    for component in &package.components {
        let symbols = defined_symbols(&component.archive)?;

        let (required, forbidden): (&[&str], &[&str]) = match component.kind {
            RuntimeArchiveKind::Common | RuntimeArchiveKind::TestCommon => (
                &[],
                &[
                    "bray_runtime_memory_",
                    "bray_runtime_string_",
                    "bray_runtime_character_",
                    "bray_runtime_root_",
                    "bray_runtime_task_",
                    "bray_platform_standard_",
                    bray_runtime_abi::TEST_ENTRY_SELECTION_SYMBOL,
                ],
            ),
            RuntimeArchiveKind::Observation => (
                &[
                    bray_runtime_abi::MEMORY_OBSERVATION_BEGIN_SYMBOL,
                    bray_runtime_abi::MEMORY_ALLOCATION_OBSERVATION_SYMBOL,
                    bray_runtime_abi::MEMORY_COPY_OBSERVATION_SYMBOL,
                    bray_runtime_abi::PERFORMANCE_INTERVAL_BEGIN_SYMBOL,
                    bray_runtime_abi::PERFORMANCE_INTERVAL_END_SYMBOL,
                ],
                &[
                    "bray_runtime_string_",
                    "bray_runtime_character_",
                    "bray_platform_standard_",
                ],
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
                    bray_runtime_abi::FOREIGN_CALLBACK_EXECUTION_SYMBOL,
                    bray_runtime_abi::TEST_ENTRY_SELECTION_SYMBOL,
                    "bray_platform_standard_",
                ],
            ),
            RuntimeArchiveKind::Callback => (
                &[bray_runtime_abi::FOREIGN_CALLBACK_EXECUTION_SYMBOL],
                &[
                    bray_runtime_abi::SYNCHRONOUS_ROOT_EXECUTION_SYMBOL,
                    bray_runtime_abi::ROOT_EXECUTION_SYMBOL,
                    bray_runtime_abi::TEST_ENTRY_SELECTION_SYMBOL,
                    "bray_runtime_memory_",
                    "bray_runtime_string_",
                    "bray_runtime_character_",
                    "bray_platform_standard_",
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
                    "bray_platform_standard_",
                ],
            ),
            RuntimeArchiveKind::Cancellation => (
                &[bray_runtime_abi::ROOT_CANCELLATION_REQUEST_SYMBOL],
                &[
                    bray_runtime_abi::ROOT_EXECUTION_SYMBOL,
                    bray_runtime_abi::TEST_ENTRY_SELECTION_SYMBOL,
                    "bray_platform_standard_",
                ],
            ),
            RuntimeArchiveKind::Event => (
                &[bray_runtime_abi::RUNTIME_EVENT_SYMBOL],
                &[
                    bray_runtime_abi::ROOT_EXECUTION_SYMBOL,
                    bray_runtime_abi::TEST_ENTRY_SELECTION_SYMBOL,
                    "bray_platform_standard_",
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

        let required = required
            .iter()
            .copied()
            .chain(
                component
                    .kind
                    .platform_services()
                    .iter()
                    .copied()
                    .map(native_platform_service_role_symbol),
            )
            .collect::<BTreeSet<_>>();

        let missing = required
            .iter()
            .filter(|symbol| !symbols.contains(**symbol))
            .map(|symbol| (*symbol).to_owned())
            .collect::<Vec<_>>();

        let forbidden = symbols
            .iter()
            .filter(|symbol| forbidden.iter().any(|prefix| symbol.starts_with(prefix)))
            .cloned()
            .collect::<Vec<_>>();

        let undeclared_platform_services = symbols
            .iter()
            .filter(|symbol| {
                symbol.starts_with("bray_platform_") && !required.contains(symbol.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();

        if !missing.is_empty() || !forbidden.is_empty() || !undeclared_platform_services.is_empty()
        {
            return Err(CommandError::RuntimeComponentBoundary {
                kind: component.kind,
                missing,
                forbidden,
                undeclared_platform_services,
            });
        }
    }

    Ok(())
}

fn defined_symbols(archive: &Path) -> Result<BTreeSet<String>, CommandError> {
    let tool =
        bray_tooling::llvm_tool_path(bray_diagnostics::DiagnosticLlvmToolRole::SymbolInspector)
            .map_err(CommandError::NativeSymbolToolUnavailable)?;

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
