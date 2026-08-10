use std::fs;
use std::path::Path;
use std::process::Command;

use bray_target::NativeTarget;

use super::core::{PRODUCT_NAME, executable_path, native_output, product_output};

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

    crate::native_toolchain::assemble(root, target, runtime, &toolchain)?;
    write_workspace(root, target, &workspace)?;

    let bray = root
        .join("target")
        .join("debug")
        .join(crate::native_toolchain::executable_name("bray"));

    let mut build = Command::new(&bray);

    build
        .current_dir(root)
        .arg("--workspace")
        .arg(&workspace)
        .arg("--toolchain-root")
        .arg(&toolchain)
        .arg("build");

    crate::command::require_success(
        build,
        "building standard-library hello world through Bray Tack",
    )?;

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

    let result = crate::command::require_success(
        run,
        "running standard-library hello world through Bray Tack",
    )?;

    require_hello_world_output(
        &result,
        "running standard-library hello world through Bray Tack",
    )
}

fn write_workspace(root: &Path, target: NativeTarget, workspace: &Path) -> Result<(), String> {
    let source_directory = workspace.join("src");

    fs::create_dir_all(&source_directory)
        .map_err(|error| format!("could not create hello-world workspace: {error}"))?;

    let workspace_manifest = serde_json::json!({
        "format": 1,
        "package": { "version": "0.1.0" },
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
        "version": { "workspace": true },
        "features": [],
        "source_roots": [{
            "name": "main",
            "path": "src",
        }],
        "products": [{
            "name": PRODUCT_NAME,
            "kind": "executable",
            "source_roots": ["main"],
            "targets": ["native"],
            "dependencies": [],
            "outputs": ["executable"],
        }],
    });

    write_json(&workspace.join("bray-workspace.json"), &workspace_manifest)?;

    write_json(&workspace.join("bray-package.json"), &package_manifest)?;

    crate::native_toolchain::copy_file(
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

fn require_hello_world_output(
    output: &std::process::Output,
    operation: &str,
) -> Result<(), String> {
    if !output.status.success() {
        return Err(crate::command::failure(operation, output));
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
