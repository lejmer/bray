use std::path::Path;
use std::process::Command;

use bray_compilation::{
    BuildConfiguration, CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget,
};
use bray_emitter::{ArtifactKind, resolve_published_artifact};
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
use bray_target::NativeTarget;
use bray_tooling::load_llvm_compilation;

use super::command::{CommandError, Package};
use super::smoke::{command_output, standard_library};

pub(super) fn smoke_test(
    runtime: &Package,
    target: NativeTarget,
    directory: &Path,
) -> Result<(), CommandError> {
    for (name, source, expected) in [
        (
            "async-values",
            include_str!("../../fixtures/native-execution/async-i32.bray"),
            42,
        ),
        (
            "async-tasks",
            include_str!("../../fixtures/native-execution/async_tasks.bray"),
            0,
        ),
    ] {
        let package = PackageIdentity::try_new("bray.runtime.execution.smoke")
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
            ),
        )
        .with_standard_library_root(standard_library(target)?);

        let compilation = load_llvm_compilation(request).map_err(|error| {
            CommandError::EntrySmoke(format!("could not load {name}: {error:?}"))
        })?;

        let diagnostics = compilation.check_diagnostics();

        if !diagnostics.is_empty() {
            return Err(CommandError::EntrySmoke(format!(
                "{name} did not pass compilation checks: {diagnostics:?}"
            )));
        }

        // Development instantiates imported MIR. Release reuses the standard
        // library's native constructors and must retain their companion backing.
        for (mode, configuration) in [
            ("development", BuildConfiguration::Development),
            ("release", BuildConfiguration::Release),
        ] {
            let output = directory.join(format!("{name}-{mode}"));

            crate::native_product::emit_executable_with_configuration(
                &compilation,
                product.clone(),
                target,
                &runtime.metadata,
                &output,
                [],
                None,
                configuration,
            )
            .map_err(CommandError::EntrySmoke)?;

            let destination = crate::native_product::managed_destination(&output)
                .map_err(CommandError::EntrySmoke)?;

            let executable =
                resolve_published_artifact(destination, &product, ArtifactKind::Executable, 0)
                    .map_err(|error| {
                        CommandError::EntrySmoke(format!("could not resolve {name}: {error:?}"))
                    })?;

            let result = command_output(
                &mut Command::new(executable.path()),
                "generated async execution",
            )?;

            if result.status.code() != Some(expected) || !result.stderr.is_empty() {
                return Err(CommandError::EntrySmoke(crate::command::failure(
                    &format!("{name} ({mode}, expected exit {expected})"),
                    &result,
                )));
            }

            println!("{name} {mode}: exit={expected}");
        }
    }

    Ok(())
}
