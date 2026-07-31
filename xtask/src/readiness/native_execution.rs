use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use bray_target::{NativeTarget, TargetOutputKind, TargetOutputName};

const STARTUP_FIXTURE: &str = "xtask/fixtures/native-execution/control-flow.bray";
const ENTRY_RESULT_FIXTURE: &str = "xtask/fixtures/native-execution/entry-i32.bray";
const ABI_FIXTURE: &str = "xtask/fixtures/native-execution/abi-primitive.bray";
const ABI_HOST: &str = "xtask/fixtures/native-execution/abi-primitive-x86_64-linux.s";
const ASYNC_UNIT_FIXTURE: &str = "xtask/fixtures/native-execution/async-unit.bray";
const ASYNC_I32_FIXTURE: &str = "xtask/fixtures/native-execution/async-i32.bray";
const ASYNC_ERROR_FIXTURE: &str = "xtask/fixtures/native-execution/async-result-error.bray";
const SYNC_PANIC_FIXTURE: &str = "xtask/fixtures/native-execution/sync-panic.bray";
const PRODUCT_NAME: &str = "application";
pub(super) fn audit(root: &Path) -> Result<(), String> {
    let target = NativeTarget::current()
        .ok_or_else(|| "native execution readiness requires a supported compiler host".to_owned())?;

    build_compiler(root)?;

    let runtime = native_output("bray-native-runtime-")?;
    let runtime = crate::runtime_artifact::build_for_readiness(target, runtime.path())?;

    audit_startup(root, target, &runtime)?;
    audit_entry_result(root, target, &runtime)?;

    if target == NativeTarget::X86_64LinuxGnu {
        audit_primitive_abi(root, target, &runtime)?;
    }

    audit_host_behavior(root, target, &runtime)?;

    crate::runtime_artifact::smoke_test_host()
}

fn audit_startup(root: &Path, target: NativeTarget, runtime: &Path) -> Result<(), String> {
    let first = native_output("bray-native-startup-first-")?;
    let second = native_output("bray-native-startup-second-")?;

    build_fixture(root, target, runtime, STARTUP_FIXTURE, first.path())?;
    build_fixture(root, target, runtime, STARTUP_FIXTURE, second.path())?;

    let first_executable = executable_path(first.path(), target);
    let second_executable = executable_path(second.path(), target);
    let first_objects = object_files(first.path(), target)?;
    let second_objects = object_files(second.path(), target)?;

    require_equal_files(&first_executable, &second_executable, "startup executable")?;
    require_equal_artifacts(&first_objects, &second_objects)?;

    execute_product(&first_executable, 0, "executing generated Bray startup")
}

fn audit_primitive_abi(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
) -> Result<(), String> {
    let first = native_output("bray-native-abi-first-")?;
    let second = native_output("bray-native-abi-second-")?;

    build_fixture(root, target, runtime, ABI_FIXTURE, first.path())?;
    build_fixture(root, target, runtime, ABI_FIXTURE, second.path())?;

    let first_executable = executable_path(first.path(), target);
    let second_executable = executable_path(second.path(), target);
    let first_objects = object_files(first.path(), target)?;
    let second_objects = object_files(second.path(), target)?;

    require_equal_files(
        &first_executable,
        &second_executable,
        "ABI fixture executable",
    )?;

    require_equal_artifacts(&first_objects, &second_objects)?;

    let report = inspect_objects(root, &first_objects)?;

    require_evidence(
        &report,
        &[
            "Format: elf64-x86-64",
            "Name: .text",
            "Name: bray_identity",
            "R_X86_64_PLT32 bray_identity",
        ],
    )?;

    let host_object = first.path().join("native-host.o");

    compile_host(root, &host_object)?;

    let host_report = inspect_objects(root, std::slice::from_ref(&host_object))?;

    require_evidence(
        &host_report,
        &["Name: _start", "R_X86_64_PLT32 bray_identity"],
    )?;

    let executable = first.path().join("native-host");
    let bray_objects = objects_without_executable_host(root, &first_objects)?;

    link_host(root, &host_object, &bray_objects, &executable)?;

    execute_product(&executable, 42, "executing Bray through the C ABI host")
}

