use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_target::{NativeTarget, TargetOutputKind, TargetOutputName};

use super::core::{
    inspect_objects, llvm_tool, object_files, reject_evidence,
    require_equal_artifacts, require_evidence, standard_library_root,
};
use super::fixtures::PRODUCT_NAME;
use super::repeatable::{RepeatableFixtureAudit, audit_repeatable_fixtures};

const STATIC_STORAGE_FIXTURE: &str = "xtask/fixtures/native-execution/static_storage.bray";
const STATIC_STORAGE_CONTRIBUTION: &str =
    "xtask/fixtures/native-execution/static_storage_contribution.bray";
const STATIC_STORAGE_HOST: &str = "xtask/fixtures/native-execution/static_storage_host.c";
const STATIC_STORAGE_ARCHIVE_HOST: &str =
    "xtask/fixtures/native-execution/static_storage_archive_host.c";

pub(super) fn audit_static_storage(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
) -> Result<(), String> {
    let section = static_host_report_section(target);

    audit_repeatable_fixtures(
        root,
        target,
        runtime,
        "bray-native-static-storage-",
        RepeatableFixtureAudit::new(
            &[STATIC_STORAGE_FIXTURE, STATIC_STORAGE_CONTRIBUTION],
            42,
            "Bray-owned static storage",
            &["bray.static.host.", section],
        ),
    )?;

    audit_library_host(root, target, runtime)?;

    audit_archive_host(root, target, runtime)
}

fn audit_archive_host(root: &Path, target: NativeTarget, runtime: &Path) -> Result<(), String> {
    let output = super::core::native_output("bray-native-static-archive-")?;

    build_library(root, target, runtime, output.path(), "static-library")?;

    let archive = published_library(output.path(), bray_emitter::ArtifactKind::StaticLibrary)?;
    let report = inspect_objects(root, &object_files(output.path(), target)?)?;

    let control = report_symbol(&report, target, "bray_product_host_control_", None)?;

    let executable = compile_archive_host(root, target, runtime, &archive, control, output.path())?;

    crate::command::require_success(
        Command::new(executable),
        "executing the native static-archive product host",
    )
    .map(|_| ())
}

fn audit_library_host(root: &Path, target: NativeTarget, runtime: &Path) -> Result<(), String> {
    let first = super::core::native_output("bray-native-static-library-first-")?;
    let second = super::core::native_output("bray-native-static-library-second-")?;

    build_library(root, target, runtime, first.path(), "shared-library")?;

    build_library(root, target, runtime, second.path(), "shared-library")?;

    let first_library = shared_library_path(first.path(), target)?;
    let second_library = shared_library_path(second.path(), target)?;
    let first_objects = object_files(first.path(), target)?;
    let second_objects = object_files(second.path(), target)?;

    require_equal_artifacts(&first_objects, &second_objects)?;

    let first_bytes = std::fs::read(&first_library)
        .map_err(|error| format!("could not read first static-host shared library: {error}"))?;

    let second_bytes = std::fs::read(&second_library)
        .map_err(|error| format!("could not read second static-host shared library: {error}"))?;

    if first_bytes != second_bytes {
        return Err("static-host shared libraries differ across repeated builds".to_owned());
    }

    let report = inspect_objects(root, &first_objects)?;

    require_evidence(
        &report,
        &[
            "bray.static.host.",
            static_host_report_section(target),
            "bray_product_host_",
            "bray_product_host_control_",
            initialized_data_section(target),
            zero_data_section(target),
        ],
    )?;

    reject_evidence(
        &report,
        &[
            "SHARED_COUNTER",
            "THREAD_ANSWER",
            "GENERIC_VALUE",
            "ZERO_VALUE",
        ],
    )?;

    let control = report_symbol(&report, target, "bray_product_host_control_", None)?;

    let descriptor = report_symbol(
        &report,
        target,
        "bray_product_host_",
        Some("bray_product_host_control_"),
    )?;

    let hidden_static = report_symbol(&report, target, "bray_static_", None)?;
    let loaded = super::core::native_output("bray-native-static-host-load-")?;
    let first_copy = loaded.path().join(shared_copy_name(target, "first")?);
    let second_copy = loaded.path().join(shared_copy_name(target, "second")?);

    std::fs::copy(&first_library, &first_copy)
        .map_err(|error| format!("could not copy first static-host library: {error}"))?;

    std::fs::copy(&first_library, &second_copy)
        .map_err(|error| format!("could not copy second static-host library: {error}"))?;

    let harness = compile_host(root, target, loaded.path())?;
    let mut command = Command::new(&harness);

    command.args([&first_copy, &second_copy]);
    command.args([control, descriptor, hidden_static]);

    crate::command::require_success(command, "executing the native static product host").map(|_| ())
}

