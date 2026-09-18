use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use bray_target::{NativeTarget, TargetOutputKind, TargetOutputName};
use bray_tooling::{InspectionTarget, OutputFormat, render_lowered_inspection};

use super::buffer::{
    audit_standard_buffer, audit_standard_format, build_standard_library_fixtures,
    standard_library_compilation,
};
use super::built_fixture::BuiltFixture;
use super::fixtures::{
    ABI_FIXTURE, ABI_HOST, ASYNC_ERROR_FIXTURE, ASYNC_I32_FIXTURE, ASYNC_TASKS_FIXTURE,
    ASYNC_UNIT_FIXTURE, ATOMIC_FIXTURE, CALL_REBORROWS_FIXTURE, ENTRY_RESULT_FIXTURE,
    GUARDED_PART_CLEANUP_FIXTURE, GUARDED_ROOT_CLEANUP_FIXTURE, HEAP_STORAGE_FIXTURE,
    MEMORY_FIXTURE, MEMORY_LAYOUT_FIXTURE, PATTERN_CONDITIONS_FIXTURE, PRODUCT_NAME, RANGE_FIXTURE,
    STANDARD_MEMORY_FIXTURE, STANDARD_RUN_SOURCE, STANDARD_TASK_SOURCE, STANDARD_TESTING_SOURCE,
    STANDARD_TEXT_FIXTURE, STARTUP_FIXTURE, SYNC_CATCH_PROPAGATION_FIXTURE, SYNC_PANIC_FIXTURE,
    TEXT_CURSOR_FIXTURE, VALUE_REPLACEMENT_FIXTURE,
};
use super::hello::audit_standard_hello_world;
use super::nullable::audit_nullable_state_queries;
use super::rejection::audit_rejections;
use super::repeatable::audit_repeatable_fixture;
use super::static_storage::audit_static_storage;

pub(crate) fn audit(root: &Path) -> Result<(), String> {
    let target = NativeTarget::current().ok_or_else(|| {
        "native execution readiness requires a supported compiler host".to_owned()
    })?;

    crate::native_toolchain::build_compiler(root)?;

    crate::progress::run("Preparing native readiness standard library", || {
        crate::standard_library::build_target_bundle(
            &root.join("standard-library"),
            &standard_library_root(root),
            target,
        )
        .map(|_| ())
    })?;

    crate::progress::run("Checking native semantic rejections", || {
        audit_rejections(root, target, &standard_library_root(root))
    })?;

    let runtime = native_output("bray-native-runtime-")?;

    let runtime = crate::progress::run("Building native readiness runtime artifacts", || {
        crate::runtime_artifact::build_for_readiness(target, runtime.path())
    })?;

    crate::progress::run("Checking native static storage", || {
        audit_static_storage(root, target, &runtime)
    })?;

    crate::progress::run("Checking standard-library hello world execution", || {
        audit_standard_hello_world(root, target, &runtime)
    })?;

    crate::progress::run("Checking standard formatting execution", || {
        audit_standard_format(root, target, &runtime)
    })?;

    crate::progress::run("Checking standard buffer execution", || {
        audit_standard_buffer(root, target, &runtime)
    })?;

    crate::progress::run("Checking text cursor execution", || {
        audit_text_cursor(root, target, &runtime)
    })?;

    crate::progress::run("Checking native startup", || {
        audit_startup(root, target, &runtime)
    })?;

    crate::progress::run("Checking half-open range execution", || {
        audit_half_open_ranges(root, target, &runtime)
    })?;

    crate::progress::run("Checking nullable state queries", || {
        audit_nullable_state_queries(root, target, &runtime)
    })?;

    crate::progress::run("Checking guarded root cleanup", || {
        audit_repeatable_fixture(
            root,
            target,
            &runtime,
            "bray-native-guarded-root-",
            GUARDED_ROOT_CLEANUP_FIXTURE,
            0,
            "guarded root cleanup",
            &[],
        )
    })?;

    crate::progress::run("Checking guarded represented-part cleanup", || {
        audit_repeatable_fixture(
            root,
            target,
            &runtime,
            "bray-native-guarded-part-",
            GUARDED_PART_CLEANUP_FIXTURE,
            0,
            "guarded represented-part cleanup",
            &[],
        )
    })?;

    crate::progress::run("Checking heap storage execution", || {
        audit_repeatable_fixture(
            root,
            target,
            &runtime,
            "bray-native-heap-storage-",
            HEAP_STORAGE_FIXTURE,
            0,
            "heap storage execution",
            &[],
        )
    })?;

    for (fixture, prefix, name) in [
        (
            VALUE_REPLACEMENT_FIXTURE,
            "bray-native-replacement-",
            "value replacement",
        ),
        (
            CALL_REBORROWS_FIXTURE,
            "bray-native-call-reborrows-",
            "call reborrows",
        ),
    ] {
        crate::progress::run(&format!("Checking {name}"), || {
            audit_repeatable_fixture(root, target, &runtime, prefix, fixture, 0, name, &[])
        })?;
    }

    crate::progress::run("Checking pattern conditions", || {
        audit_repeatable_fixture(
            root,
            target,
            &runtime,
            "bray-native-pattern-",
            PATTERN_CONDITIONS_FIXTURE,
            0,
            "pattern conditions",
            &[],
        )
    })?;

    crate::progress::run("Checking native entry results", || {
        audit_entry_result(root, target, &runtime)
    })?;

    crate::progress::run("Checking native memory operations", || {
        audit_memory_operations(root, target, &runtime)
    })?;

    crate::progress::run("Checking native atomic operations", || {
        audit_atomic_operations(root, target, &runtime)
    })?;

    crate::progress::run("Checking native memory layout", || {
        audit_memory_layout(root, target, &runtime)
    })?;

    if target == NativeTarget::X86_64LinuxGnu {
        crate::progress::run("Checking the native primitive ABI", || {
            audit_primitive_abi(root, target, &runtime)
        })?;
    }

    crate::progress::run("Checking native host behavior", || {
        audit_host_behavior(root, target, &runtime)
    })?;

    crate::progress::run("Checking runtime artifact integration", || {
        crate::runtime_artifact::smoke_test_host()
    })
}

