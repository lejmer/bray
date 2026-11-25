use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_runtime_interface::RuntimeArtifactMetadata;
use bray_target::NativeTarget;

pub(crate) fn retain_failed_artifacts<E>(
    directory: tempfile::TempDir,
    result: Result<(), E>,
) -> Result<(), E> {
    if result.is_err() {
        eprintln!(
            "failed native artifacts retained at {}",
            directory.keep().display()
        );
    }

    result
}

pub(crate) fn build_compiler(root: &Path) -> Result<(), String> {
    let mut command = Command::new("cargo");

    // Retain the workspace feature selection while building only compiler binaries.
    command
        .current_dir(root)
        .args(["build", "--release", "--bin", "brayc", "--bin", "bray"]);

    crate::progress::run("Building Bray compiler tools", || {
        let output = crate::command::output_with_streamed_stderr(&mut command)
            .map_err(|error| format!("could not start building Bray tools: {error}"))?;

        if output.status.success() {
            Ok(())
        } else {
            Err(crate::command::failure("building Bray tools", &output))
        }
    })
}

pub(crate) fn compiler_executable(root: &Path, name: &str) -> std::path::PathBuf {
    crate::workspace::cargo_target(root)
        .join("release")
        .join(executable_name(name))
}

pub(crate) fn prepare_bundles(
    root: &Path,
    target: NativeTarget,
) -> Result<(crate::runtime_artifact::Package, PathBuf), String> {
    let standard_library = crate::runtime_artifact::standard_library_directory(root, target);

    crate::progress::run("Preparing the standard library bundle", || {
        crate::standard_library::build_target_bundle(&root.join("standard-library"), &standard_library, target)
            .map(|_| ())
    })?;

    let runtime = crate::progress::run("Preparing the native runtime artifacts", || {
        crate::runtime_artifact::build_for_readiness(target, &crate::runtime_artifact::readiness_directory(root, target))
    })?;

    Ok((runtime, standard_library))
}

pub(crate) fn assemble_from_bundles(
    target: NativeTarget,
    runtime: &Path,
    standard_library: &Path,
    toolchain: &Path,
) -> Result<(), String> {
    let library_root = toolchain.join("lib").join("bray");

    copy_directory(standard_library, &library_root.join("standard-library"))?;

    install_runtime(runtime, target, &library_root)
}

fn install_runtime(
    runtime: &Path,
    target: NativeTarget,
    library_root: &Path,
) -> Result<(), String> {
    let metadata = runtime_artifact_metadata(runtime)?;

    let source_directory = runtime
        .parent()
        .ok_or_else(|| "runtime artifact metadata has no parent directory".to_owned())?;

    let runtime_directory = library_root.join("runtime").join(target.as_str());

    fs::create_dir_all(&runtime_directory)
        .map_err(|error| format!("could not create toolchain runtime directory: {error}"))?;

    copy_file(runtime, &runtime_directory.join("bray-runtime.brayrt"))?;

    for index in metadata.native_indexes() {
        copy_file(
            &source_directory.join(index.file_name()),
            &runtime_directory.join(index.file_name()),
        )?;
    }

    copy_directory(
        &source_directory.join("native"),
        &runtime_directory.join("native"),
    )
}

pub(crate) fn runtime_artifact_metadata(runtime: &Path) -> Result<RuntimeArtifactMetadata, String> {
    let runtime_bytes = fs::read(runtime)
        .map_err(|error| format!("could not read runtime artifact metadata: {error}"))?;

    RuntimeArtifactMetadata::decode_json(&runtime_bytes)
        .map_err(|error| format!("could not decode runtime artifact metadata: {error:?}"))
}

pub(crate) fn copy_file(source: &Path, destination: &Path) -> Result<(), String> {
    fs::copy(source, destination).map(|_| ()).map_err(|error| {
        format!(
            "could not copy {} to {}: {error}",
            source.display(),
            destination.display()
        )
    })
}

pub(crate) fn copy_directory(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| {
        format!(
            "could not create toolchain directory {}: {error}",
            destination.display()
        )
    })?;

    let entries = fs::read_dir(source).map_err(|error| {
        format!(
            "could not read toolchain directory {}: {error}",
            source.display()
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "could not read an entry from toolchain directory {}: {error}",
                source.display()
            )
        })?;

        let source = entry.path();
        let destination = destination.join(entry.file_name());

        if source.is_dir() {
            copy_directory(&source, &destination)?;
        } else {
            copy_file(&source, &destination)?;
        }
    }

    Ok(())
}

pub(crate) fn executable_name(name: &str) -> std::ffi::OsString {
    if cfg!(windows) {
        format!("{name}.exe").into()
    } else {
        name.into()
    }
}
