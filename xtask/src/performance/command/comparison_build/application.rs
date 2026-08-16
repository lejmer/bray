use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use bray_target::NativeTarget;

use crate::performance::compilation::{
    MATCHED_APPLICATION_CONTRACT, authority, comparison, external_invocation, reuse_evidence,
    source_bytes, source_digest,
};
use crate::performance::model::{
    ArtifactKind, CompilationBuildReport, CompilationEvidenceReport, CompilationKind,
    CompilationLanguage, CompilationReuseEvidence, LibraryReuse, LinkerInvocationReport,
    PeerCompilerConfiguration,
};

use super::evidence::{bray_reuse_evidence, cpp_reuse_role, rust_reuse_role};

const BRAY_APPLICATION_SOURCE: &str =
    include_str!("../../../../fixtures/performance-application.bray");
const RUST_APPLICATION_SOURCE: &str =
    include_str!("../../../../fixtures/performance-application.rs");
const CPP_APPLICATION_SOURCE: &str =
    include_str!("../../../../fixtures/performance-application.cpp");

pub(in crate::performance::command) fn build_application(
    root: &Path,
    compiler: &Path,
    output: &Path,
    target: NativeTarget,
    toolchain: &Path,
    runtime: &Path,
) -> Result<crate::performance::model::CompilationComparisonReport, String> {
    let output = output.join("application-compilation");

    std::fs::create_dir_all(&output)
        .map_err(|error| format!("could not create {}: {error}", output.display()))?;

    let builds = [
        (
            CompilationLanguage::Bray,
            build_bray_application(compiler, &output, target, toolchain, runtime)?,
        ),
        (
            CompilationLanguage::Rust,
            build_rust_application(root, &output, target)?,
        ),
        (
            CompilationLanguage::Cpp,
            build_cpp_application(root, &output, target)?,
        ),
    ]
    .into_iter()
    .collect();

    Ok(comparison(
        CompilationKind::Application,
        MATCHED_APPLICATION_CONTRACT,
        builds,
    ))
}

fn build_bray_application(
    compiler: &Path,
    output: &Path,
    target: NativeTarget,
    toolchain: &Path,
    runtime: &Path,
) -> Result<CompilationBuildReport, String> {
    let output = output.join("bray");

    std::fs::create_dir_all(&output)
        .map_err(|error| format!("could not create {}: {error}", output.display()))?;

    let source = output.join("performance-application.bray");
    let timed_output = output.join("timed");
    let evidence_output = output.join("evidence");
    let profile = output.join("profile.json");
    let linker_map = output.join("application.map");
    let standard_library = toolchain.join("lib/bray/standard-library");

    std::fs::create_dir_all(&timed_output)
        .and_then(|()| std::fs::create_dir_all(&evidence_output))
        .map_err(|error| format!("could not create matched Bray output: {error}"))?;

    std::fs::write(&source, BRAY_APPLICATION_SOURCE)
        .map_err(|error| format!("could not write {}: {error}", source.display()))?;

    let arguments = bray_application_arguments(
        &source,
        &timed_output,
        target,
        &standard_library,
        runtime,
        None,
    );

    let run = super::super::compiler::run_timed(
        compiler,
        arguments.clone(),
        "building matched Bray application",
    )?;

    let evidence_arguments = bray_application_arguments(
        &source,
        &evidence_output,
        target,
        &standard_library,
        runtime,
        Some((&profile, &linker_map)),
    );

    let profile = super::super::compiler::run_profiled(
        compiler,
        &evidence_arguments,
        &profile,
        "collecting matched Bray application evidence",
    )?;

    let product = bray_product("bray.performance.application", "application")?;

    let executable = bray_emitter::resolve_published_artifact(
        &evidence_output,
        &product,
        bray_emitter::ArtifactKind::Executable,
        0,
    )
    .map_err(|error| format!("could not resolve matched Bray application: {error:?}"))?;

    let artifact = crate::performance::retention::inspect(
        ArtifactKind::Executable,
        &executable,
        Some(&linker_map),
    )?;

    let reuse = bray_reuse_evidence(&artifact, target, &standard_library, runtime)?;
    let linker_map = artifact.linker_map.clone();
    let compiler_identity = crate::path::slash_separated(compiler);

    Ok(CompilationBuildReport {
        toolchain: crate::performance::peer::command_identity(
            Command::new(compiler).arg("--version"),
            "brayc",
        )?,
        source_sha256: source_digest(BRAY_APPLICATION_SOURCE),
        elapsed_nanoseconds: run.elapsed_nanoseconds,
        authority: authority(
            1,
            source_bytes(BRAY_APPLICATION_SOURCE)?,
            ["bray.performance.application".to_owned(), "std".to_owned()],
            ["performance_application".to_owned(), "std".to_owned()],
            LibraryReuse::Packaged,
        ),
        compiler: run.invocation,
        linker: LinkerInvocationReport::IntegratedCompilerDriver {
            driver: compiler_identity.clone(),
            arguments,
        },
        evidence: Some(CompilationEvidenceReport {
            compiler: external_invocation(compiler_identity, evidence_arguments, BTreeMap::new()),
            linker_map,
        }),
        reuse,
        profile: Some(profile),
    })
}