fn audit_memory_operations(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
) -> Result<(), String> {
    audit_repeatable_fixture(
        root,
        target,
        runtime,
        "bray-native-memory-",
        MEMORY_FIXTURE,
        42,
        "Bray-provided memory operations",
        &[],
    )
}

fn audit_atomic_operations(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
) -> Result<(), String> {
    let first = native_output("bray-native-atomic-first-")?;
    let second = native_output("bray-native-atomic-second-")?;

    build_fixture(root, target, runtime, ATOMIC_FIXTURE, first.path())?;
    build_fixture(root, target, runtime, ATOMIC_FIXTURE, second.path())?;

    let first_executable = executable_path(first.path(), "command.line")?;
    let second_executable = executable_path(second.path(), "command.line")?;
    let first_objects = object_files(first.path(), target)?;
    let second_objects = object_files(second.path(), target)?;

    require_equal_files(
        first_executable.path(),
        second_executable.path(),
        "atomic executable",
    )?;

    require_equal_artifacts(&first_objects, &second_objects)?;

    let report = inspect_objects(root, &first_objects)?;

    reject_evidence(
        &report,
        &["__atomic", "pthread_mutex", "WaitForSingleObject"],
    )?;

    execute_product(
        first_executable.path(),
        42,
        "executing compiler-provided atomic operations",
    )
}

fn audit_memory_layout(root: &Path, target: NativeTarget, runtime: &Path) -> Result<(), String> {
    let fixtures = [STANDARD_MEMORY_FIXTURE, MEMORY_LAYOUT_FIXTURE];

    let first = BuiltFixture::build_standard_library(
        "bray-native-memory-layout-first-",
        target,
        |output| build_standard_library_fixtures(root, target, runtime, output, &fixtures),
    )?;

    let second = BuiltFixture::build_standard_library(
        "bray-native-memory-layout-second-",
        target,
        |output| build_standard_library_fixtures(root, target, runtime, output, &fixtures),
    )?;

    require_equal_files(
        first.executable(),
        second.executable(),
        "memory layout executable",
    )?;

    require_equal_artifacts(first.objects(), second.objects())?;
    require_lowered_layout_operations(root, target)?;

    execute_product(first.executable(), 42, "executing memory layout operations")
}

fn audit_text_cursor(root: &Path, target: NativeTarget, runtime: &Path) -> Result<(), String> {
    let output =
        BuiltFixture::build_standard_library("bray-native-text-cursor-", target, |output| {
            build_standard_library_fixtures(
                root,
                target,
                runtime,
                output,
                &[STANDARD_TEXT_FIXTURE, TEXT_CURSOR_FIXTURE],
            )
        })?;

    execute_product(
        output.executable(),
        42,
        "executing standard text scalar iteration",
    )
}

