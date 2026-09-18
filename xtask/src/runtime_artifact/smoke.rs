use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_runtime_interface::RuntimeArtifactPurpose;
use bray_symbols::{NativeLinkKind, NativeLinkRequirement};
use bray_target::NativeTarget;

use super::command::{CommandError, Package, RuntimeArchiveKind};

const PANIC_REPORT_SOURCE: &str = include_str!("../../fixtures/runtime-panic-report.rs");
const OBSERVATION_SMOKE_SOURCE: &str = include_str!("../../fixtures/runtime-observation-smoke.c");
const SMOKE_SOURCE: &str = include_str!("../../fixtures/runtime-smoke.rs");
const SYNC_SMOKE_SOURCE: &str = include_str!("../../fixtures/runtime-sync-smoke.rs");
const BOOTSTRAP_SMOKE_SOURCE: &str = include_str!("../../fixtures/runtime-bootstrap-smoke.c");

pub(super) fn smoke_test(
    package: &Package,
    target: NativeTarget,
    directory: &Path,
) -> Result<(), CommandError> {
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
    smoke_test_observation(package, target, directory)?;

    Ok(())
}

fn smoke_test_observation(
    package: &Package,
    target: NativeTarget,
    directory: &Path,
) -> Result<(), CommandError> {
    let archives = component_archives(
        package,
        &[
            RuntimeArchiveKind::Observation,
            RuntimeArchiveKind::Bootstrap,
            RuntimeArchiveKind::Host,
            RuntimeArchiveKind::Callback,
            RuntimeArchiveKind::Cancellation,
            RuntimeArchiveKind::Common,
        ],
    )?;

    let metadata = crate::native_toolchain::runtime_artifact_metadata(&package.metadata)
        .map_err(CommandError::ObservationSmoke)?;

    let native_links = metadata
        .components()
        .iter()
        .find(|component| {
            component.purpose() == RuntimeArtifactPurpose::Product
                && component.identity().as_str().ends_with(".common")
        })
        .ok_or_else(|| {
            CommandError::ObservationSmoke(
                "runtime metadata has no product common component".to_owned(),
            )
        })?
        .native_links();

    let map = directory.join("runtime-observation-smoke.map");

    let executable = compile_c_smoke(
        OBSERVATION_SMOKE_SOURCE,
        "runtime-observation-smoke",
        &archives,
        target,
        directory,
        &map,
        native_links,
    )?;

    let empty = directory.join("runtime-observation-empty.bin");

    fs::write(&empty, b"stale observation")
        .map_err(|error| CommandError::write(&empty, error))?;

    require_observation_success(&executable, "empty", &empty)?;

    let empty_bytes = fs::read(&empty).map_err(|error| CommandError::read(&empty, error))?;

    if empty_bytes != bray_runtime_abi::PERFORMANCE_OBSERVATION_HEADER {
        return Err(CommandError::ObservationSmoke(
            "zero-event session did not truncate to the observation header".to_owned(),
        ));
    }

    let records = directory.join("runtime-observation-records.bin");
    require_observation_success(&executable, "records", &records)?;
    validate_observation_records(&records)?;

    let missing = Command::new(&executable)
        .arg("empty")
        .env_remove(bray_runtime_abi::PERFORMANCE_OBSERVATION_PATH_ENVIRONMENT)
        .output()
        .map_err(|error| CommandError::SmokeExecution {
            name: "observation missing path",
            error,
        })?;

    require_observation_failure(
        &missing,
        "performance observation output path is missing",
    )?;

    let invalid = directory.join("runtime-observation-invalid.bin");

    let invalid_interval = Command::new(&executable)
        .arg("invalid-interval")
        .env(
            bray_runtime_abi::PERFORMANCE_OBSERVATION_PATH_ENVIRONMENT,
            &invalid,
        )
        .output()
        .map_err(|error| CommandError::SmokeExecution {
            name: "observation invalid interval",
            error,
        })?;

    require_observation_failure(
        &invalid_interval,
        "performance observation interval is not active",
    )?;

    require_observation_mode_failure(
        &executable,
        "repeated-begin",
        &invalid,
        "performance observation interval is already active",
    )?;

    require_observation_mode_failure(
        &executable,
        "record-cap",
        &invalid,
        "performance observation record limit exceeded",
    )?;

    let record_cap_length = fs::metadata(&invalid)
        .map_err(|error| CommandError::read(&invalid, error))?
        .len();

    let expected_record_cap_length =
        bray_runtime_abi::PERFORMANCE_OBSERVATION_HEADER.len() as u64
            + bray_runtime_abi::MAX_PERFORMANCE_OBSERVATION_RECORDS * 9;

    if record_cap_length != expected_record_cap_length {
        return Err(CommandError::ObservationSmoke(format!(
            "record-cap session produced {record_cap_length} bytes instead of {expected_record_cap_length}"
        )));
    }

    audit_observation_link_map(&map)
}

