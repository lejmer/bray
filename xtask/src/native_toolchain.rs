use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use bray_runtime_interface::RuntimeArtifactMetadata;
use bray_target::NativeTarget;

pub(crate) fn build_compiler(root: &Path) -> Result<(), String> {
    let mut command = Command::new("cargo");

    command.current_dir(root).args([
        "build",
        "--quiet",
        "--release",
        "--package",
        "brayc",
        "--package",
        "bray",
    ]);

    crate::progress::run("Building Bray compiler tools", || {
        crate::command::require_success(command, "building Bray tools").map(|_| ())
    })
}

pub(crate) fn compiler_executable(root: &Path, name: &str) -> std::path::PathBuf {
    crate::workspace::cargo_target(root)
        .join("release")
        .join(executable_name(name))
}

pub(crate) fn assemble(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
    toolchain: &Path,
) -> Result<(), String> {
    let library_root = toolchain.join("lib").join("bray");
    let standard_library = library_root.join("standard-library");

    crate::progress::run("Building the standard library bundle", || {
        build_standard_library(root, target, &standard_library)
    })?;

    install_runtime(runtime, target, &library_root)
}

pub(crate) fn assemble_from_bundles(
    target: NativeTarget,
    runtime: &Path,
    standard_library: &Path,
    toolchain: &Path,
) -> Result<(), String> {
    let library_root = toolchain.join("lib").join("bray");

    copy_directory(
        standard_library,
        &library_root.join("standard-library"),
    )?;

    install_runtime(runtime, target, &library_root)
}

fn build_standard_library(root: &Path, target: NativeTarget, output: &Path) -> Result<(), String> {
    crate::standard_library::build_target_bundle(&root.join("standard-library"), output, target)
        .map(|_| ())
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

    let archive_names: BTreeSet<_> = metadata
        .components()
        .iter()
        .map(bray_runtime_interface::RuntimeArtifactComponentMetadata::archive_file_name)
        .collect();

    for archive_name in archive_names {
        copy_file(
            &source_directory.join(archive_name),
            &runtime_directory.join(archive_name),
        )?;
    }

    Ok(())
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

fn copy_directory(source: &Path, destination: &Path) -> Result<(), String> {
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
