use std::path::{Path, PathBuf};
use subprocess::Exec;

use bray_compilation::{CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget};
use bray_emitter::{ArtifactKind, resolve_published_artifact};
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
use bray_target::NativeTarget;
use bray_tooling::load_llvm_compilation;

use super::command::{CommandError, Package, RuntimeArchiveKind};
use super::smoke::{
    command_output, compile_c_smoke, component_archives, native_links, standard_library,
};

const PRODUCT_SOURCE: &str = include_str!("../../fixtures/runtime-entry-product.bray");
const DRIVER_SOURCE: &str = include_str!("../../fixtures/runtime-entry-smoke.c");

pub(super) fn smoke_test(
    package: &Package,
    target: NativeTarget,
    directory: &Path,
) -> Result<(), CommandError> {
    let product = emit_product(package, target, directory)?;

    let mut archives = component_archives(
        package,
        &[
            RuntimeArchiveKind::Bootstrap,
            RuntimeArchiveKind::Host,
            RuntimeArchiveKind::Callback,
            RuntimeArchiveKind::Scheduler,
            RuntimeArchiveKind::Cancellation,
        ],
    )?;

    archives.insert(0, &product);

    let native_links = native_links(package).map_err(CommandError::EntrySmoke)?;
    let map = directory.join("runtime-entry-smoke.map");

    let executable = compile_c_smoke(
        DRIVER_SOURCE,
        "runtime-entry-smoke",
        &archives,
        target,
        directory,
        &map,
        &native_links,
    )?;

    let output = command_output(Exec::cmd(&executable), "generated entry")?;

    if !output.status.success() {
        return Err(CommandError::EntrySmoke(crate::command::failure(
            &format!("generated entry ({})", executable.display()),
            &output,
        )));
    }

    if !output.stderr.is_empty() {
        return Err(CommandError::EntrySmoke(format!(
            "generated entry wrote unexpected stderr: {}",
            String::from_utf8_lossy(&output.stderr),
        )));
    }

    report_measurement(&output.stdout)
}

fn report_measurement(output: &[u8]) -> Result<(), CommandError> {
    let text = std::str::from_utf8(output).map_err(|error| {
        CommandError::EntrySmoke(format!("entry measurement is not UTF-8: {error}"))
    })?;

    let line = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'));

    let seconds = line
        .and_then(|line| line.strip_prefix("foreign_entry iterations=100000 clock_seconds="))
        .and_then(|seconds| seconds.parse::<f64>().ok());

    if !seconds.is_some_and(|seconds| seconds.is_finite() && seconds >= 0.0) {
        return Err(CommandError::EntrySmoke(format!(
            "unexpected entry measurement output: {text:?}"
        )));
    }

    println!(
        "{}",
        line.expect("validated measurement has exactly one line")
    );

    Ok(())
}

fn emit_product(
    runtime: &Package,
    target: NativeTarget,
    directory: &Path,
) -> Result<PathBuf, CommandError> {
    let package = PackageIdentity::try_new("bray.runtime.entry.smoke")
        .expect("fixture package identity is valid");

    let product = ProductIdentity::try_new(package.clone(), "library")
        .expect("fixture product identity is valid");

    let source = SourceInput::virtual_text(
        SourceIdentity::new(0),
        "runtime-entry-product.bray",
        SourceVersion::new(0),
        PRODUCT_SOURCE,
    );

    let request = CompilationRequest::with_options(
        package,
        vec![source],
        CompilationOptions::new(
            WorkerBudget::default(),
            ProductKind::Library,
            SelectedTarget::for_native(target),
        )
        .with_native_link_inputs([bray_symbols::NativeLinkRequirement::new(
            bray_base::NonEmptySharedStr::try_new("entry_driver")
                .expect("fixture link name is valid"),
            bray_symbols::NativeLinkKind::Static,
        )]),
    )
    .with_standard_library_root(standard_library(target)?);

    let compilation = load_llvm_compilation(request).map_err(|error| {
        CommandError::EntrySmoke(format!("could not load generated entry product: {error:?}"))
    })?;

    let diagnostics = compilation.check_diagnostics();

    if !diagnostics.is_empty() {
        return Err(CommandError::EntrySmoke(format!(
            "generated entry product did not pass compilation checks: {diagnostics:?}"
        )));
    }

    let output = directory.join("runtime-entry-product");

    crate::native_product::emit_library(
        &compilation,
        product.clone(),
        target,
        &runtime.metadata,
        &output,
    )
    .map_err(CommandError::EntrySmoke)?;

    let destination =
        crate::native_product::managed_destination(&output).map_err(CommandError::EntrySmoke)?;

    let artifact =
        resolve_published_artifact(destination, &product, ArtifactKind::StaticLibrary, 0).map_err(
            |error| {
                CommandError::EntrySmoke(format!(
                    "could not resolve generated entry product: {error:?}"
                ))
            },
        )?;

    Ok(artifact.path().to_path_buf())
}
