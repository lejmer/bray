use std::fs;
use std::path::Path;
use std::time::Instant;

use bray_compilation::{
    BuildConfiguration, CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget,
};
use bray_emitter::{ArtifactKind, resolve_published_artifact};
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
use bray_target::NativeTarget;
use subprocess::Exec;

use super::command::{CommandError, Package};
use super::smoke::{command_output, standard_library};

/// Exercise ordinary compiler emission, including source and native standard-library reuse.
pub(super) fn smoke_test(
    runtime: &Package,
    target: NativeTarget,
    directory: &Path,
) -> Result<(), CommandError> {
    let mut release_sizes = Vec::new();

    let (bray_units, runtime_units) = super::provenance::load_runtime_provenance(runtime, target)?;

    let probe = super::execution::allocation_probe(target, directory)?;

    for (name, source, expected, stdout, stderr) in [
        (
            "direct",
            include_str!("../../fixtures/native-execution/sync-direct.bray"),
            0,
            "",
            "",
        ),
        (
            "empty",
            include_str!("../../fixtures/native-execution/sync-empty.bray"),
            0,
            "",
            "",
        ),
        (
            "output",
            include_str!("../../fixtures/native-execution/sync-output.bray"),
            0,
            "sync output\n",
            "",
        ),
        (
            "layout",
            include_str!("../../fixtures/native-execution/sync-layout.bray"),
            0,
            "",
            "",
        ),
        (
            "numeric",
            include_str!("../../fixtures/native-execution/sync-numeric.bray"),
            0,
            "",
            "",
        ),
        (
            "panic",
            include_str!("../../fixtures/native-execution/sync-panic.bray"),
            1,
            "",
            "panic",
        ),
        (
            "static",
            include_str!("../../fixtures/native-execution/sync-static.bray"),
            0,
            "static cleanup\n",
            "",
        ),
        (
            "thread-static",
            include_str!("../../fixtures/native-execution/thread-static-cleanup.bray"),
            42,
            "",
            "",
        ),
        (
            "entry-error",
            include_str!("../../fixtures/native-execution/sync-result-cleanup.bray"),
            1,
            "",
            "entry returned an error",
        ),
        (
            "cleanup-error",
            include_str!("../../fixtures/native-execution/static-cleanup-failure.bray"),
            1,
            "",
            "cleanup_error",
        ),
        (
            "denied-cleanup",
            include_str!("../../fixtures/native-execution/sync-denied-cleanup.bray"),
            1,
            "",
            "panic\npanic\npanic\npanic\n",
        ),
    ] {
        let preparing = Instant::now();

        let package = PackageIdentity::try_new("bray.runtime.synchronous.smoke")
            .expect("fixture package identity is valid");

        let product = ProductIdentity::try_new(package.clone(), name)
            .expect("fixture product identity is valid");

        let request = CompilationRequest::with_options(
            package,
            vec![SourceInput::virtual_text(
                SourceIdentity::new(0),
                name,
                SourceVersion::new(0),
                source,
            )],
            CompilationOptions::new(
                WorkerBudget::default(),
                ProductKind::Executable,
                SelectedTarget::for_native(target),
            )
            .with_native_link_inputs((name == "denied-cleanup").then(|| probe.clone())),
        )
        .with_standard_library_root(standard_library(target)?);

        let compilation = bray_tooling::load_llvm_compilation(request).map_err(|error| {
            CommandError::EntrySmoke(format!("could not load {name}: {error:?}"))
        })?;

        let preparation_seconds = preparing.elapsed().as_secs_f64();

        for (mode, configuration) in [
            ("development", BuildConfiguration::Development),
            ("release", BuildConfiguration::Release),
        ] {
            let output = directory.join(format!("synchronous-{name}-{mode}"));
            let map = output.with_extension("map");
            let started = Instant::now();

            let native = crate::native_product::emit_executable_with_configuration(
                &compilation,
                product.clone(),
                target,
                &runtime.metadata,
                &output,
                [bray_linker::LinkSearchPath::try_new(
                    bray_linker::LinkSearchPathKind::Library,
                    directory,
                )
                .expect("fixture directory is a valid search path")],
                bray_linker::SystemLinkerMapOutput::try_new(&map),
                configuration,
            )
            .map_err(CommandError::EntrySmoke)?;

            let build_seconds = started.elapsed().as_secs_f64();

            audit_synchronous_product(
                &native,
                &bray_units,
                &output,
            )
            .map_err(CommandError::EntrySmoke)?;

            super::provenance::audit_runtime_units(&runtime_units, &bray_units, target, &map)?;

            let destination = crate::native_product::managed_destination(&output)
                .map_err(CommandError::EntrySmoke)?;

            let executable =
                resolve_published_artifact(destination, &product, ArtifactKind::Executable, 0)
                    .map_err(|error| {
                        CommandError::EntrySmoke(format!("could not resolve {name}: {error:?}"))
                    })?;

            let started = Instant::now();
            let result = command_output(Exec::cmd(executable.path()), "synchronous product")?;
            let startup_seconds = started.elapsed().as_secs_f64();
            let actual_stdout = String::from_utf8_lossy(&result.stdout).replace("\r\n", "\n");
            let actual_stderr = String::from_utf8_lossy(&result.stderr);

            if result.status.code() != Some(expected)
                || actual_stdout != stdout
                || (stderr.is_empty() && !actual_stderr.is_empty())
                || (!stderr.is_empty() && !actual_stderr.contains(stderr))
                || (name == "denied-cleanup" && actual_stderr != stderr)
            {
                return Err(CommandError::EntrySmoke(crate::command::failure(
                    &format!("synchronous {name} {mode}"),
                    &result,
                )));
            }

            let bytes = fs::metadata(executable.path())
                .map_err(|error| CommandError::read(executable.path(), error))?
                .len();

            if configuration == BuildConfiguration::Release {
                release_sizes.push((name, bytes));
            }

            let whole_build_seconds = preparation_seconds + build_seconds;

            println!(
                "synchronous_product name={name} mode={mode} bytes={bytes} preparation_seconds={preparation_seconds:.6} build_seconds={build_seconds:.6} whole_build_seconds={whole_build_seconds:.6} startup_seconds={startup_seconds:.6}"
            );
        }
    }

    assert!(
        release_sizes[0].1 < release_sizes[1].1,
        "direct product must avoid the hosted synchronous closure"
    );

    Ok(())
}

