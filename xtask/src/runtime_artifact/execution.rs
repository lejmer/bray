use std::fs;
use std::path::Path;
use subprocess::Exec;

use bray_base::NonEmptySharedStr;
use bray_compilation::{
    BuildConfiguration, CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget,
};
use bray_diagnostics::DiagnosticLlvmToolRole;
use bray_emitter::{ArtifactKind, resolve_published_artifact};
use bray_linker::{LinkSearchPath, LinkSearchPathKind};
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_symbols::{
    NativeLinkKind, NativeLinkRequirement, PackageIdentity, ProductIdentity, ProductKind,
};
use bray_target::{NativeTarget, TargetOutputKind, TargetOutputName};
use bray_tooling::{llvm_tool_path, load_llvm_compilation};

use super::command::{CommandError, Package};
use super::smoke::{command_output, standard_library};

pub(super) fn smoke_test(
    runtime: &Package,
    target: NativeTarget,
    directory: &Path,
) -> Result<(), CommandError> {
    let probe = allocation_probe(target, directory)?;

    for (name, source, expected) in [
        (
            "async-values",
            include_str!("../../fixtures/native-execution/async-i32.bray"),
            42,
        ),
        (
            "ordinary-finalization",
            include_str!("../../fixtures/native-execution/ordinary-finalization.bray"),
            0,
        ),
        (
            "scoped-cleanup",
            include_str!("../../fixtures/native-execution/scoped-cleanup.bray"),
            0,
        ),
        (
            "operation-cleanup",
            include_str!("../../fixtures/native-execution/operation-cleanup.bray"),
            0,
        ),
        (
            "async-tasks",
            include_str!("../../fixtures/native-execution/async-tasks.bray"),
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
            )
            .with_native_link_inputs(
                matches!(
                    name,
                    "ordinary-finalization" | "scoped-cleanup" | "operation-cleanup"
                )
                .then(|| probe.clone()),
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
                [
                    LinkSearchPath::try_new(LinkSearchPathKind::Library, directory)
                        .expect("native smoke directory is a valid link search path"),
                ],
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

            let result = command_output(Exec::cmd(executable.path()), "generated async execution")?;

            let expected_stderr = if name == "scoped-cleanup" {
                let stderr = String::from_utf8_lossy(&result.stderr);
                let mut lines = stderr.lines();

                matches!(
                    (lines.next(), lines.next(), lines.next()),
                    (Some(segment), Some(error), None)
                        if segment.starts_with("cleanup_incident ordinal=0 task=")
                            && error.starts_with("cleanup_error type=")
                )
            } else {
                result.stderr.is_empty()
            };

            if result.status.code() != Some(expected) || !expected_stderr {
                return Err(CommandError::EntrySmoke(crate::command::failure(
                    &format!(
                        "{name} ({}, {mode}, expected exit {expected})",
                        executable.path().display()
                    ),
                    &result,
                )));
            }

            println!("{name} {mode}: exit={expected}");
        }
    }

    Ok(())
}

pub(super) fn allocation_probe(
    target: NativeTarget,
    directory: &Path,
) -> Result<NativeLinkRequirement, CommandError> {
    let name = "bray_allocation_probe";
    let source = directory.join("allocation-probe.c");
    let object = directory.join("allocation-probe.o");

    let archive = directory.join(
        TargetOutputName::for_native(target.object_format(), TargetOutputKind::StaticLibrary)
            .file_name(name)
            .expect("allocation probe library name is valid"),
    );

    fs::write(
        &source,
        include_str!("../../fixtures/runtime-allocation-probe.c"),
    )
    .map_err(|error| CommandError::write(&source, error))?;

    let compiler = llvm_tool_path(DiagnosticLlvmToolRole::CompilerDriver)
        .map_err(CommandError::NativeCompilerToolUnavailable)?;

    crate::native_product::compile_c_fixture(&compiler, &source, &object, &[])
        .map_err(CommandError::EntrySmoke)?;

    let bytes = fs::read(&object).map_err(|error| CommandError::read(&object, error))?;

    let archiver = llvm_tool_path(DiagnosticLlvmToolRole::Archiver)
        .map_err(CommandError::NativeCompilerToolUnavailable)?;

    let bytes = crate::native_archive::archive_bytes(&archiver, &[bytes], "o")
        .map_err(CommandError::RuntimePartitionTool)?;

    fs::write(&archive, bytes).map_err(|error| CommandError::write(&archive, error))?;

    Ok(NativeLinkRequirement::new(
        NonEmptySharedStr::try_new(name).expect("allocation probe link name is valid"),
        NativeLinkKind::Static,
    ))
}
