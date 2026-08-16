use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use bray_target::{NativeTarget, ObjectFormat};

use crate::performance::compilation::{
    MATCHED_LIBRARY_CONTRACT, authority, comparison, empty_reuse_evidence,
    external_invocation, source_bytes, source_digest,
};
use crate::performance::model::{
    CompilationBuildReport, CompilationEvidenceReport, CompilationKind, CompilationLanguage,
    LibraryReuse, LinkerInvocationReport, PeerBatching, PeerCompilerConfiguration,
};

const BRAY_LIBRARY_SOURCE: &str =
    include_str!("../../../../fixtures/performance-library.bray");
const RUST_LIBRARY_SOURCE: &str =
    include_str!("../../../../fixtures/performance-library.rs");
const CPP_LIBRARY_SOURCE: &str =
    include_str!("../../../../fixtures/performance-library.cpp");

pub(in crate::performance::command) fn build_library(
    root: &Path,
    compiler: &Path,
    output: &Path,
    target: NativeTarget,
) -> Result<crate::performance::model::CompilationComparisonReport, String> {
    let output = output.join("library-compilation");

    std::fs::create_dir_all(&output)
        .map_err(|error| format!("could not create {}: {error}", output.display()))?;

    let builds = [
        (
            CompilationLanguage::Bray,
            build_bray_library(compiler, &output, target)?,
        ),
        (
            CompilationLanguage::Rust,
            build_rust_library(&output, target)?,
        ),
        (
            CompilationLanguage::Cpp,
            build_cpp_library(root, &output, target)?,
        ),
    ]
    .into_iter()
    .collect();

    Ok(comparison(
        CompilationKind::Library,
        MATCHED_LIBRARY_CONTRACT,
        builds,
    ))
}

fn build_bray_library(
    compiler: &Path,
    output: &Path,
    target: NativeTarget,
) -> Result<CompilationBuildReport, String> {
    let output = output.join("bray");

    std::fs::create_dir_all(&output)
        .map_err(|error| format!("could not create {}: {error}", output.display()))?;

    let source = output.join("performance-library.bray");
    let timed_output = output.join("timed");
    let evidence_output = output.join("evidence");
    let profile = output.join("profile.json");

    std::fs::create_dir_all(&timed_output)
        .and_then(|()| std::fs::create_dir_all(&evidence_output))
        .map_err(|error| format!("could not create matched Bray output: {error}"))?;

    std::fs::write(&source, BRAY_LIBRARY_SOURCE)
        .map_err(|error| format!("could not write {}: {error}", source.display()))?;

    let arguments = bray_library_arguments(&source, &timed_output, target, None);

    let run = super::super::compiler::run_timed(
        compiler,
        arguments,
        "building matched Bray library",
    )?;

    let evidence_arguments =
        bray_library_arguments(&source, &evidence_output, target, Some(&profile));

    let profile = super::super::compiler::run_profiled(
        compiler,
        &evidence_arguments,
        &profile,
        "collecting matched Bray library evidence",
    )?;

    require_object_in_directory(&timed_output, target)?;
    require_object_in_directory(&evidence_output, target)?;

    Ok(CompilationBuildReport {
        toolchain: crate::performance::peer::command_identity(
            Command::new(compiler).arg("--version"),
            "brayc",
        )?,
        source_sha256: source_digest(BRAY_LIBRARY_SOURCE),
        elapsed_nanoseconds: run.elapsed_nanoseconds,
        authority: authority(
            1,
            source_bytes(BRAY_LIBRARY_SOURCE)?,
            ["bray.performance.library".to_owned()],
            ["performance_library".to_owned()],
            LibraryReuse::Source,
        ),
        compiler: run.invocation,
        linker: LinkerInvocationReport::NotApplicable,
        evidence: Some(CompilationEvidenceReport {
            compiler: external_invocation(
                crate::path::slash_separated(compiler),
                evidence_arguments,
                BTreeMap::new(),
            ),
            linker_map: None,
        }),
        reuse: empty_reuse_evidence(),
        profile: Some(profile),
    })
}

fn bray_library_arguments(
    source: &Path,
    output: &Path,
    target: NativeTarget,
    profile: Option<&Path>,
) -> Vec<String> {
    let mut arguments = Vec::new();

    if let Some(profile) = profile {
        arguments.extend([
            "--profile".to_owned(),
            "summary".to_owned(),
            "--profile-output".to_owned(),
            crate::path::slash_separated(&profile),
        ]);
    }

    arguments.extend([
        "build".to_owned(),
        "--package".to_owned(),
        "bray.performance.library".to_owned(),
        "--product".to_owned(),
        "library".to_owned(),
        "--product-kind".to_owned(),
        "library".to_owned(),
        "--target".to_owned(),
        target.as_str().to_owned(),
        "--release".to_owned(),
        "--artifact".to_owned(),
        "relocatable-object".to_owned(),
        "--output".to_owned(),
        crate::path::slash_separated(output),
        crate::path::slash_separated(source),
    ]);

    arguments
}