fn require_lowered_layout_operations(root: &Path, target: NativeTarget) -> Result<(), String> {
    let compilation = standard_library_compilation(
        root,
        target,
        &[STANDARD_MEMORY_FIXTURE, MEMORY_LAYOUT_FIXTURE],
    )?;

    let inspection = render_lowered_inspection(
        &compilation,
        InspectionTarget::source(1),
        OutputFormat::Json,
    )
    .map_err(|_| "could not inspect lowered memory layout operations".to_owned())?;

    let (report, diagnostics) = inspection.into_parts();

    if diagnostics.has_errors() {
        return Err(format!(
            "lowered memory layout inspection reported diagnostics: {diagnostics:?}"
        ));
    }

    require_evidence(
        report.as_str(),
        &[
            r#""value": "size_of""#,
            r#""value": "align_of""#,
            r#""value": "stride_of""#,
            r#""value": "layout_of""#,
        ],
    )
}

fn audit_startup(root: &Path, target: NativeTarget, runtime: &Path) -> Result<(), String> {
    audit_repeatable_fixture(
        root,
        target,
        runtime,
        "bray-native-startup-",
        STARTUP_FIXTURE,
        0,
        "generated Bray startup",
        &[],
    )
}

fn audit_half_open_ranges(root: &Path, target: NativeTarget, runtime: &Path) -> Result<(), String> {
    audit_repeatable_fixture(
        root,
        target,
        runtime,
        "bray-native-range-",
        RANGE_FIXTURE,
        10,
        "half-open ranges",
        &[],
    )
}

fn audit_primitive_abi(root: &Path, target: NativeTarget, runtime: &Path) -> Result<(), String> {
    let first = BuiltFixture::build_command_line("bray-native-abi-first-", target, |output| {
        build_fixture(root, target, runtime, ABI_FIXTURE, output)
    })?;

    let second = BuiltFixture::build_command_line("bray-native-abi-second-", target, |output| {
        build_fixture(root, target, runtime, ABI_FIXTURE, output)
    })?;

    require_equal_files(
        first.executable(),
        second.executable(),
        "ABI fixture executable",
    )?;

    require_equal_artifacts(first.objects(), second.objects())?;

    let report = inspect_objects(root, first.objects())?;

    require_evidence(
        &report,
        &[
            "Format: elf64-x86-64",
            "Name: .text",
            "Name: bray_identity",
            "R_X86_64_PLT32 bray_identity",
        ],
    )?;

    let host_object = first.output().join("native-host.o");

    compile_host(root, &host_object)?;

    let host_report = inspect_objects(root, std::slice::from_ref(&host_object))?;

    require_evidence(
        &host_report,
        &["Name: _start", "R_X86_64_PLT32 bray_identity"],
    )?;

    let executable = first.output().join("native-host");
    let bray_objects = objects_without_executable_host(root, first.objects())?;

    link_host(root, &host_object, &bray_objects, &executable)?;

    execute_product(&executable, 42, "executing Bray through the C ABI host")
}

fn audit_entry_result(root: &Path, target: NativeTarget, runtime: &Path) -> Result<(), String> {
    audit_repeatable_fixture(
        root,
        target,
        runtime,
        "bray-native-entry-result-",
        ENTRY_RESULT_FIXTURE,
        42,
        "generated i32 entry result",
        &[],
    )
}

fn audit_host_behavior(root: &Path, target: NativeTarget, runtime: &Path) -> Result<(), String> {
    for (name, fixture, expected, stderr, uses_standard_library) in [
        ("async unit", ASYNC_UNIT_FIXTURE, 0, None, false),
        ("async i32", ASYNC_I32_FIXTURE, 42, None, false),
        (
            "synchronous catch propagation",
            SYNC_CATCH_PROPAGATION_FIXTURE,
            0,
            None,
            false,
        ),
        (
            "independently started tasks",
            ASYNC_TASKS_FIXTURE,
            0,
            None,
            true,
        ),
        (
            "async Result.Error",
            ASYNC_ERROR_FIXTURE,
            1,
            Some("[2a, 00, 00, 00]"),
            false,
        ),
        (
            "synchronous panic",
            SYNC_PANIC_FIXTURE,
            1,
            Some("native readiness panic"),
            false,
        ),
    ] {
        let first = build_host_fixture(
            root,
            target,
            runtime,
            "bray-native-host-first-",
            fixture,
            uses_standard_library,
        )?;

        let second = build_host_fixture(
            root,
            target,
            runtime,
            "bray-native-host-second-",
            fixture,
            uses_standard_library,
        )?;

        require_equal_files(first.executable(), second.executable(), name)?;

        require_equal_artifacts(first.objects(), second.objects())?;

        let output = product_output(first.executable(), name)?;

        if output.status.code() != Some(expected) {
            return Err(crate::command::failure(name, &output));
        }

        if let Some(required) = stderr
            && !output_contains(&output, required)
        {
            return Err(format!("{name} did not report required stderr evidence"));
        }
    }

    Ok(())
}