fn require_observation_mode_failure(
    executable: &Path,
    mode: &str,
    output: &Path,
    expected: &str,
) -> Result<(), CommandError> {
    let execution = Command::new(executable)
        .arg(mode)
        .env(
            bray_runtime_abi::PERFORMANCE_OBSERVATION_PATH_ENVIRONMENT,
            output,
        )
        .output()
        .map_err(|error| CommandError::SmokeExecution {
            name: "observation invalid mode",
            error,
        })?;

    require_observation_failure(&execution, expected)
}

fn require_observation_success(
    executable: &Path,
    mode: &str,
    output: &Path,
) -> Result<(), CommandError> {
    let execution = Command::new(executable)
        .arg(mode)
        .env(
            bray_runtime_abi::PERFORMANCE_OBSERVATION_PATH_ENVIRONMENT,
            output,
        )
        .output()
        .map_err(|error| CommandError::SmokeExecution {
            name: "observation",
            error,
        })?;

    if !execution.status.success() {
        return Err(CommandError::ObservationSmoke(format!(
            "{mode} session failed: {}",
            String::from_utf8_lossy(&execution.stderr).trim()
        )));
    }

    if !execution.stdout.is_empty() || !execution.stderr.is_empty() {
        return Err(CommandError::ObservationSmoke(format!(
            "{mode} session changed program output"
        )));
    }

    Ok(())
}

fn require_observation_failure(
    execution: &std::process::Output,
    expected: &str,
) -> Result<(), CommandError> {
    let stderr = String::from_utf8_lossy(&execution.stderr);

    if execution.status.success() || !stderr.contains(expected) {
        return Err(CommandError::ObservationSmoke(format!(
            "expected fatal diagnostic {expected:?}, got status {} and stderr {:?}",
            execution.status,
            stderr.trim(),
        )));
    }

    Ok(())
}

fn validate_observation_records(path: &Path) -> Result<(), CommandError> {
    let bytes = fs::read(path).map_err(|error| CommandError::read(path, error))?;
    let header = bray_runtime_abi::PERFORMANCE_OBSERVATION_HEADER;

    let Some(records) = bytes.strip_prefix(&header) else {
        return Err(CommandError::ObservationSmoke(
            "recorded session omitted its header".to_owned(),
        ));
    };

    if records.len() != 27 {
        return Err(CommandError::ObservationSmoke(format!(
            "recorded session produced {} record bytes instead of 27",
            records.len(),
        )));
    }

    let values = [
        (records[0], u64::from_le_bytes(records[1..9].try_into().unwrap_or_else(|_| unreachable!()))),
        (records[9], u64::from_le_bytes(records[10..18].try_into().unwrap_or_else(|_| unreachable!()))),
        (records[18], u64::from_le_bytes(records[19..27].try_into().unwrap_or_else(|_| unreachable!()))),
    ];

    if values[0] != (1, 3) || values[1] != (2, 5) || values[2].0 != 3 {
        return Err(CommandError::ObservationSmoke(format!(
            "recorded session has unexpected records {values:?}"
        )));
    }

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

    let executable = compile_c_smoke(
        BOOTSTRAP_SMOKE_SOURCE,
        "runtime-bootstrap-smoke",
        &[archive],
        target,
        directory,
        &map,
        &[],
    )?;

    let status = Command::new(&executable)
        .status()
        .map_err(CommandError::BootstrapSmokeExecution)?;

    if !status.success() {
        return Err(CommandError::BootstrapSmokeExecutionFailed(status));
    }

    audit_bootstrap_link_map(&map)
}