fn bray_application_arguments(
    source: &Path,
    output: &Path,
    target: NativeTarget,
    standard_library: &Path,
    runtime: &Path,
    evidence: Option<(&Path, &Path)>,
) -> Vec<String> {
    let mut arguments = Vec::new();

    if let Some((profile, _)) = evidence {
        arguments.extend([
            "--profile".to_owned(),
            "summary".to_owned(),
            "--profile-output".to_owned(),
            crate::path::slash_separated(&profile),
        ]);
    }

    arguments.extend([
        "--standard-library-root".to_owned(),
        crate::path::slash_separated(&standard_library),
        "build".to_owned(),
        "--package".to_owned(),
        "bray.performance.application".to_owned(),
        "--product".to_owned(),
        "application".to_owned(),
        "--product-kind".to_owned(),
        "executable".to_owned(),
        "--target".to_owned(),
        target.as_str().to_owned(),
        "--release".to_owned(),
        "--artifact".to_owned(),
        "executable".to_owned(),
        "--runtime-artifact".to_owned(),
        crate::path::slash_separated(runtime),
    ]);

    if let Some((_, linker_map)) = evidence {
        arguments.extend([
            "--linker-map-output".to_owned(),
            crate::path::slash_separated(linker_map),
        ]);
    }

    arguments.extend([
        "--output".to_owned(),
        crate::path::slash_separated(output),
        crate::path::slash_separated(source),
    ]);

    arguments
}

fn build_rust_application(
    root: &Path,
    output: &Path,
    target: NativeTarget,
) -> Result<CompilationBuildReport, String> {
    let output = output.join("rust");

    std::fs::create_dir_all(&output)
        .map_err(|error| format!("could not create {}: {error}", output.display()))?;

    let source = output.join("performance-application.rs");
    let timed_output = output.join("timed");
    let evidence_output = output.join("evidence");
    let executable = timed_output.join(crate::native_toolchain::executable_name("application"));

    let evidence_executable =
        evidence_output.join(crate::native_toolchain::executable_name("application"));

    let linker_map = output.join("application.map");

    std::fs::create_dir_all(&timed_output)
        .and_then(|()| std::fs::create_dir_all(&evidence_output))
        .map_err(|error| format!("could not create matched Rust output: {error}"))?;

    std::fs::write(&source, RUST_APPLICATION_SOURCE)
        .map_err(|error| format!("could not write {}: {error}", source.display()))?;

    let (_linker, configuration) = crate::performance::peer::matched_rust_configuration(
        root,
        &source,
        &executable,
        target,
    )?;

    let (_linker, evidence_configuration) =
        crate::performance::peer::matched_rust_evidence_configuration(
            root,
            &source,
            &evidence_executable,
            &linker_map,
            target,
        )?;

    let started = Instant::now();

    crate::performance::peer::run_compiler(
        "rustc",
        &configuration,
        "building matched Rust application",
    )?;

    let elapsed_nanoseconds = crate::performance::peer::elapsed_nanoseconds(started);

    crate::performance::peer::run_compiler(
        "rustc",
        &evidence_configuration,
        "collecting matched Rust application evidence",
    )?;

    let artifact = crate::performance::retention::inspect(
        ArtifactKind::Executable,
        &evidence_executable,
        Some(&linker_map),
    )?;

    let reuse = reuse_evidence(std::slice::from_ref(&artifact), rust_reuse_role);
    let linker_map = artifact.linker_map.clone();

    Ok(application_build(
        "rustc",
        crate::performance::peer::command_identity(Command::new("rustc").arg("--version"), "rustc")?,
        RUST_APPLICATION_SOURCE,
        elapsed_nanoseconds,
        ["performance_application".to_owned(), "std".to_owned()],
        ["crate".to_owned(), "std".to_owned()],
        configuration,
        evidence_configuration,
        linker_map,
        reuse,
    )?)
}