fn audit_entry_result(root: &Path, target: NativeTarget, runtime: &Path) -> Result<(), String> {
    let first = native_output("bray-native-entry-result-first-")?;
    let second = native_output("bray-native-entry-result-second-")?;

    build_fixture(root, target, runtime, ENTRY_RESULT_FIXTURE, first.path())?;
    build_fixture(root, target, runtime, ENTRY_RESULT_FIXTURE, second.path())?;

    let first_executable = executable_path(first.path(), target);
    let second_executable = executable_path(second.path(), target);
    let first_objects = object_files(first.path(), target)?;
    let second_objects = object_files(second.path(), target)?;

    require_equal_files(
        &first_executable,
        &second_executable,
        "entry-result executable",
    )?;

    require_equal_artifacts(&first_objects, &second_objects)?;

    execute_product(
        &first_executable,
        42,
        "executing generated i32 entry result",
    )
}

fn audit_host_behavior(root: &Path, target: NativeTarget, runtime: &Path) -> Result<(), String> {
    for (name, fixture, expected, stderr) in [
        ("async unit", ASYNC_UNIT_FIXTURE, 0, None),
        ("async i32", ASYNC_I32_FIXTURE, 42, None),
        (
            "async Result.Error",
            ASYNC_ERROR_FIXTURE,
            1,
            Some("[2a, 00, 00, 00]"),
        ),
        (
            "synchronous panic",
            SYNC_PANIC_FIXTURE,
            1,
            Some("native readiness panic"),
        ),
    ] {
        let first = native_output("bray-native-host-first-")?;
        let second = native_output("bray-native-host-second-")?;

        build_fixture(root, target, runtime, fixture, first.path())?;
        build_fixture(root, target, runtime, fixture, second.path())?;

        let first_executable = executable_path(first.path(), target);
        let second_executable = executable_path(second.path(), target);

        require_equal_files(&first_executable, &second_executable, name)?;

        require_equal_artifacts(
            &object_files(first.path(), target)?,
            &object_files(second.path(), target)?,
        )?;

        let output = product_output(&first_executable, name)?;

        if output.status.code() != Some(expected) {
            return Err(command_failure(name, &output));
        }

        if let Some(required) = stderr
            && !String::from_utf8_lossy(&output.stderr).contains(required)
        {
            return Err(format!("{name} did not report required stderr evidence"));
        }
    }

    Ok(())
}

fn native_output(prefix: &str) -> Result<tempfile::TempDir, String> {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir()
        .map_err(|error| format!("could not create native output directory: {error}"))
}

fn build_compiler(root: &Path) -> Result<(), String> {
    let mut command = Command::new("cargo");

    command
        .current_dir(root)
        .args(["build", "--quiet", "--package", "brayc"]);

    require_success(command, "building brayc").map(|_| ())
}

fn build_fixture(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
    fixture: &str,
    output: &Path,
) -> Result<(), String> {
    let compiler = root
        .join("target")
        .join("debug")
        .join(executable_name("brayc"));

    let fixture = root.join(fixture);
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

    command.arg(output).arg(&fixture);

    require_success(
        command,
        &format!("building native execution fixture {}", fixture.display()),
    )
    .map(|_| ())
}

fn compile_host(root: &Path, output: &Path) -> Result<(), String> {
    let mut command = Command::new(llvm_tool(root, "clang"));

    command
        .arg("--target=x86_64-unknown-linux-gnu")
        .arg("-c")
        .arg(root.join(ABI_HOST))
        .arg("-o")
        .arg(output);

    require_success(command, "compiling the native ABI host").map(|_| ())
}

