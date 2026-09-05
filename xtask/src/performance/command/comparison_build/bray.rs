use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use bray_target::NativeTarget;

use crate::performance::compilation::{
    authority, empty_reuse_evidence, external_invocation, source_bytes, source_digest,
};
use crate::performance::model::{
    ArtifactKind, CompilationBuildReport, CompilationEvidenceReport, CompilationKind, LibraryReuse,
    LinkerInvocationReport,
};

const APPLICATION_SOURCE: &str = include_str!("../../../../fixtures/performance-application.bray");
const LIBRARY_SOURCE: &str = include_str!("../../../../fixtures/performance-library.bray");

pub(super) fn build(
    compiler: &Path,
    output: &Path,
    kind: CompilationKind,
    target: NativeTarget,
    standard_library: &Path,
    runtime: &Path,
) -> Result<CompilationBuildReport, String> {
    let output = output.join("bray");

    std::fs::create_dir_all(&output)
        .map_err(|error| format!("could not create {}: {error}", output.display()))?;

    let (source_contents, source_name, package, product_name, packages, modules) = match kind {
        CompilationKind::Application => (
            APPLICATION_SOURCE,
            "performance-application.bray",
            "bray.performance.application",
            "application",
            vec!["bray.performance.application".to_owned(), "std".to_owned()],
            vec!["performance_application".to_owned(), "std".to_owned()],
        ),
        CompilationKind::Library => (
            LIBRARY_SOURCE,
            "performance-library.bray",
            "bray.performance.library",
            "library",
            vec!["bray.performance.library".to_owned()],
            vec!["performance_library".to_owned()],
        ),
    };

    let source = output.join(source_name);
    let timed_output = output.join("timed");
    let evidence_output = output.join("evidence");
    let profile_path = output.join("profile.json");
    let linker_map_path = output.join("application.map");

    std::fs::create_dir_all(&timed_output)
        .and_then(|()| std::fs::create_dir_all(&evidence_output))
        .map_err(|error| format!("could not create matched Bray output: {error}"))?;

    std::fs::write(&source, source_contents)
        .map_err(|error| format!("could not write {}: {error}", source.display()))?;

    let timed_arguments = arguments(
        kind,
        package,
        product_name,
        &source,
        &timed_output,
        target,
        standard_library,
        runtime,
        None,
        None,
    );

    let description = match kind {
        CompilationKind::Application => "building matched Bray application",
        CompilationKind::Library => "building matched Bray library",
    };

    let run = super::super::compiler::run_timed(compiler, timed_arguments, description)?;

    let evidence_arguments = arguments(
        kind,
        package,
        product_name,
        &source,
        &evidence_output,
        target,
        standard_library,
        runtime,
        Some(&profile_path),
        (kind == CompilationKind::Application).then_some(linker_map_path.as_path()),
    );

    let evidence_description = match kind {
        CompilationKind::Application => "collecting matched Bray application evidence",
        CompilationKind::Library => "collecting matched Bray library evidence",
    };

    let profile = super::super::compiler::run_profiled(
        compiler,
        &evidence_arguments,
        &profile_path,
        evidence_description,
    )?;

    let (reuse, linker_map) = match kind {
        CompilationKind::Application => {
            let product = product_identity(package, product_name)?;

            let executable = bray_emitter::resolve_published_artifact(
                &evidence_output,
                &product,
                bray_emitter::ArtifactKind::Executable,
                0,
            )
            .map_err(|error| format!("could not resolve matched Bray application: {error:?}"))?;

            let artifact = crate::performance::retention::inspect_physical(
                ArtifactKind::Executable,
                executable.path(),
                &linker_map_path,
            )?;

            let reuse =
                super::evidence::bray_reuse_evidence(&artifact, target, standard_library, runtime)?;

            (reuse, artifact.linker_map)
        }
        CompilationKind::Library => {
            super::shared::require_object_in_directory(&timed_output, target)?;
            super::shared::require_object_in_directory(&evidence_output, target)?;

            (empty_reuse_evidence(), None)
        }
    };

    let compiler_identity = crate::path::slash_separated(compiler);

    let linker = match kind {
        CompilationKind::Application => LinkerInvocationReport::IntegratedCompilerDriver {
            driver: compiler_identity.clone(),
            arguments: run.invocation.arguments.clone(),
        },
        CompilationKind::Library => LinkerInvocationReport::NotApplicable,
    };

    Ok(CompilationBuildReport {
        toolchain: crate::performance::peer::command_identity(
            Command::new(compiler).arg("--version"),
            "brayc",
        )?,
        source_sha256: source_digest(source_contents),
        elapsed_nanoseconds: run.elapsed_nanoseconds,
        authority: authority(
            1,
            source_bytes(source_contents)?,
            packages,
            modules,
            match kind {
                CompilationKind::Application => LibraryReuse::Packaged,
                CompilationKind::Library => LibraryReuse::Source,
            },
        ),
        compiler: run.invocation,
        linker,
        evidence: Some(CompilationEvidenceReport {
            compiler: external_invocation(compiler_identity, evidence_arguments, BTreeMap::new()),
            linker_map,
        }),
        reuse,
        profile: Some(profile),
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are the exact compiler inputs whose equivalence is measured"
)]
fn arguments(
    kind: CompilationKind,
    package: &str,
    product: &str,
    source: &Path,
    output: &Path,
    target: NativeTarget,
    standard_library: &Path,
    runtime: &Path,
    profile: Option<&Path>,
    linker_map: Option<&Path>,
) -> Vec<String> {
    match kind {
        CompilationKind::Application => super::super::compiler::application_arguments(
            package,
            product,
            source,
            output,
            target,
            standard_library,
            runtime,
            profile,
            linker_map,
        ),
        CompilationKind::Library => super::super::compiler::library_arguments(
            package, product, source, output, target, profile,
        ),
    }
}

fn product_identity(package: &str, product: &str) -> Result<bray_symbols::ProductIdentity, String> {
    let package = bray_symbols::PackageIdentity::try_new(package)
        .ok_or_else(|| "matched Bray package identity is invalid".to_owned())?;

    bray_symbols::ProductIdentity::try_new(package, product)
        .ok_or_else(|| "matched Bray product identity is invalid".to_owned())
}
