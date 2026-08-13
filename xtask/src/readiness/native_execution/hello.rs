use std::fs;
use std::path::Path;
use std::process::Command;

use bray_emitter::ArtifactKind;
use bray_symbols::{PackageIdentity, ProductIdentity};
use bray_target::NativeTarget;

use super::core::{PRODUCT_NAME, native_output, product_output};

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

    let bray = crate::workspace::cargo_target(root)
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
        .join("debug")
        .join("example.hello")
        .join(PRODUCT_NAME);

    let package = PackageIdentity::try_new("example.hello")
        .ok_or_else(|| "hello-world package identity is invalid".to_owned())?;

    let product = ProductIdentity::try_new(package, PRODUCT_NAME)
        .ok_or_else(|| "hello-world product identity is invalid".to_owned())?;

    let executable = bray_emitter::resolve_published_artifact(
        &output_directory,
        &product,
        ArtifactKind::Executable,
        0,
    )
    .map_err(|error| format!("could not resolve Bray Tack build output: {error:?}"))?;

    let result = product_output(&executable, "executing Bray Tack build output")?;

    require_hello_world_output(&result, "executing Bray Tack build output", None)?;

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
        Some(target),
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
    crate::json::write_pretty(path, value)
}

fn require_hello_world_output(
    output: &std::process::Output,
    operation: &str,
    progress_target: Option<NativeTarget>,
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

    let progress = String::from_utf8_lossy(&output.stderr);

    if let Some(target) = progress_target {
        let finished = finished_progress(target)?;

        if !progress.contains("Building example.hello/application [debug]")
            || !progress.contains(&finished)
        {
            return Err(format!("{operation} omitted expected build progress: {progress:?}"));
        }
    }

    if progress_target.is_none() && !output.stderr.is_empty() {
        return Err(format!(
            "{operation} wrote unexpected stderr: {:?}",
            progress
        ));
    }

    Ok(())
}

fn finished_progress(target: NativeTarget) -> Result<String, String> {
    let executable_name = bray_target::TargetOutputName::for_native(
        target.object_format(),
        bray_target::TargetOutputKind::Executable,
    )
    .file_name(PRODUCT_NAME)
    .ok_or_else(|| "hello-world executable name is invalid".to_owned())?;

    Ok(format!(
        "Finished {executable_name} build/native/debug/example.hello/application"
    ))
}

#[cfg(test)]
mod tests {
    use bray_target::NativeTarget;

    use super::finished_progress;

    #[test]
    fn finished_progress_uses_the_target_executable_name() {
        assert_eq!(
            finished_progress(NativeTarget::X86_64WindowsMsvc),
            Ok(String::from(
                "Finished application.exe build/native/debug/example.hello/application"
            ))
        );

        for target in [
            NativeTarget::X86_64LinuxGnu,
            NativeTarget::Aarch64LinuxGnu,
            NativeTarget::X86_64MacOs,
            NativeTarget::Aarch64MacOs,
        ] {
            assert_eq!(
                finished_progress(target),
                Ok(String::from(
                    "Finished application build/native/debug/example.hello/application"
                ))
            );
        }
    }
}
