use std::fs;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use bray_compilation::{
    BuildConfiguration, Compilation, CompilationOptions, CompilationRequest, SelectedTarget,
    WorkerBudget,
};
use bray_emitter::{ArtifactKind, resolve_published_artifact};
use bray_runtime_interface::RuntimeArtifactPurpose;
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_standard_library::StandardLibraryRoot;
use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
use bray_target::NativeTarget;
use bray_tooling::load_llvm_compilation;

use super::command::{CommandError, Package, RuntimeArchiveKind};
use super::smoke::{audit_link_symbols, compile_c_smoke, component_archives};

const C_SMOKE_SOURCE: &str = include_str!("../../fixtures/runtime-observation-smoke.c");
const PRODUCT_SOURCE: &str = include_str!("../../fixtures/runtime-observation-product.bray");
const EXECUTION_TIMEOUT: Duration = Duration::from_secs(30);
const CONCURRENT_THREAD_COUNT: usize = 32;
const CONCURRENT_RECORDS_PER_THREAD: usize = 256;

const OBSERVATION_SYMBOLS: [&str; 5] = [
    bray_runtime_abi::MEMORY_OBSERVATION_BEGIN_SYMBOL,
    bray_runtime_abi::MEMORY_ALLOCATION_OBSERVATION_SYMBOL,
    bray_runtime_abi::MEMORY_COPY_OBSERVATION_SYMBOL,
    bray_runtime_abi::PERFORMANCE_INTERVAL_BEGIN_SYMBOL,
    bray_runtime_abi::PERFORMANCE_INTERVAL_END_SYMBOL,
];

pub(super) fn smoke_test(
    package: &Package,
    target: NativeTarget,
    directory: &Path,
) -> Result<(), CommandError> {
    smoke_test_direct_hooks(package, target, directory)?;

    smoke_test_generated_products(package, target, directory)
}

fn smoke_test_direct_hooks(
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
        C_SMOKE_SOURCE,
        "runtime-observation-smoke",
        &archives,
        target,
        directory,
        &map,
        native_links,
    )?;

    let empty = directory.join("runtime-observation-empty.bin");

    fs::write(&empty, b"stale observation").map_err(|error| CommandError::write(&empty, error))?;

    require_mode_success(&executable, Some("empty"), &empty)?;

    let empty_bytes = fs::read(&empty).map_err(|error| CommandError::read(&empty, error))?;

    if empty_bytes != bray_runtime_abi::PERFORMANCE_OBSERVATION_HEADER {
        return Err(CommandError::ObservationSmoke(
            "zero-event session did not truncate to the observation header".to_owned(),
        ));
    }

    let records = directory.join("runtime-observation-records.bin");

    require_mode_success(&executable, Some("records"), &records)?;

    let values = read_records(&records)?;

    if values.len() != 3 || values[0] != (1, 3) || values[1] != (2, 5) || values[2].0 != 3 {
        return Err(CommandError::ObservationSmoke(format!(
            "direct-hook session has unexpected records {values:?}"
        )));
    }

    let concurrent = directory.join("runtime-observation-concurrent.bin");

    require_mode_success(&executable, Some("concurrent-first-hooks"), &concurrent)?;

    let concurrent_values = read_records(&concurrent)?;
    let expected_concurrent_records = CONCURRENT_THREAD_COUNT * CONCURRENT_RECORDS_PER_THREAD;

    if concurrent_values.len() != expected_concurrent_records {
        return Err(CommandError::ObservationSmoke(format!(
            "concurrent first-hook session produced {} records instead of {expected_concurrent_records}",
            concurrent_values.len()
        )));
    }

    let mut concurrent_counts = [0; CONCURRENT_THREAD_COUNT];

    for (kind, value) in concurrent_values {
        let Some(index) = value
            .checked_sub(1)
            .and_then(|value| usize::try_from(value).ok())
        else {
            return Err(CommandError::ObservationSmoke(format!(
                "concurrent first-hook session recorded invalid value {value}"
            )));
        };

        if kind != 1 || index >= CONCURRENT_THREAD_COUNT {
            return Err(CommandError::ObservationSmoke(format!(
                "concurrent first-hook session recorded invalid kind/value ({kind}, {value})"
            )));
        }

        concurrent_counts[index] += 1;
    }

    if concurrent_counts
        .iter()
        .any(|count| *count != CONCURRENT_RECORDS_PER_THREAD)
    {
        return Err(CommandError::ObservationSmoke(format!(
            "concurrent first-hook session has unexpected per-thread counts {concurrent_counts:?}"
        )));
    }

    let missing = command_output(
        Command::new(&executable)
            .arg("empty")
            .env_remove(bray_runtime_abi::PERFORMANCE_OBSERVATION_PATH_ENVIRONMENT),
        "observation missing path",
    )?;

    require_failure(&missing, "performance observation output path is missing")?;

    let invalid = directory.join("runtime-observation-invalid.bin");

    require_mode_failure(
        &executable,
        "invalid-interval",
        &invalid,
        "performance observation interval is not active",
    )?;

    require_mode_failure(
        &executable,
        "repeated-begin",
        &invalid,
        "performance observation interval is already active",
    )?;

    require_mode_failure(
        &executable,
        "repeated-interval",
        &invalid,
        "performance observation interval is already completed",
    )?;

    require_mode_failure(
        &executable,
        "record-cap",
        &invalid,
        "performance observation record limit exceeded",
    )?;

    let record_cap_length = fs::metadata(&invalid)
        .map_err(|error| CommandError::read(&invalid, error))?
        .len();

    let expected_record_cap_length = bray_runtime_abi::PERFORMANCE_OBSERVATION_HEADER.len() as u64
        + bray_runtime_abi::MAX_PERFORMANCE_OBSERVATION_RECORDS * 9;

    if record_cap_length != expected_record_cap_length {
        return Err(CommandError::ObservationSmoke(format!(
            "record-cap session produced {record_cap_length} bytes instead of {expected_record_cap_length}"
        )));
    }

    audit_observation_link_map(&map, &OBSERVATION_SYMBOLS)
}

