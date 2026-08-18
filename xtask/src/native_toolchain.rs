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

pub(crate) fn assemble_in_publication(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
    work: &Path,
    toolchain: &Path,
) -> Result<(), String> {
    let committed_standard_library = work.join("standard-library");

    crate::progress::run("Building the standard library bundle", || {
        build_standard_library(root, target, &committed_standard_library)
    })?;

    let library_root = toolchain.join("lib").join("bray");
    let standard_library = library_root.join("standard-library");

    install_committed_directory(&committed_standard_library, &standard_library)?;

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

fn install_committed_directory(source: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "toolchain component destination has no parent directory".to_owned())?;

    fs::create_dir_all(parent)
        .map_err(|error| format!("could not create toolchain component directory: {error}"))?;

    crate::bundle::rename_directory(source, destination).map_err(|error| {
        format!(
            "could not install committed toolchain component {} at {}: {error}",
            source.display(),
            destination.display()
        )
    })
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

#[cfg(test)]
mod tests {
    use super::install_committed_directory;

    #[test]
    fn outer_publication_installs_only_committed_component_directories() {
        let temporary = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary publication root must exist: {error}"));

        let destination = temporary.path().join("published");

        let publication = crate::bundle::DirectoryPublication::begin(&destination, "outer-")
            .unwrap_or_else(|error| panic!("outer publication must begin: {error}"));

        let component_destination = publication.work().join("component");

        let component =
            crate::bundle::DirectoryPublication::begin(&component_destination, "component-")
                .unwrap_or_else(|error| panic!("component publication must begin: {error}"));

        std::fs::write(component.contents().join("manifest.json"), b"committed")
            .unwrap_or_else(|error| panic!("component manifest must write: {error}"));

        let committed = component
            .publish()
            .unwrap_or_else(|error| panic!("component publication must complete: {error}"));

        let installed = publication.contents().join("toolchain/component");

        install_committed_directory(&committed, &installed)
            .unwrap_or_else(|error| panic!("committed component must install: {error}"));

        assert!(!committed.exists());

        assert_eq!(
            std::fs::read(installed.join("manifest.json"))
                .unwrap_or_else(|error| panic!("installed component must be readable: {error}")),
            b"committed"
        );

        let published = publication
            .publish()
            .unwrap_or_else(|error| panic!("outer publication must complete: {error}"));

        assert_eq!(
            std::fs::read(published.join("toolchain/component/manifest.json"))
                .unwrap_or_else(|error| panic!("published component must be readable: {error}")),
            b"committed"
        );
    }
}