fn build_library(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
    output: &Path,
    artifact: &str,
) -> Result<(), String> {
    let compiler = crate::native_toolchain::compiler_executable(root, "brayc");
    let mut command = Command::new(compiler);

    command.current_dir(root).args([
        "build",
        "--product-kind",
        "library",
        "--product",
        PRODUCT_NAME,
        "--target",
        target.as_str(),
        "--artifact",
        artifact,
        "--inspect",
        "relocatable-object",
        "--runtime-artifact",
    ]);

    command.arg(runtime).args(["--output"]);
    command.arg(output);

    command
        .arg("--standard-library-root")
        .arg(standard_library_root(root));

    command.arg(root.join(STATIC_STORAGE_FIXTURE));
    command.arg(root.join(STATIC_STORAGE_CONTRIBUTION));

    crate::command::require_success(command, "building the native static-host library").map(|_| ())
}

fn compile_host(root: &Path, target: NativeTarget, output: &Path) -> Result<PathBuf, String> {
    let compiler = llvm_tool(
        root,
        bray_diagnostics::DiagnosticLlvmToolRole::CompilerDriver,
    );

    let executable = output.join(
        TargetOutputName::for_native(target.object_format(), TargetOutputKind::Executable)
            .file_name("static_storage_host")
            .ok_or_else(|| "native static-host executable name is invalid".to_owned())?,
    );

    let mut command = Command::new(compiler);

    command
        .arg(format!("--target={}", target.as_str()))
        .arg("-std=c11")
        .arg(root.join(STATIC_STORAGE_HOST))
        .arg("-o")
        .arg(&executable);

    if matches!(
        target,
        NativeTarget::X86_64LinuxGnu | NativeTarget::Aarch64LinuxGnu
    ) {
        command.args(["-ldl", "-pthread"]);
    } else if matches!(
        target,
        NativeTarget::X86_64MacOs | NativeTarget::Aarch64MacOs
    ) {
        command.arg("-pthread");
    }

    crate::command::require_success(command, "compiling the native static-host harness")?;

    Ok(executable)
}

fn compile_archive_host(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
    archive: &Path,
    control: &str,
    output: &Path,
) -> Result<PathBuf, String> {
    let compiler = llvm_tool(
        root,
        bray_diagnostics::DiagnosticLlvmToolRole::CompilerDriver,
    );

    let executable = output.join(
        TargetOutputName::for_native(target.object_format(), TargetOutputKind::Executable)
            .file_name("static_storage_archive_host")
            .ok_or_else(|| "native static-archive host executable name is invalid".to_owned())?,
    );

    let mut command = Command::new(compiler);

    command
        .arg(format!("--target={}", target.as_str()))
        .arg("-std=c11")
        .arg(format!("-DBRAY_PRODUCT_HOST_CONTROL={control}"))
        .arg(root.join(STATIC_STORAGE_ARCHIVE_HOST))
        .arg(archive);

    for runtime_archive in runtime_archives(runtime)? {
        command.arg(runtime_archive);
    }

    for requirement in runtime_native_links(runtime)? {
        match requirement.kind() {
            bray_symbols::NativeLinkKind::Framework => {
                command.arg("-framework").arg(requirement.name());
            }
            bray_symbols::NativeLinkKind::Dynamic
            | bray_symbols::NativeLinkKind::Static
            | bray_symbols::NativeLinkKind::System => {
                command.arg(format!("-l{}", requirement.name()));
            }
        }
    }

    command.arg("-o").arg(&executable);

    if matches!(
        target,
        NativeTarget::X86_64LinuxGnu | NativeTarget::Aarch64LinuxGnu
    ) {
        command.args(["-ldl", "-pthread"]);
    } else if matches!(
        target,
        NativeTarget::X86_64MacOs | NativeTarget::Aarch64MacOs
    ) {
        command.arg("-pthread");
    }

    crate::command::require_success(command, "linking the native static-archive host")?;

    Ok(executable)
}

