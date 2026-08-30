use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_runtime_interface::native_platform_service_role_symbol;
use bray_target::NativeTarget;

use super::command::{CommandError, Package, RuntimeArchiveKind};

const SMOKE_SOURCE: &str = include_str!("../../fixtures/runtime-smoke.rs");
const SYNC_SMOKE_SOURCE: &str = include_str!("../../fixtures/runtime-sync-smoke.rs");
const BOOTSTRAP_SMOKE_SOURCE: &str = include_str!("../../fixtures/runtime-bootstrap-smoke.c");

pub(super) fn smoke_test(
    package: &Package,
    target: NativeTarget,
    directory: &Path,
) -> Result<(), CommandError> {
    audit_runtime_archives(package)?;

    let archives = component_archives(
        package,
        &[
            RuntimeArchiveKind::Bootstrap,
            RuntimeArchiveKind::Host,
            RuntimeArchiveKind::Callback,
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

    let output =
        Command::new(&executable)
            .output()
            .map_err(|error| CommandError::SmokeExecution {
                name: "asynchronous",
                error,
            })?;

    if !output.status.success() {
        return Err(CommandError::SmokeExecutionFailed {
            name: "asynchronous",
            status: output.status,
        });
    }

    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stderr.contains("cleanup_incident ordinal=0 ") {
        return Err(CommandError::CleanupReportMissing);
    }

    let map = directory.join("runtime-sync-smoke.map");

    let synchronous_archives = component_archives(
        package,
        &[
            RuntimeArchiveKind::Bootstrap,
            RuntimeArchiveKind::Host,
            RuntimeArchiveKind::Callback,
            RuntimeArchiveKind::Cancellation,
            RuntimeArchiveKind::Common,
        ],
    )?;

    let executable = compile_smoke(
        SYNC_SMOKE_SOURCE,
        "runtime-sync-smoke",
        &synchronous_archives,
        target,
        directory,
        Some(&map),
    )?;

    let status =
        Command::new(&executable)
            .status()
            .map_err(|error| CommandError::SmokeExecution {
                name: "synchronous",
                error,
            })?;

    if !status.success() {
        return Err(CommandError::SmokeExecutionFailed {
            name: "synchronous",
            status,
        });
    }

    audit_synchronous_link_map(&map)?;
    smoke_test_bootstrap(package, target, directory)?;

    Ok(())
}

fn smoke_test_bootstrap(
    package: &Package,
    target: NativeTarget,
    directory: &Path,
) -> Result<(), CommandError> {
    let archive = component_archives(package, &[RuntimeArchiveKind::Bootstrap])?
        .into_iter()
        .next()
        .ok_or(CommandError::MetadataContract)?;

    let map = directory.join("runtime-bootstrap-smoke.map");
    let executable = compile_bootstrap_smoke(archive, target, directory, &map)?;

    let status = Command::new(&executable)
        .status()
        .map_err(CommandError::BootstrapSmokeExecution)?;

    if !status.success() {
        return Err(CommandError::BootstrapSmokeExecutionFailed(status));
    }

    audit_bootstrap_link_map(&map)
}

fn compile_bootstrap_smoke(
    archive: &Path,
    target: NativeTarget,
    directory: &Path,
    map: &Path,
) -> Result<PathBuf, CommandError> {
    let source = directory.join("runtime-bootstrap-smoke.c");

    let executable = directory.join(if cfg!(windows) {
        "runtime-bootstrap-smoke.exe"
    } else {
        "runtime-bootstrap-smoke"
    });

    fs::write(&source, BOOTSTRAP_SMOKE_SOURCE)
        .map_err(|error| CommandError::write(&source, error))?;

    let compiler =
        bray_tooling::llvm_tool_path(bray_diagnostics::DiagnosticLlvmToolRole::CompilerDriver)
            .map_err(CommandError::NativeCompilerToolUnavailable)?;

    let mut command = Command::new(compiler);

    command
        .arg(format!("--target={}", target.as_str()))
        .args(["-std=c11", "-O2", "-fuse-ld=lld"])
        .arg(&source)
        .arg(archive);

    match target.object_format() {
        bray_target::ObjectFormat::Coff => {
            command
                .arg("-Xlinker")
                .arg(linker_map_argument(target, map)?);
        }
        bray_target::ObjectFormat::Elf | bray_target::ObjectFormat::MachO => {
            command
                .arg("-pthread")
                .arg(linker_map_argument(target, map)?);
        }
        bray_target::ObjectFormat::WebAssembly | bray_target::ObjectFormat::Xcoff => {
            return Err(CommandError::HostTarget);
        }
    }

    let status = command
        .arg("-o")
        .arg(&executable)
        .status()
        .map_err(CommandError::NativeCompiler)?;

    if !status.success() {
        return Err(CommandError::BootstrapSmokeLinkFailed);
    }

    Ok(executable)
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
        bray_runtime_abi::ROOT_EXECUTION_SYMBOL,
        bray_runtime_abi::TASK_ALLOCATION_SYMBOL,
        bray_runtime_abi::TASK_START_SYMBOL,
        bray_runtime_abi::WAKE_SYMBOL,
        bray_runtime_abi::TEST_ENTRY_SELECTION_SYMBOL,
        "bray_runtime_memory_allocation",
        "bray_runtime_string_scalar_count",
        "bray_runtime_character_scalar_value",
    ];

    audit_link_symbols(&contents, &required, &forbidden)
        .map_err(CommandError::SynchronousLinkMapBoundary)?;

    if contents.contains("blake3") {
        return Err(CommandError::SynchronousLinkMapBoundary(
            "retained blake3".to_owned(),
        ));
    }

    Ok(())
}

fn audit_bootstrap_link_map(map: &Path) -> Result<(), CommandError> {
    let contents = fs::read_to_string(map).map_err(|error| CommandError::read(map, error))?;

    let required = [
        bray_runtime_abi::RUNTIME_INITIALIZATION_SYMBOL,
        bray_runtime_abi::SYNCHRONOUS_ROOT_EXECUTION_SYMBOL,
        bray_runtime_abi::FOREIGN_CALLBACK_EXECUTION_SYMBOL,
        bray_runtime_abi::THREAD_ATTACHMENT_IDENTITY_SYMBOL,
        bray_runtime_abi::THREAD_STATIC_CLEANUP_REGISTRATION_SYMBOL,
        bray_runtime_abi::PANIC_REPORT_CONSTRUCTION_SYMBOL,
        bray_runtime_abi::PANIC_REPORTING_SYMBOL,
        bray_runtime_abi::STRUCTURED_SHUTDOWN_SYMBOL,
        bray_runtime_interface::native_platform_service_role_symbol(
            bray_runtime_interface::PlatformServiceRole::ThreadStorageCreate,
        ),
        bray_runtime_interface::native_platform_service_role_symbol(
            bray_runtime_interface::PlatformServiceRole::ThreadStorageLoad,
        ),
        bray_runtime_interface::native_platform_service_role_symbol(
            bray_runtime_interface::PlatformServiceRole::ThreadStorageStore,
        ),
        bray_runtime_interface::native_platform_service_role_symbol(
            bray_runtime_interface::PlatformServiceRole::ThreadStorageDestroy,
        ),
    ];

    let forbidden = [
        bray_runtime_abi::ROOT_EXECUTION_SYMBOL,
        bray_runtime_abi::TASK_ALLOCATION_SYMBOL,
        bray_runtime_abi::TASK_START_SYMBOL,
        bray_runtime_abi::WAKE_SYMBOL,
        bray_runtime_abi::TEST_ENTRY_SELECTION_SYMBOL,
        bray_runtime_abi::CURRENT_NATIVE_THREAD_IDENTITY_SYMBOL,
        bray_runtime_abi::MAIN_NATIVE_THREAD_IDENTITY_SYMBOL,
        bray_runtime_abi::AWAITED_FRAME_COMPOSITION_SYMBOL,
        bray_runtime_abi::FRAME_COMPLETION_MOVE_SYMBOL,
        "__rust_alloc",
        "__rust_dealloc",
        "rust_eh_personality",
    ];

    audit_link_symbols(&contents, &required, &forbidden)
        .map_err(CommandError::BootstrapLinkMapBoundary)?;

    for archive in [
        "bray_runtime_common",
        "bray_runtime_host",
        "bray_runtime_callback",
        "bray_runtime_scheduler",
        "bray_runtime_cancellation",
        "bray_runtime_event",
        "bray_runtime_test_host",
        "std.lib",
        "libstd.a",
    ] {
        if contents.contains(archive) {
            return Err(CommandError::BootstrapLinkMapBoundary(format!(
                "retained {archive}"
            )));
        }
    }

    Ok(())
}

fn audit_link_symbols(contents: &str, required: &[&str], forbidden: &[&str]) -> Result<(), String> {
    if let Some(symbol) = required
        .iter()
        .find(|symbol| !crate::link_map::contains_symbol(contents, symbol))
    {
        return Err(format!("missing {symbol}"));
    }

    if let Some(symbol) = forbidden
        .iter()
        .find(|symbol| crate::link_map::contains_symbol(contents, symbol))
    {
        return Err(format!("retained {symbol}"));
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
            RuntimeArchiveKind::Bootstrap => (
                &[
                    bray_runtime_abi::RUNTIME_INITIALIZATION_SYMBOL,
                    bray_runtime_abi::SYNCHRONOUS_ROOT_EXECUTION_SYMBOL,
                    bray_runtime_abi::FOREIGN_CALLBACK_EXECUTION_SYMBOL,
                    bray_runtime_abi::NATIVE_THREAD_EXECUTION_SYMBOL,
                    bray_runtime_abi::THREAD_ATTACHMENT_IDENTITY_SYMBOL,
                    bray_runtime_abi::THREAD_STATIC_CLEANUP_REGISTRATION_SYMBOL,
                    bray_runtime_abi::PANIC_REPORTING_SYMBOL,
                    bray_runtime_abi::STRUCTURED_SHUTDOWN_SYMBOL,
                    bray_runtime_abi::PANIC_REPORT_CONSTRUCTION_SYMBOL,
                ],
                &[
                    bray_runtime_abi::ROOT_EXECUTION_SYMBOL,
                    bray_runtime_abi::TASK_START_SYMBOL,
                    bray_runtime_abi::TEST_ENTRY_SELECTION_SYMBOL,
                    "__rust_",
                    "rust_",
                ],
            ),
            RuntimeArchiveKind::Host => (
                &[
                    "bray_runtime_substrate_initialization",
                    "bray_runtime_substrate_shutdown",
                ],
                &[
                    "bray_runtime_memory_",
                    "bray_runtime_string_",
                    "bray_runtime_character_",
                    bray_runtime_abi::ROOT_EXECUTION_SYMBOL,
                    bray_runtime_abi::TASK_START_SYMBOL,
                    bray_runtime_abi::SYNCHRONOUS_ROOT_EXECUTION_SYMBOL,
                    bray_runtime_abi::FOREIGN_CALLBACK_EXECUTION_SYMBOL,
                    bray_runtime_abi::STRUCTURED_SHUTDOWN_SYMBOL,
                    bray_runtime_abi::TEST_ENTRY_SELECTION_SYMBOL,
                    "bray_platform_standard_",
                ],
            ),
            RuntimeArchiveKind::Callback => (
                &[
                    "bray_runtime_substrate_panic_reporting",
                    "bray_runtime_substrate_synchronous_root_execution",
                    "bray_runtime_substrate_foreign_callback_execution",
                    "bray_runtime_substrate_native_thread_execution",
                ],
                &[
                    bray_runtime_abi::SYNCHRONOUS_ROOT_EXECUTION_SYMBOL,
                    bray_runtime_abi::FOREIGN_CALLBACK_EXECUTION_SYMBOL,
                    bray_runtime_abi::NATIVE_THREAD_EXECUTION_SYMBOL,
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
