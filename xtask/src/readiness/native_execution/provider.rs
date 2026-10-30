use std::path::Path;

use bray_target::{NativeTarget, TargetOutputKind};

use super::static_storage::{build_library, compile_fixture, shared_library_path};

const LIFETIME: &str = "xtask/fixtures/native-execution/provider-lifetime.bray";
const IMPORTED: &str = "xtask/fixtures/native-execution/provider-imported.bray";

pub(super) fn audit(root: &Path, target: NativeTarget, runtime: &Path) -> Result<(), String> {
    let library = super::core::native_output("bray-native-provider-library-")?;
    let host = super::core::native_output("bray-native-provider-host-")?;

    build_library(
        root,
        target,
        runtime,
        library.path(),
        "shared-library",
        None,
        &[LIFETIME],
    )?;

    let image = shared_library_path(library.path(), target)?;

    let first = host.path().join(
        image
            .path()
            .file_name()
            .expect("shared image has a file name"),
    );

    std::fs::copy(image.path(), &first)
        .map_err(|error| format!("could not copy provider lifetime image: {error}"))?;

    let second = host.path().join(format!(
        "second-{}",
        first
            .file_name()
            .expect("provider copy has a name")
            .to_string_lossy()
    ));

    std::fs::copy(&first, &second)
        .map_err(|error| format!("could not copy independent provider image: {error}"))?;

    let lifetime = compile_fixture(
        root,
        target,
        runtime,
        host.path(),
        "xtask/fixtures/native-execution/provider-lifetime-host.c",
        TargetOutputKind::Executable,
    )?;

    let formation = compile_fixture(
        root,
        target,
        runtime,
        host.path(),
        "xtask/fixtures/native-execution/provider-formation-host.c",
        TargetOutputKind::Executable,
    )?;

    let domain = compile_fixture(
        root,
        target,
        runtime,
        host.path(),
        "xtask/fixtures/native-execution/provider-domain-image.c",
        TargetOutputKind::SharedLibrary,
    )?;

    execute(
        subprocess::Exec::cmd(&lifetime).arg(&first).arg(&second),
        "provider lifetime and denied terminal allocation",
    )?;

    execute(
        subprocess::Exec::cmd(&formation)
            .arg(&first)
            .arg(&second)
            .arg(&domain),
        "provider formation and retirement races",
    )?;

    let archive = super::core::native_output("bray-native-provider-archive-")?;
    let imported = super::core::native_output("bray-native-provider-imported-")?;

    build_library(
        root,
        target,
        runtime,
        archive.path(),
        "static-library",
        None,
        &[LIFETIME],
    )?;

    build_library(
        root,
        target,
        runtime,
        imported.path(),
        "shared-library",
        Some(archive.path()),
        &[IMPORTED],
    )?;

    let image = shared_library_path(imported.path(), target)?;

    execute(
        subprocess::Exec::cmd(&lifetime).arg(image.path()).args([
            "bray_imported_provider_report",
            "bray_imported_provider_abnormal_report",
            "bray_imported_provider_state",
        ]),
        "imported MIR provider ownership",
    )?;

    execute(
        subprocess::Exec::cmd(&lifetime).arg(image.path()).args([
            "bray_archive_provider_report",
            "bray_archive_provider_abnormal_report",
            "bray_imported_provider_state",
        ]),
        "native archive provider ownership",
    )
}

fn execute(command: subprocess::Exec, name: &str) -> Result<(), String> {
    crate::validation_process::success(command, crate::validation_process::TIMEOUT)
        .map(|_| ())
        .map_err(|error| format!("executing {name}: {error}"))
}