fn build_host_fixture(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
    prefix: &str,
    fixture: &str,
    uses_standard_library: bool,
) -> Result<BuiltFixture, String> {
    if uses_standard_library {
        BuiltFixture::build_standard_library(prefix, target, |output| {
            build_standard_library_fixtures(
                root,
                target,
                runtime,
                output,
                &[
                    STANDARD_RUN_SOURCE,
                    STANDARD_TASK_SOURCE,
                    STANDARD_TESTING_SOURCE,
                    fixture,
                ],
            )
        })
    } else {
        BuiltFixture::build_command_line(prefix, target, |output| {
            build_fixture(root, target, runtime, fixture, output)
        })
    }
}

pub(super) fn native_output(prefix: &str) -> Result<tempfile::TempDir, String> {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir()
        .map_err(|error| format!("could not create native output directory: {error}"))
}

fn build_fixture(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
    fixture: &str,
    output: &Path,
) -> Result<(), String> {
    build_fixtures(root, target, runtime, output, None, &[fixture])
}

pub(super) fn build_fixtures(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
    output: &Path,
    package: Option<&str>,
    fixtures: &[&str],
) -> Result<(), String> {
    let compiler = crate::native_toolchain::compiler_executable(root, "brayc");

    let mut command = Command::new(compiler);

    command.current_dir(root).args([
        "build",
        "--product-kind",
        "executable",
        "--target",
        target.as_str(),
        "--inspect",
        "relocatable-object",
        "--runtime-artifact",
    ]);

    command.arg(runtime).args(["--output"]);

    command.arg(output);

    command
        .arg("--standard-library-root")
        .arg(standard_library_root(root));

    if let Some(package) = package {
        command.args(["--package", package]);
    }

    for fixture in fixtures {
        command.arg(root.join(fixture));
    }

    let operation = format!("building native execution fixtures {fixtures:?}");

    crate::command::require_success(command, &operation).map(|_| ())
}

pub(super) fn standard_library_root(root: &Path) -> PathBuf {
    crate::workspace::cargo_target(root)
        .join("release")
        .join("lib")
        .join("bray")
        .join("standard-library")
}

fn compile_host(root: &Path, output: &Path) -> Result<(), String> {
    let mut command = Command::new(llvm_tool(
        root,
        bray_diagnostics::DiagnosticLlvmToolRole::CompilerDriver,
    ));

    command
        .arg("--target=x86_64-unknown-linux-gnu")
        .arg("-c")
        .arg(root.join(ABI_HOST))
        .arg("-o")
        .arg(output);

    crate::command::require_success(command, "compiling the native ABI host").map(|_| ())
}

fn link_host(
    root: &Path,
    host: &Path,
    bray_objects: &[PathBuf],
    output: &Path,
) -> Result<(), String> {
    let mut command = Command::new(llvm_tool(
        root,
        bray_diagnostics::DiagnosticLlvmToolRole::Linker,
    ));

    command
        .args(["--static", "--entry=_start", "-o"])
        .arg(output)
        .arg(host)
        .args(bray_objects);

    crate::command::require_success(command, "linking the native ABI host").map(|_| ())
}

pub(super) fn require_equal_files(left: &Path, right: &Path, artifact: &str) -> Result<(), String> {
    let left =
        std::fs::read(left).map_err(|error| format!("could not read first {artifact}: {error}"))?;

    let right = std::fs::read(right)
        .map_err(|error| format!("could not read second {artifact}: {error}"))?;

    if left != right {
        return Err(format!("{artifact} differs across repeated native builds"));
    }

    Ok(())
}

