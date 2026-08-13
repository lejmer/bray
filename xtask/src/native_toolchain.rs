use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_runtime_interface::RuntimeArtifactMetadata;
use bray_target::NativeTarget;

pub(crate) fn build_compiler(root: &Path) -> Result<(), String> {
    let mut command = Command::new("cargo");

    command.current_dir(root).args([
        "build",
        "--quiet",
        "--package",
        "brayc",
        "--package",
        "bray",
    ]);

    crate::command::require_success(command, "building Bray tools").map(|_| ())
}

pub(crate) fn assemble(
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

pub(crate) fn copy_file(source: &Path, destination: &Path) -> Result<(), String> {
    fs::copy(source, destination).map(|_| ()).map_err(|error| {
        format!(
            "could not copy {} to {}: {error}",
            source.display(),
            destination.display()
        )
    })
}

pub(crate) fn executable_name(name: &str) -> std::ffi::OsString {
    if cfg!(windows) {
        format!("{name}.exe").into()
    } else {
        name.into()
    }
}

pub(crate) fn cargo_target_directory(root: &Path) -> PathBuf {
    env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map(|directory| {
            if directory.is_absolute() {
                directory
            } else {
                root.join(directory)
            }
        })
        .unwrap_or_else(|| root.join("target"))
}