fn build_rust_library(
    output: &Path,
    target: NativeTarget,
) -> Result<CompilationBuildReport, String> {
    let output = output.join("rust");

    std::fs::create_dir_all(&output)
        .map_err(|error| format!("could not create {}: {error}", output.display()))?;

    let source = output.join("performance-library.rs");
    let object = output.join(object_name(target));

    std::fs::write(&source, RUST_LIBRARY_SOURCE)
        .map_err(|error| format!("could not write {}: {error}", source.display()))?;

    let arguments = vec![
        crate::path::slash_separated(&source),
        "--edition".to_owned(),
        "2024".to_owned(),
        "--crate-name".to_owned(),
        "performance_library".to_owned(),
        "--crate-type".to_owned(),
        "lib".to_owned(),
        "--emit".to_owned(),
        "obj".to_owned(),
        "--target".to_owned(),
        target.as_str().to_owned(),
        "-C".to_owned(),
        "opt-level=3".to_owned(),
        "-C".to_owned(),
        "debuginfo=0".to_owned(),
        "-C".to_owned(),
        "panic=abort".to_owned(),
        "-C".to_owned(),
        "codegen-units=1".to_owned(),
        "-C".to_owned(),
        "lto=off".to_owned(),
        "-o".to_owned(),
        crate::path::slash_separated(&object),
    ];

    let configuration = compiler_configuration(arguments.clone());
    let started = Instant::now();

    crate::performance::peer::run_compiler(
        "rustc",
        &configuration,
        "building matched Rust library",
    )?;

    require_object(&object)?;

    Ok(CompilationBuildReport {
        toolchain: crate::performance::peer::command_identity(
            Command::new("rustc").arg("--version"),
            "rustc",
        )?,
        source_sha256: source_digest(RUST_LIBRARY_SOURCE),
        elapsed_nanoseconds: crate::performance::peer::elapsed_nanoseconds(started),
        authority: library_authority(RUST_LIBRARY_SOURCE)?,
        compiler: external_invocation(
            "rustc",
            arguments,
            BTreeMap::new(),
        ),
        linker: LinkerInvocationReport::NotApplicable,
        evidence: None,
        reuse: empty_reuse_evidence(),
        profile: None,
    })
}

fn build_cpp_library(
    root: &Path,
    output: &Path,
    target: NativeTarget,
) -> Result<CompilationBuildReport, String> {
    let output = output.join("cpp");

    std::fs::create_dir_all(&output)
        .map_err(|error| format!("could not create {}: {error}", output.display()))?;

    let source = output.join("performance-library.cpp");
    let object = output.join(object_name(target));

    std::fs::write(&source, CPP_LIBRARY_SOURCE)
        .map_err(|error| format!("could not write {}: {error}", source.display()))?;

    let clang =
        bray_tooling::llvm_tool_path(bray_diagnostics::DiagnosticLlvmToolRole::CompilerDriver)
            .unwrap_or_else(|_| bray_llvm_toolchain::tool_path(root, "clang"));

    let arguments = vec![
        "--driver-mode=g++".to_owned(),
        "-std=c++20".to_owned(),
        "-O3".to_owned(),
        "-DNDEBUG".to_owned(),
        "-fno-exceptions".to_owned(),
        "-fno-rtti".to_owned(),
        format!("--target={}", target.as_str()),
        "-c".to_owned(),
        crate::path::slash_separated(&source),
        "-o".to_owned(),
        crate::path::slash_separated(&object),
    ];

    let configuration = compiler_configuration(arguments.clone());
    let compiler = crate::path::slash_separated(&clang);
    let started = Instant::now();

    crate::performance::peer::run_compiler(
        &compiler,
        &configuration,
        "building matched C++ library",
    )?;

    require_object(&object)?;

    Ok(CompilationBuildReport {
        toolchain: crate::performance::peer::command_identity(
            Command::new(&clang).arg("--version"),
            "clang",
        )?,
        source_sha256: source_digest(CPP_LIBRARY_SOURCE),
        elapsed_nanoseconds: crate::performance::peer::elapsed_nanoseconds(started),
        authority: library_authority(CPP_LIBRARY_SOURCE)?,
        compiler: external_invocation(
            compiler,
            arguments,
            BTreeMap::new(),
        ),
        linker: LinkerInvocationReport::NotApplicable,
        evidence: None,
        reuse: empty_reuse_evidence(),
        profile: None,
    })
}

fn compiler_configuration(arguments: Vec<String>) -> PeerCompilerConfiguration {
    PeerCompilerConfiguration {
        arguments,
        environment: BTreeMap::new(),
        batching: PeerBatching::SingleExecution,
    }
}

fn library_authority(source: &str) -> Result<crate::performance::model::CompilationAuthority, String> {
    Ok(authority(
        1,
        source_bytes(source)?,
        ["performance_library".to_owned()],
        ["performance_library".to_owned()],
        LibraryReuse::Source,
    ))
}

fn object_name(target: NativeTarget) -> PathBuf {
    PathBuf::from(match target.object_format() {
        ObjectFormat::Coff => "performance-library.obj",
        ObjectFormat::Elf | ObjectFormat::MachO => "performance-library.o",
        ObjectFormat::WebAssembly | ObjectFormat::Xcoff => "performance-library.o",
    })
}

fn require_object(path: &Path) -> Result<(), String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;

    if metadata.is_file() && metadata.len() > 0 {
        Ok(())
    } else {
        Err(format!("matched library object is empty: {}", path.display()))
    }
}

fn require_object_in_directory(directory: &Path, target: NativeTarget) -> Result<(), String> {
    let suffix = match target.object_format() {
        ObjectFormat::Coff => "obj",
        ObjectFormat::Elf | ObjectFormat::MachO => "o",
        ObjectFormat::WebAssembly | ObjectFormat::Xcoff => "o",
    };

    let has_object = std::fs::read_dir(directory)
        .map_err(|error| format!("could not inspect {}: {error}", directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("could not inspect {}: {error}", directory.display()))?
        .into_iter()
        .any(|entry| {
            entry.path().extension().is_some_and(|extension| extension == suffix)
                && entry.metadata().is_ok_and(|metadata| metadata.len() > 0)
        });

    if has_object {
        Ok(())
    } else {
        Err("matched Bray library produced no relocatable object".to_owned())
    }
}
