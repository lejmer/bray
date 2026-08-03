use std::fs;
use std::path::Path;
use std::process::Command;

use bray_runtime_interface::RuntimeArtifactMetadata;
use bray_target::NativeTarget;

use super::core::{
    PRODUCT_NAME, executable_name, executable_path, native_output, product_output, require_success,
};

const HELLO_WORLD_FIXTURE: &str = "xtask/fixtures/native-execution/standard-hello-world.bray";
const HELLO_WORLD_OUTPUT: &[u8] = b"Hello world!";

pub(super) fn audit_standard_hello_world(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
) -> Result<(), String> {
    let directory = native_output("bray-standard-hello-world-")?;
    let toolchain = directory.path().join("toolchain");
    let workspace = directory.path().join("workspace");

    assemble_toolchain(root, target, runtime, &toolchain)?;
    write_workspace(root, target, &workspace)?;

    let bray = root
        .join("target")
        .join("debug")
        .join(executable_name("bray"));

    let mut build = Command::new(&bray);

    build
        .current_dir(root)
        .arg("--workspace")
        .arg(&workspace)
        .arg("--toolchain-root")
        .arg(&toolchain)
        .arg("build");

    require_success(build, "building standard-library hello world through Bray Tack")?;

    let output_directory = workspace
        .join("build")
        .join("native")
        .join("example.hello")
        .join(PRODUCT_NAME);

    let executable = executable_path(&output_directory, target);
    let result = product_output(&executable, "executing Bray Tack build output")?;

    require_hello_world_output(&result, "executing Bray Tack build output")?;

    let mut run = Command::new(bray);

    run.current_dir(root)
        .arg("--workspace")
        .arg(&workspace)
        .arg("--toolchain-root")
        .arg(&toolchain)
        .arg("run");

    let result = require_success(run, "running standard-library hello world through Bray Tack")?;

    require_hello_world_output(&result, "running standard-library hello world through Bray Tack")
}

fn assemble_toolchain(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
    toolchain: &Path,
) -> Result<(), String> {
    let library_root = toolchain.join("lib").join("bray");
    let standard_library = library_root.join("standard-library");

    crate::standard_library::build_target_bundle(
        &root.join("standard-library"),
        &standard_library,
        target,
    )?;

    let runtime_bytes = fs::read(runtime)
        .map_err(|error| format!("could not read runtime artifact metadata: {error}"))?;

    let metadata = RuntimeArtifactMetadata::decode_json(&runtime_bytes)
        .map_err(|error| format!("could not decode runtime artifact metadata: {error:?}"))?;

    let source_directory = runtime
        .parent()
        .ok_or_else(|| "runtime artifact metadata has no parent directory".to_owned())?;

    let runtime_directory = library_root.join("runtime").join(target.as_str());

    fs::create_dir_all(&runtime_directory)
        .map_err(|error| format!("could not create toolchain runtime directory: {error}"))?;

    copy_file(runtime, &runtime_directory.join("bray-runtime.brayrt"))?;

    copy_file(
        &source_directory.join(metadata.archive_file_name()),
        &runtime_directory.join(metadata.archive_file_name()),
    )
}

fn write_workspace(root: &Path, target: NativeTarget, workspace: &Path) -> Result<(), String> {
    let source_directory = workspace.join("src");

    fs::create_dir_all(&source_directory)
        .map_err(|error| format!("could not create hello-world workspace: {error}"))?;

    let workspace_manifest = serde_json::json!({
        "format": 1,
        "output_root": "build",
        "targets": [{
            "name": "native",
            "identity": target.as_str(),
        }],
        "packages": [{
            "path": ".",
            "role": "root",
            "features": [],
        }],
    });

    let package_manifest = serde_json::json!({
        "format": 1,
        "identity": "example.hello",
        "features": [],
        "source_roots": [{
            "name": "main",
            "path": "src",
        }],
        "dependencies": [],
        "products": [{
            "name": PRODUCT_NAME,
            "kind": "executable",
            "source_roots": ["main"],
            "targets": ["native"],
            "outputs": ["executable"],
        }],
    });

    write_json(
        &workspace.join("bray-workspace.json"),
        &workspace_manifest,
    )?;

    write_json(&workspace.join("bray-package.json"), &package_manifest)?;

    copy_file(
        &root.join(HELLO_WORLD_FIXTURE),
        &source_directory.join("main.bray"),
    )
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("could not encode {}: {error}", path.display()))?;

    bytes.push(b'\n');

    fs::write(path, bytes).map_err(|error| format!("could not write {}: {error}", path.display()))
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), String> {
    fs::copy(source, destination)
        .map(|_| ())
        .map_err(|error| {
            format!(
                "could not copy {} to {}: {error}",
                source.display(),
                destination.display()
            )
        })
}

fn require_hello_world_output(
    output: &std::process::Output,
    operation: &str,
) -> Result<(), String> {
    if !output.status.success() {
        return Err(super::core::command_failure(operation, output));
    }

    if output.stdout != HELLO_WORLD_OUTPUT {
        return Err(format!(
            "{operation} wrote unexpected stdout: {:?}",
            String::from_utf8_lossy(&output.stdout)
        ));
    }

    if !output.stderr.is_empty() {
        return Err(format!(
            "{operation} wrote unexpected stderr: {:?}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}