/// Prove the ordinary emitter's selected synchronous runtime before executing its product.
fn audit_synchronous_product(
    native: &bray_compilation::NativeProductPlan,
    bray_units: &std::collections::BTreeSet<bray_native_artifact::NativeContentDigest>,
    output: &Path,
) -> Result<(), String> {
    let map = output.with_extension("map");
    let symbols = fs::read_to_string(&map).map_err(|error| format!("could not read synchronous link map: {error}"))?;

    super::smoke::audit_link_symbols(
        &symbols,
        &[],
        &[
            bray_runtime_abi::symbols::DEADLINE_CREATION_SYMBOL,
            bray_runtime_abi::symbols::DEADLINE_DISPATCH_SYMBOL,
            bray_runtime_interface::PlatformServiceRole::ClockMonotonicNow.native_symbol(),
        ],
    )?;

    let Some(host) = native.executable_host() else {
        return Err("synchronous executable must retain its entry contract".to_owned());
    };

    if host
        .requirements()
        .capabilities()
        .contains(&bray_runtime_interface::RuntimeCapability::CooperativeExecution)
        || host
            .entries()
            .iter()
            .any(|entry| entry.root() != bray_runtime_interface::RootExecution::Synchronous)
    {
        return Err("synchronous product acquired asynchronous execution demand".to_owned());
    }

    super::provenance::audit_product_runtime(native, bray_units, output)
}