fn build_cpp_application(
    root: &Path,
    output: &Path,
    target: NativeTarget,
) -> Result<CompilationBuildReport, String> {
    let output = output.join("cpp");

    std::fs::create_dir_all(&output)
        .map_err(|error| format!("could not create {}: {error}", output.display()))?;

    let source = output.join("performance-application.cpp");
    let timed_output = output.join("timed");
    let evidence_output = output.join("evidence");
    let executable = timed_output.join(crate::native_toolchain::executable_name("application"));

    let evidence_executable =
        evidence_output.join(crate::native_toolchain::executable_name("application"));

    let linker_map = output.join("application.map");

    std::fs::create_dir_all(&timed_output)
        .and_then(|()| std::fs::create_dir_all(&evidence_output))
        .map_err(|error| format!("could not create matched C++ output: {error}"))?;

    std::fs::write(&source, CPP_APPLICATION_SOURCE)
        .map_err(|error| format!("could not write {}: {error}", source.display()))?;

    let clang =
        bray_tooling::llvm_tool_path(bray_diagnostics::DiagnosticLlvmToolRole::CompilerDriver)
            .unwrap_or_else(|_| bray_llvm_toolchain::tool_path(root, "clang"));

    let configuration = crate::performance::peer::matched_cpp_configuration(
        &source,
        &executable,
        target,
    )?;

    let evidence_configuration = crate::performance::peer::matched_cpp_evidence_configuration(
        &source,
        &evidence_executable,
        &linker_map,
        target,
    )?;

    let compiler = crate::path::slash_separated(&clang);
    let started = Instant::now();

    crate::performance::peer::run_compiler(
        &compiler,
        &configuration,
        "building matched C++ application",
    )?;

    let elapsed_nanoseconds = crate::performance::peer::elapsed_nanoseconds(started);

    crate::performance::peer::run_compiler(
        &compiler,
        &evidence_configuration,
        "collecting matched C++ application evidence",
    )?;

    let artifact = crate::performance::retention::inspect(
        ArtifactKind::Executable,
        &evidence_executable,
        Some(&linker_map),
    )?;

    let reuse = reuse_evidence(std::slice::from_ref(&artifact), cpp_reuse_role);
    let linker_map = artifact.linker_map.clone();

    Ok(application_build(
        &compiler,
        crate::performance::peer::command_identity(
            Command::new(&clang).arg("--version"),
            "clang",
        )?,
        CPP_APPLICATION_SOURCE,
        elapsed_nanoseconds,
        ["cxx_runtime".to_owned(), "performance_application".to_owned()],
        ["cxx_runtime".to_owned(), "translation_unit".to_owned()],
        configuration,
        evidence_configuration,
        linker_map,
        reuse,
    )?)
}

fn application_build(
    compiler: &str,
    toolchain: String,
    source: &str,
    elapsed_nanoseconds: u64,
    packages: impl IntoIterator<Item = String>,
    modules: impl IntoIterator<Item = String>,
    configuration: PeerCompilerConfiguration,
    evidence_configuration: PeerCompilerConfiguration,
    linker_map: Option<crate::performance::model::LinkerMapReport>,
    reuse: CompilationReuseEvidence,
) -> Result<CompilationBuildReport, String> {
    let arguments = configuration.arguments;

    Ok(CompilationBuildReport {
        toolchain,
        source_sha256: source_digest(source),
        elapsed_nanoseconds,
        authority: authority(
            1,
            source_bytes(source)?,
            packages,
            modules,
            LibraryReuse::Packaged,
        ),
        compiler: external_invocation(
            compiler,
            arguments.clone(),
            configuration.environment,
        ),
        linker: LinkerInvocationReport::IntegratedCompilerDriver {
            driver: compiler.to_owned(),
            arguments,
        },
        evidence: Some(CompilationEvidenceReport {
            compiler: external_invocation(
                compiler,
                evidence_configuration.arguments,
                evidence_configuration.environment,
            ),
            linker_map,
        }),
        reuse,
        profile: None,
    })
}

fn bray_product(package: &str, product: &str) -> Result<bray_symbols::ProductIdentity, String> {
    let package = bray_symbols::PackageIdentity::try_new(package)
        .ok_or_else(|| "matched Bray package identity is invalid".to_owned())?;

    bray_symbols::ProductIdentity::try_new(package, product)
        .ok_or_else(|| "matched Bray product identity is invalid".to_owned())
}