fn smoke_test_generated_products(
    package: &Package,
    target: NativeTarget,
    directory: &Path,
) -> Result<(), CommandError> {
    let (compilation, product) = observation_product(target)?;

    let (ordinary, ordinary_map) =
        crate::progress::run("Emitting an ordinary Bray smoke product", || {
            emit_product(
                &compilation,
                product.clone(),
                package,
                target,
                directory,
                "ordinary",
                BuildConfiguration::Release,
            )
        })?;

    let ordinary_execution = command_output(
        Command::new(&ordinary)
            .env_remove(bray_runtime_abi::PERFORMANCE_OBSERVATION_PATH_ENVIRONMENT),
        "ordinary generated observation product",
    )?;

    require_clean_success("ordinary generated product", &ordinary_execution)?;
    audit_unobserved_link_map(&ordinary_map)?;

    let (memory, memory_map) =
        crate::progress::run("Emitting a memory-observed Bray smoke product", || {
            emit_product(
                &compilation,
                product.clone(),
                package,
                target,
                directory,
                "memory",
                BuildConfiguration::ObservedRelease,
            )
        })?;

    let memory_output = directory.join("runtime-observation-generated-memory.bin");

    require_mode_success(&memory, None, &memory_output)?;

    let memory_records = read_records(&memory_output)?;

    let expected_memory_records = [
        (1, 4),
        (2, 0),
        (1, 8),
        (2, 4),
        (1, 16),
        (2, 8),
        (1, 32),
        (2, 16),
        (1, 64),
        (2, 32),
    ];

    if memory_records != expected_memory_records {
        return Err(CommandError::ObservationSmoke(format!(
            "generated memory product has unexpected records {memory_records:?}"
        )));
    }

    audit_observation_link_map(&memory_map, &OBSERVATION_SYMBOLS[..3])?;

    let (timed, timed_map) = crate::progress::run("Emitting a timed Bray smoke product", || {
        emit_product(
            &compilation,
            product,
            package,
            target,
            directory,
            "timed",
            BuildConfiguration::TimedRelease {
                inner_iterations: NonZeroU64::MIN,
            },
        )
    })?;

    let timed_output = directory.join("runtime-observation-generated-timed.bin");

    require_mode_success(&timed, None, &timed_output)?;

    let timed_records = read_records(&timed_output)?;

    if timed_records.len() != 1 || timed_records[0].0 != 3 {
        return Err(CommandError::ObservationSmoke(format!(
            "generated timed product has unexpected records {timed_records:?}"
        )));
    }

    audit_observation_link_map(&timed_map, &OBSERVATION_SYMBOLS[3..])
}

