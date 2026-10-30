use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::Instant;

use bray_compilation::{
    BuildConfiguration, CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget,
};
use bray_emitter::{ArtifactKind, resolve_published_artifact};
use bray_native_artifact::{NativeContentDigest, NativeUnit, NativeUnitKind};
use bray_package_interface::PackageArtifactInput;
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
use bray_target::NativeTarget;
use subprocess::Exec;

use super::bootstrap::BrayRuntimeComponent;
use super::command::{CommandError, Package};
use super::smoke::{command_output, standard_library};

/// Exercise ordinary compiler emission, including source and native standard-library reuse.
pub(super) fn smoke_test(
    runtime: &Package,
    target: NativeTarget,
    directory: &Path,
) -> Result<(), CommandError> {
    let mut release_sizes = Vec::new();
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
                target,
                runtime
                    .metadata
                    .parent()
                    .expect("runtime bundle has a directory"),
                &output,
            )
            .map_err(CommandError::EntrySmoke)?;

            audit_runtime_units(runtime, target, &map)?;

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

            println!(
                "synchronous_product name={name} mode={mode} bytes={bytes} build_seconds={build_seconds:.6} startup_seconds={startup_seconds:.6}"
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
    target: NativeTarget,
    directory: &Path,
    output: &Path,
) -> Result<(), String> {
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

    let Some(runtime) = native
        .link()
        .and_then(bray_emitter::ProductLinkInputs::runtime)
    else {
        return Ok(());
    };

    let bray_units = bray_unit_digests(directory, target)?;
    let mut receipt = String::new();

    for unit in runtime.native_units() {
        if unit.kind() == NativeUnitKind::OpaqueArchive || !bray_units.contains(&unit.digest()) {
            return Err(format!(
                "synchronous product selected non-Bray runtime unit {}",
                unit.digest()
            ));
        }

        use std::fmt::Write;

        writeln!(
            receipt,
            "{} {:?} {:?}",
            unit.digest(),
            unit.kind(),
            unit.inclusion(),
        )
        .expect("writing runtime inclusion receipt to a string must succeed");
    }

    fs::write(output.with_extension("runtime-units"), receipt)
        .map_err(|error| format!("could not write runtime inclusion receipt: {error}"))?;

    Ok(())
}

fn bray_unit_digests(
    output: &Path,
    target: NativeTarget,
) -> Result<BTreeSet<NativeContentDigest>, String> {
    let mut units = BTreeSet::new();

    for component in BrayRuntimeComponent::ALL {
        let implementation =
            PackageArtifactInput::file(component.implementation_path(output, target), None)
                .load_implementation()
                .map_err(|error| format!("runtime provenance could not load: {error:?}"))?;

        let index = implementation
            .native_artifact()
            .expect("producer implementation must authenticate")
            .expect("producer implementation must carry native units");

        units.extend(
            index
                .units()
                .iter()
                .filter(|unit| unit.kind() != NativeUnitKind::OpaqueArchive)
                .map(|unit| unit.digest()),
        );
    }

    Ok(units)
}

fn audit_runtime_units(
    runtime: &Package,
    target: NativeTarget,
    map: &Path,
) -> Result<(), CommandError> {
    let output = runtime
        .metadata
        .parent()
        .expect("runtime package has an artifact directory");

    let bray_units = bray_unit_digests(output, target).map_err(CommandError::EntrySmoke)?;

    let artifact = bray_tooling::load_runtime_artifact(
        &runtime.metadata,
        &target.identity(),
        SelectedTarget::for_native(target).runtime_abi(),
    )
    .map_err(|error| {
        CommandError::EntrySmoke(format!(
            "runtime provenance could not authenticate: {error:?}"
        ))
    })?;

    let contents = fs::read_to_string(map).map_err(|error| CommandError::read(map, error))?;

    let retained = retained_runtime_units(
        artifact.native_indexes()[0].index().units(),
        &bray_units,
        target,
        &contents,
    )
    .map_err(CommandError::SynchronousLinkMapBoundary)?;

    fs::write(map.with_extension("runtime-members"), retained.join("\n"))
        .map_err(|error| CommandError::write(map, error))?;

    Ok(())
}

fn retained_runtime_units(
    units: &[NativeUnit],
    bray_units: &BTreeSet<NativeContentDigest>,
    target: NativeTarget,
    contents: &str,
) -> Result<Vec<String>, String> {
    let mut retained = Vec::new();

    // Every bundled runtime input is content addressed. Compare complete member paths
    // with authenticated producer identities, including archives with opaque member names.
    for unit in units {
        let name = unit.kind().file_name(unit.digest(), target);

        if contents.contains(&name) {
            if unit.kind() == NativeUnitKind::OpaqueArchive || !bray_units.contains(&unit.digest())
            {
                return Err(format!("retained non-Bray runtime unit {name}"));
            }

            retained.push(name);
        }
    }

    Ok(retained)
}

#[cfg(test)]
mod tests {
    use super::retained_runtime_units;
    use bray_native_artifact::{NativeContentDigest, NativeUnit};
    use bray_native_artifact::{NativeUnitKind, NativeUnitSummary};
    use bray_target::NativeTarget;
    use std::collections::BTreeSet;

    #[test]
    fn synchronous_provenance_rejects_opaque_members_without_rust_symbol_names() {
        let digest = NativeContentDigest::new([7; 32]);
        let target = NativeTarget::X86_64LinuxGnu;

        let unit = NativeUnit::new(
            digest,
            NativeUnitKind::OpaqueArchive,
            NativeUnitSummary::opaque([]),
            [],
        );

        let name = unit.kind().file_name(digest, target);
        let map = format!("{name}(0001.o):(.text.renamed)");

        assert_eq!(
            retained_runtime_units(&[unit.clone()], &BTreeSet::new(), target, &map),
            Err(format!("retained non-Bray runtime unit {name}"))
        );

        assert_eq!(
            retained_runtime_units(&[unit.clone()], &BTreeSet::from([digest]), target, &map),
            Err(format!("retained non-Bray runtime unit {name}"))
        );

        assert_eq!(
            retained_runtime_units(
                &[unit],
                &BTreeSet::new(),
                target,
                "generated.o:(.text.main)"
            ),
            Ok(Vec::new())
        );
    }
}