fn compile_c_smoke(
    source_text: &str,
    name: &str,
    archives: &[&Path],
    target: NativeTarget,
    directory: &Path,
    map: &Path,
    native_links: &[NativeLinkRequirement],
) -> Result<PathBuf, CommandError> {
    let source = directory.join(format!("{name}.c"));

    let executable = directory.join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    });

    fs::write(&source, source_text).map_err(|error| CommandError::write(&source, error))?;

    let compiler =
        bray_tooling::llvm_tool_path(bray_diagnostics::DiagnosticLlvmToolRole::CompilerDriver)
            .map_err(CommandError::NativeCompilerToolUnavailable)?;

    let mut command = Command::new(compiler);

    command
        .arg(format!("--target={}", target.as_str()))
        .args(["-std=c11", "-O2", "-fuse-ld=lld"])
        .arg(&source)
        .args(archives);

    for link in native_links {
        if target.object_format() == bray_target::ObjectFormat::Coff
            && link.kind() == NativeLinkKind::System
            && link.name().eq_ignore_ascii_case("ucrt")
        {
            continue;
        }

        match link.kind() {
            NativeLinkKind::Framework => {
                command.arg("-framework").arg(link.name());
            }
            NativeLinkKind::Dynamic | NativeLinkKind::Static | NativeLinkKind::System => {
                command.arg(format!("-l{}", link.name()));
            }
        }
    }

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
        return Err(CommandError::NativeSmokeLinkFailed(name.to_owned()));
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

    let report = directory.join("panic_report.rs");
    fs::write(&report, PANIC_REPORT_SOURCE).map_err(|error| CommandError::write(&report, error))?;
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
    let required = [bray_runtime_abi::symbols::SYNCHRONOUS_ROOT_EXECUTION_SYMBOL];

    let forbidden = [
        bray_runtime_abi::symbols::ROOT_EXECUTION_SYMBOL,
        bray_runtime_abi::symbols::TASK_ALLOCATION_SYMBOL,
        bray_runtime_abi::symbols::TASK_START_SYMBOL,
        bray_runtime_abi::symbols::WAKE_SYMBOL,
        bray_runtime_abi::symbols::TEST_ENTRY_SELECTION_SYMBOL,
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
        bray_runtime_abi::symbols::RUNTIME_INITIALIZATION_SYMBOL,
        bray_runtime_abi::symbols::PANIC_REPORT_CONSTRUCTION_SYMBOL,
        bray_runtime_abi::symbols::PANIC_REPORTING_SYMBOL,
        bray_runtime_abi::symbols::STRUCTURED_SHUTDOWN_SYMBOL,
    ];

    let forbidden = [
        bray_runtime_abi::symbols::SYNCHRONOUS_ROOT_EXECUTION_SYMBOL,
        bray_runtime_abi::symbols::FOREIGN_CALLBACK_EXECUTION_SYMBOL,
        bray_runtime_abi::symbols::THREAD_ATTACHMENT_IDENTITY_SYMBOL,
        bray_runtime_abi::symbols::THREAD_STATIC_CLEANUP_REGISTRATION_SYMBOL,
        bray_runtime_abi::symbols::ROOT_EXECUTION_SYMBOL,
        bray_runtime_abi::symbols::TASK_ALLOCATION_SYMBOL,
        bray_runtime_abi::symbols::TASK_START_SYMBOL,
        bray_runtime_abi::symbols::WAKE_SYMBOL,
        bray_runtime_abi::symbols::TEST_ENTRY_SELECTION_SYMBOL,
        bray_runtime_abi::symbols::CURRENT_NATIVE_THREAD_IDENTITY_SYMBOL,
        bray_runtime_abi::symbols::MAIN_NATIVE_THREAD_IDENTITY_SYMBOL,
        bray_runtime_abi::symbols::AWAITED_FRAME_COMPOSITION_SYMBOL,
        bray_runtime_abi::symbols::FRAME_COMPLETION_MOVE_SYMBOL,
        bray_runtime_interface::PlatformServiceRole::ThreadStorageCreate.native_symbol(),
        bray_runtime_interface::PlatformServiceRole::ThreadStorageLoad.native_symbol(),
        bray_runtime_interface::PlatformServiceRole::ThreadStorageStore.native_symbol(),
        bray_runtime_interface::PlatformServiceRole::ThreadStorageDestroy.native_symbol(),
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

fn audit_observation_link_map(map: &Path) -> Result<(), CommandError> {
    let contents = fs::read_to_string(map).map_err(|error| CommandError::read(map, error))?;

    let required = [
        bray_runtime_abi::MEMORY_OBSERVATION_BEGIN_SYMBOL,
        bray_runtime_abi::MEMORY_ALLOCATION_OBSERVATION_SYMBOL,
        bray_runtime_abi::MEMORY_COPY_OBSERVATION_SYMBOL,
        bray_runtime_abi::PERFORMANCE_INTERVAL_BEGIN_SYMBOL,
        bray_runtime_abi::PERFORMANCE_INTERVAL_END_SYMBOL,
    ];

    audit_link_symbols(&contents, &required, &[]).map_err(CommandError::ObservationSmoke)?;

    if !contents.contains("bray_runtime_observation") {
        return Err(CommandError::ObservationSmoke(
            "link map does not attribute the hooks to the Observation archive".to_owned(),
        ));
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