fn observation_product(
    target: NativeTarget,
) -> Result<(Compilation, ProductIdentity), CommandError> {
    let package = PackageIdentity::try_new("bray.runtime.observation.smoke")
        .unwrap_or_else(|| unreachable!("fixture package identity is valid"));

    let product = ProductIdentity::try_new(package.clone(), "application")
        .unwrap_or_else(|| unreachable!("fixture product identity is valid"));

    let source = SourceInput::virtual_text(
        SourceIdentity::new(0),
        "runtime-observation-product.bray",
        SourceVersion::new(0),
        PRODUCT_SOURCE,
    );

    let selected = SelectedTarget::for_native(target);

    let request = CompilationRequest::with_options(
        package,
        vec![source],
        CompilationOptions::new(WorkerBudget::default(), ProductKind::Executable, selected),
    )
    .with_standard_library_root(observation_standard_library(target)?);

    let compilation = load_llvm_compilation(request).map_err(|error| {
        CommandError::ObservationSmoke(format!(
            "could not load generated observation product: {error:?}"
        ))
    })?;

    let diagnostics = compilation.check_diagnostics();

    if !diagnostics.is_empty() {
        return Err(CommandError::ObservationSmoke(format!(
            "generated observation product did not pass compilation checks: {diagnostics:?}"
        )));
    }

    Ok((compilation, product))
}