fn runtime_archives(runtime: &Path) -> Result<Vec<PathBuf>, String> {
    let metadata = runtime_metadata(runtime)?;

    let directory = runtime
        .parent()
        .ok_or_else(|| "native runtime metadata has no parent directory".to_owned())?;

    Ok(metadata
        .components()
        .iter()
        .map(|component| directory.join(component.archive_file_name()))
        .collect())
}

fn runtime_native_links(
    runtime: &Path,
) -> Result<BTreeSet<bray_symbols::NativeLinkRequirement>, String> {
    Ok(runtime_metadata(runtime)?
        .components()
        .iter()
        .flat_map(bray_runtime_interface::RuntimeArtifactComponentMetadata::native_links)
        .cloned()
        .collect())
}

fn runtime_metadata(
    runtime: &Path,
) -> Result<bray_runtime_interface::RuntimeArtifactMetadata, String> {
    crate::native_toolchain::runtime_artifact_metadata(runtime)
}

fn shared_library_path(output: &Path, target: NativeTarget) -> Result<PathBuf, String> {
    let path = published_library(output, bray_emitter::ArtifactKind::SharedLibrary)?;

    let expected =
        TargetOutputName::for_native(target.object_format(), TargetOutputKind::SharedLibrary)
            .file_name(PRODUCT_NAME)
            .ok_or_else(|| "native static-host shared-library name is invalid".to_owned())?;

    if path.file_name() != Some(expected.as_ref()) {
        return Err("native static-host shared-library name is inconsistent".to_owned());
    }

    Ok(path)
}

fn published_library(
    output: &Path,
    artifact: bray_emitter::ArtifactKind,
) -> Result<PathBuf, String> {
    let package = bray_symbols::PackageIdentity::try_new("command.line")
        .ok_or_else(|| "native static-host package identity is invalid".to_owned())?;

    let product = bray_symbols::ProductIdentity::try_new(package, PRODUCT_NAME)
        .ok_or_else(|| "native static-host product identity is invalid".to_owned())?;

    bray_emitter::resolve_published_artifact(output, &product, artifact, 0)
        .map_err(|error| format!("could not resolve native static-host library: {error:?}"))
}

fn shared_copy_name(target: NativeTarget, stem: &str) -> Result<String, String> {
    TargetOutputName::for_native(target.object_format(), TargetOutputKind::SharedLibrary)
        .file_name(stem)
        .ok_or_else(|| "native static-host shared-library copy name is invalid".to_owned())
}

fn report_symbol<'report>(
    report: &'report str,
    target: NativeTarget,
    fragment: &str,
    excluded_fragment: Option<&str>,
) -> Result<&'report str, String> {
    let symbol = report.lines().find_map(|line| {
        let name = line
            .trim()
            .strip_prefix("Name: ")?
            .split_whitespace()
            .next()?;

        let name = if matches!(
            target,
            NativeTarget::X86_64MacOs | NativeTarget::Aarch64MacOs
        ) {
            name.strip_prefix('_').unwrap_or(name)
        } else {
            name
        };

        let suffix = name.strip_prefix(fragment)?;

        if suffix.len() != 64
            || !suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
            || excluded_fragment.is_some_and(|excluded| name.contains(excluded))
        {
            return None;
        }

        Some(name)
    });

    symbol.ok_or_else(|| format!("native object inspection is missing symbol {fragment}"))
}

fn static_host_report_section(target: NativeTarget) -> &'static str {
    let section = bray_codegen::static_host_section_name(target.object_format());

    section
        .rsplit_once(',')
        .map_or(section, |(_, section)| section)
}

const fn initialized_data_section(target: NativeTarget) -> &'static str {
    match target {
        NativeTarget::X86_64MacOs | NativeTarget::Aarch64MacOs => "__data",
        NativeTarget::X86_64WindowsMsvc
        | NativeTarget::Aarch64WindowsMsvc
        | NativeTarget::X86_64LinuxGnu
        | NativeTarget::Aarch64LinuxGnu => ".data",
    }
}

const fn zero_data_section(target: NativeTarget) -> &'static str {
    match target {
        NativeTarget::X86_64MacOs | NativeTarget::Aarch64MacOs => "__bss",
        NativeTarget::X86_64WindowsMsvc
        | NativeTarget::Aarch64WindowsMsvc
        | NativeTarget::X86_64LinuxGnu
        | NativeTarget::Aarch64LinuxGnu => ".bss",
    }
}