fn link_host(
    root: &Path,
    host: &Path,
    bray_objects: &[PathBuf],
    output: &Path,
) -> Result<(), String> {
    let mut command = Command::new(llvm_tool(root, "ld.lld"));

    command
        .args(["--static", "--entry=_start", "-o"])
        .arg(output)
        .arg(host)
        .args(bray_objects);

    require_success(command, "linking the native ABI host").map(|_| ())
}

fn require_equal_files(left: &Path, right: &Path, artifact: &str) -> Result<(), String> {
    let left =
        std::fs::read(left).map_err(|error| format!("could not read first {artifact}: {error}"))?;

    let right = std::fs::read(right)
        .map_err(|error| format!("could not read second {artifact}: {error}"))?;

    if left != right {
        return Err(format!("{artifact} differs across repeated native builds"));
    }

    Ok(())
}

fn object_files(directory: &Path, target: NativeTarget) -> Result<Vec<PathBuf>, String> {
    let suffix = TargetOutputName::for_native(
        target.object_format(),
        TargetOutputKind::RelocatableObject,
    )
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

fn require_equal_artifacts(left: &[PathBuf], right: &[PathBuf]) -> Result<(), String> {
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

fn inspect_objects(root: &Path, objects: &[PathBuf]) -> Result<String, String> {
    let tool = llvm_tool(root, "llvm-readobj");
    let mut report = String::new();

    for object in objects {
        let mut command = Command::new(&tool);

        command.args(["--file-headers", "--sections", "--relocations", "--symbols"]);

        command.arg(object);

        let output = require_success(command, "inspecting native object")?;

        report.push_str(&String::from_utf8_lossy(&output.stdout));
    }

    Ok(report)
}

fn require_evidence(report: &str, required: &[&str]) -> Result<(), String> {
    for required in required {
        if !report.contains(required) {
            return Err(format!(
                "native object inspection is missing required evidence: {required}"
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

fn execute_product(executable: &Path, expected: i32, operation: &str) -> Result<(), String> {
    let output = product_output(executable, operation)?;

    if output.status.code() == Some(expected) {
        return Ok(());
    }

    Err(command_failure(operation, &output))
}

fn product_output(executable: &Path, operation: &str) -> Result<Output, String> {
    Command::new(executable)
        .output()
        .map_err(|error| format!("could not start {operation}: {error}"))
}

fn executable_path(directory: &Path, target: NativeTarget) -> PathBuf {
    let name = TargetOutputName::for_native(target.object_format(), TargetOutputKind::Executable)
        .file_name(PRODUCT_NAME)
        .unwrap_or_else(|| panic!("native executable name must be valid"));

    directory.join(name)
}

fn require_success(mut command: Command, operation: &str) -> Result<Output, String> {
    let output = command
        .output()
        .map_err(|error| format!("could not start {operation}: {error}"))?;

    if output.status.success() {
        return Ok(output);
    }

    Err(command_failure(operation, &output))
}

fn command_failure(operation: &str, output: &Output) -> String {
    let mut details = Vec::new();
    let standard_output = String::from_utf8_lossy(&output.stdout);
    let standard_error = String::from_utf8_lossy(&output.stderr);

    if !standard_output.trim().is_empty() {
        details.push(format!("stdout: {}", standard_output.trim()));
    }

    if !standard_error.trim().is_empty() {
        details.push(format!("stderr: {}", standard_error.trim()));
    }

    let details = if details.is_empty() {
        String::new()
    } else {
        format!(": {}", details.join("; "))
    };

    format!("{operation} failed with status {}{details}", output.status)
}

fn llvm_tool(root: &Path, name: &str) -> PathBuf {
    crate::llvm::tool_path(root, name)
}

fn executable_name(name: &str) -> OsString {
    if cfg!(windows) {
        format!("{name}.exe").into()
    } else {
        name.into()
    }
}
