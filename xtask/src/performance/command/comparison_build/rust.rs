use std::path::Path;
use std::process::Command;
use std::time::Instant;

use bray_target::NativeTarget;

use crate::performance::compilation::{empty_reuse_evidence, reuse_evidence};
use crate::performance::model::{ArtifactKind, CompilationBuildReport, CompilationKind};

use super::shared::PeerReportInput;

const APPLICATION_SOURCE: &str =
    include_str!("../../../../fixtures/performance-application.rs");
const LIBRARY_SOURCE: &str = include_str!("../../../../fixtures/performance-library.rs");

pub(super) fn build(
    root: &Path,
    output: &Path,
    kind: CompilationKind,
    target: NativeTarget,
) -> Result<CompilationBuildReport, String> {
    let output = output.join("rust");

    std::fs::create_dir_all(&output)
        .map_err(|error| format!("could not create {}: {error}", output.display()))?;

    let (source_contents, source_name, packages, modules) = match kind {
        CompilationKind::Application => (
            APPLICATION_SOURCE,
            "performance-application.rs",
            vec!["performance_application".to_owned(), "std".to_owned()],
            vec!["crate".to_owned(), "std".to_owned()],
        ),
        CompilationKind::Library => (
            LIBRARY_SOURCE,
            "performance-library.rs",
            vec!["performance_library".to_owned()],
            vec!["performance_library".to_owned()],
        ),
    };

    let source = output.join(source_name);

    std::fs::write(&source, source_contents)
        .map_err(|error| format!("could not write {}: {error}", source.display()))?;

    let (elapsed_nanoseconds, configuration, evidence, linker_map, reuse) = match kind {
        CompilationKind::Application => {
            let timed_output = output.join("timed");
            let evidence_output = output.join("evidence");

            let executable =
                timed_output.join(crate::native_toolchain::executable_name("application"));

            let evidence_executable =
                evidence_output.join(crate::native_toolchain::executable_name("application"));

            let linker_map_path = output.join("application.map");

            std::fs::create_dir_all(&timed_output)
                .and_then(|()| std::fs::create_dir_all(&evidence_output))
                .map_err(|error| format!("could not create matched Rust output: {error}"))?;

            let timed_configuration = build_configuration(
                root,
                kind,
                &source,
                &executable,
                None,
                target,
            )?;

            let evidence = build_configuration(
                root,
                kind,
                &source,
                &evidence_executable,
                Some(&linker_map_path),
                target,
            )?;

            let started = Instant::now();

            crate::performance::peer::run_compiler(
                "rustc",
                &timed_configuration,
                "building matched Rust application",
            )?;

            let elapsed_nanoseconds = crate::performance::peer::elapsed_nanoseconds(started);

            crate::performance::peer::run_compiler(
                "rustc",
                &evidence,
                "collecting matched Rust application evidence",
            )?;

            let artifact = crate::performance::retention::inspect(
                ArtifactKind::Executable,
                &evidence_executable,
                Some(&linker_map_path),
            )?;

            let reuse = reuse_evidence(
                std::slice::from_ref(&artifact),
                super::evidence::rust_reuse_role,
            );

            (
                elapsed_nanoseconds,
                timed_configuration,
                Some(evidence),
                artifact.linker_map,
                reuse,
            )
        }
        CompilationKind::Library => {
            let object = output.join(super::shared::object_name(target));
            let configuration = build_configuration(root, kind, &source, &object, None, target)?;
            let started = Instant::now();

            crate::performance::peer::run_compiler(
                "rustc",
                &configuration,
                "building matched Rust library",
            )?;

            let elapsed_nanoseconds = crate::performance::peer::elapsed_nanoseconds(started);

            super::shared::require_object(&object)?;

            (
                elapsed_nanoseconds,
                configuration,
                None,
                None,
                empty_reuse_evidence(),
            )
        }
    };

    super::shared::peer_report(PeerReportInput {
        compiler: "rustc",
        toolchain: crate::performance::peer::command_identity(
            Command::new("rustc").arg("--version"),
            "rustc",
        )?,
        source: source_contents,
        kind,
        elapsed_nanoseconds,
        packages,
        modules,
        configuration,
        evidence,
        linker_map,
        reuse,
    })
}

fn build_configuration(
    root: &Path,
    kind: CompilationKind,
    source: &Path,
    output: &Path,
    linker_map: Option<&Path>,
    target: NativeTarget,
) -> Result<crate::performance::model::PeerCompilerConfiguration, String> {
    let mut arguments = match kind {
        CompilationKind::Application => {
            let linker = crate::performance::peer::rust_linker(root, target);

            crate::performance::peer::rust_executable_arguments(source, target, &linker)
        }
        CompilationKind::Library => {
            let mut arguments = crate::performance::peer::rust_release_arguments(source, target);

            arguments.extend([
                "--crate-name".to_owned(),
                "performance_library".to_owned(),
                "--crate-type".to_owned(),
                "lib".to_owned(),
                "--emit".to_owned(),
                "obj".to_owned(),
            ]);

            arguments
        }
    };

    if let Some(linker_map) = linker_map {
        crate::performance::peer::append_rust_linker_map(
            &mut arguments,
            target.object_format(),
            linker_map,
        )?;
    }

    arguments.extend(["-o".to_owned(), crate::path::slash_separated(output)]);

    Ok(super::shared::compiler_configuration(arguments))
}