pub(super) fn object_files(directory: &Path, target: NativeTarget) -> Result<Vec<PathBuf>, String> {
    let suffix =
        TargetOutputName::for_native(target.object_format(), TargetOutputKind::RelocatableObject)
            .suffix()
            .trim_start_matches('.')
            .to_owned();

    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("could not list native output directory: {error}"))?;

    let mut objects = entries
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("could not read native output entry: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    objects.retain(|path| {
        path.extension()
            .is_some_and(|extension| extension == suffix.as_str())
    });

    objects.sort();

    if objects.is_empty() {
        return Err("native build produced no relocatable objects".to_owned());
    }

    Ok(objects)
}

pub(super) fn require_equal_artifacts(left: &[PathBuf], right: &[PathBuf]) -> Result<(), String> {
    if left.len() != right.len() {
        return Err("repeated native builds produced different object counts".to_owned());
    }

    for (left, right) in left.iter().zip(right) {
        if left.file_name() != right.file_name() {
            return Err("repeated native builds produced different object names".to_owned());
        }

        require_equal_files(left, right, "relocatable object")?;
    }

    Ok(())
}

pub(super) fn inspect_objects(root: &Path, objects: &[PathBuf]) -> Result<String, String> {
    let tool = llvm_tool(
        root,
        bray_diagnostics::DiagnosticLlvmToolRole::ObjectInspector,
    );

    let mut report = String::new();

    for object in objects {
        let mut command = Command::new(&tool);

        command.args(["--file-headers", "--sections", "--relocations", "--symbols"]);

        command.arg(object);

        let output = crate::command::require_success(command, "inspecting native object")?;

        report.push_str(&String::from_utf8_lossy(&output.stdout));
    }

    Ok(report)
}

pub(super) fn require_evidence(report: &str, required: &[&str]) -> Result<(), String> {
    for required in required {
        if !report.contains(required) {
            return Err(format!(
                "native object inspection is missing required evidence: {required}"
            ));
        }
    }

    Ok(())
}

pub(super) fn reject_evidence(report: &str, forbidden: &[&str]) -> Result<(), String> {
    for forbidden in forbidden {
        if report.contains(forbidden) {
            return Err(format!(
                "native object inspection contains forbidden evidence: {forbidden}"
            ));
        }
    }

    Ok(())
}

fn objects_without_executable_host(
    root: &Path,
    objects: &[PathBuf],
) -> Result<Vec<PathBuf>, String> {
    let mut retained = Vec::new();
    let mut host_objects = 0_usize;

    for object in objects {
        let report = inspect_objects(root, std::slice::from_ref(object))?;

        if report.contains("Name: main") {
            host_objects = host_objects.saturating_add(1);
        } else {
            retained.push(object.clone());
        }
    }

    if host_objects != 1 {
        return Err(format!(
            "native ABI fixture produced {host_objects} executable host objects instead of one"
        ));
    }

    Ok(retained)
}

pub(super) fn execute_product(
    executable: &Path,
    expected: i32,
    operation: &str,
) -> Result<(), String> {
    let output = product_output(executable, operation)?;

    if output.status.code() == Some(expected) {
        return Ok(());
    }

    Err(crate::command::failure(operation, &output))
}

pub(super) fn product_output(executable: &Path, operation: &str) -> Result<Output, String> {
    Command::new(executable)
        .output()
        .map_err(|error| format!("could not start {operation}: {error}"))
}

pub(super) fn executable_path(
    destination: impl Into<bray_emitter::ManagedFilesystemDestination>,
    package: &str,
) -> Result<bray_emitter::PublishedArtifact, String> {
    let package = bray_symbols::PackageIdentity::try_new(package)
        .ok_or_else(|| "native fixture package identity is invalid".to_owned())?;

    let product = bray_symbols::ProductIdentity::try_new(package, PRODUCT_NAME)
        .ok_or_else(|| "native fixture product identity is invalid".to_owned())?;

    bray_emitter::resolve_published_artifact(
        destination,
        &product,
        bray_emitter::ArtifactKind::Executable,
        0,
    )
    .map_err(|error| format!("could not resolve native fixture executable: {error:?}"))
}

pub(super) fn output_contains(output: &Output, required: &str) -> bool {
    String::from_utf8_lossy(&output.stdout).contains(required)
        || String::from_utf8_lossy(&output.stderr).contains(required)
}

pub(super) fn llvm_tool(root: &Path, tool: bray_diagnostics::DiagnosticLlvmToolRole) -> PathBuf {
    bray_tooling::llvm_tool_path(tool)
        .unwrap_or_else(|_| bray_llvm_toolchain::tool_path(root, tool.executable_name()))
}