fn observation_standard_library(target: NativeTarget) -> Result<StandardLibraryRoot, CommandError> {
    let root = crate::workspace::root().map_err(CommandError::Workspace)?;

    let path = crate::workspace::cargo_target(&root)
        .join("runtime-bootstrap-standard-library")
        .join(target.as_str());

    StandardLibraryRoot::try_new(path).ok_or_else(|| {
        CommandError::ObservationSmoke(
            "runtime observation standard-library bundle path is invalid".to_owned(),
        )
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "the fixture emission needs the exact compilation and runtime inputs"
)]
fn emit_product(
    compilation: &Compilation,
    product: ProductIdentity,
    package: &Package,
    target: NativeTarget,
    directory: &Path,
    mode: &str,
    configuration: BuildConfiguration,
) -> Result<(PathBuf, PathBuf), CommandError> {
    let output = directory.join(format!("runtime-observation-product-{mode}"));
    let map = directory.join(format!("runtime-observation-product-{mode}.map"));

    let map_output = bray_linker::SystemLinkerMapOutput::try_new(map.clone()).ok_or_else(|| {
        CommandError::ObservationSmoke(format!(
            "generated observation linker-map path is invalid: {}",
            map.display()
        ))
    })?;

    crate::native_product::emit_executable_with_configuration(
        compilation,
        product.clone(),
        target,
        &package.metadata,
        &output,
        [],
        Some(map_output),
        configuration,
    )
    .map_err(CommandError::ObservationSmoke)?;

    let destination = crate::native_product::managed_destination(&output)
        .map_err(CommandError::ObservationSmoke)?;

    let executable = resolve_published_artifact(destination, &product, ArtifactKind::Executable, 0)
        .map_err(|error| {
            CommandError::ObservationSmoke(format!(
                "could not resolve generated observation product: {error:?}"
            ))
        })?;

    Ok((executable.path().to_path_buf(), map))
}

fn require_mode_failure(
    executable: &Path,
    mode: &str,
    output: &Path,
    expected: &str,
) -> Result<(), CommandError> {
    let execution = command_output(
        Command::new(executable).arg(mode).env(
            bray_runtime_abi::PERFORMANCE_OBSERVATION_PATH_ENVIRONMENT,
            output,
        ),
        "observation invalid mode",
    )?;

    require_failure(&execution, expected)
}

fn require_mode_success(
    executable: &Path,
    mode: Option<&str>,
    output: &Path,
) -> Result<(), CommandError> {
    let mut command = Command::new(executable);

    command.env(
        bray_runtime_abi::PERFORMANCE_OBSERVATION_PATH_ENVIRONMENT,
        output,
    );

    if let Some(mode) = mode {
        command.arg(mode);
    }

    let execution = command_output(&mut command, "observation")?;

    require_clean_success(mode.unwrap_or("generated product"), &execution)
}

fn command_output(command: &mut Command, name: &'static str) -> Result<Output, CommandError> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| CommandError::SmokeExecution { name, error })?;

    let deadline = Instant::now() + EXECUTION_TIMEOUT;

    loop {
        if child
            .try_wait()
            .map_err(|error| CommandError::SmokeExecution { name, error })?
            .is_some()
        {
            return child
                .wait_with_output()
                .map_err(|error| CommandError::SmokeExecution { name, error });
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();

            return Err(CommandError::ObservationSmoke(format!(
                "{name} exceeded the {} second execution limit",
                EXECUTION_TIMEOUT.as_secs()
            )));
        }

        std::thread::sleep(Duration::from_millis(10));
    }
}

fn require_clean_success(name: &str, execution: &Output) -> Result<(), CommandError> {
    if !execution.status.success() {
        return Err(CommandError::ObservationSmoke(format!(
            "{name} session failed: {}",
            String::from_utf8_lossy(&execution.stderr).trim()
        )));
    }

    if !execution.stdout.is_empty() || !execution.stderr.is_empty() {
        return Err(CommandError::ObservationSmoke(format!(
            "{name} session changed program output"
        )));
    }

    Ok(())
}

fn require_failure(execution: &Output, expected: &str) -> Result<(), CommandError> {
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

fn read_records(path: &Path) -> Result<Vec<(u8, u64)>, CommandError> {
    let bytes = fs::read(path).map_err(|error| CommandError::read(path, error))?;
    let header = bray_runtime_abi::PERFORMANCE_OBSERVATION_HEADER;

    let Some(records) = bytes.strip_prefix(&header) else {
        return Err(CommandError::ObservationSmoke(
            "recorded session omitted its header".to_owned(),
        ));
    };

    if records.len() % 9 != 0 {
        return Err(CommandError::ObservationSmoke(format!(
            "recorded session has {} trailing record bytes",
            records.len() % 9
        )));
    }

    Ok(records
        .chunks_exact(9)
        .map(|record| {
            (
                record[0],
                u64::from_le_bytes(
                    record[1..9]
                        .try_into()
                        .unwrap_or_else(|_| unreachable!("nine-byte records contain u64 values")),
                ),
            )
        })
        .collect())
}

fn audit_observation_link_map(map: &Path, required: &[&str]) -> Result<(), CommandError> {
    let contents = fs::read_to_string(map).map_err(|error| CommandError::read(map, error))?;

    audit_link_symbols(&contents, required, &[]).map_err(CommandError::ObservationSmoke)?;

    if !contents.contains("bray_runtime_observation") {
        return Err(CommandError::ObservationSmoke(
            "link map does not attribute the hooks to the Observation archive".to_owned(),
        ));
    }

    Ok(())
}

fn audit_unobserved_link_map(map: &Path) -> Result<(), CommandError> {
    let contents = fs::read_to_string(map).map_err(|error| CommandError::read(map, error))?;

    audit_link_symbols(&contents, &[], &OBSERVATION_SYMBOLS)
        .map_err(CommandError::ObservationSmoke)?;

    if contents.contains("bray_runtime_observation") {
        return Err(CommandError::ObservationSmoke(
            "ordinary product retained the Observation archive".to_owned(),
        ));
    }

    Ok(())
}
