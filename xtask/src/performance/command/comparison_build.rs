use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use bray_target::{NativeTarget, ObjectFormat};

use super::super::compilation::{
    MATCHED_APPLICATION_CONTRACT, MATCHED_LIBRARY_CONTRACT, authority, comparison,
    empty_reused_artifacts, external_invocation, reused_artifacts, source_digest,
};
use super::super::model::{
    ArtifactKind, CompilationBuildReport, CompilationKind, CompilationLanguage, LibraryReuse,
    LinkerInvocationReport, PeerBatching, PeerCompilerConfiguration, RetainedInput,
};

const BRAY_APPLICATION_SOURCE: &str =
    include_str!("../../../fixtures/performance-application.bray");
const RUST_APPLICATION_SOURCE: &str =
    include_str!("../../../fixtures/performance-application.rs");
const CPP_APPLICATION_SOURCE: &str =
    include_str!("../../../fixtures/performance-application.cpp");
const BRAY_LIBRARY_SOURCE: &str = include_str!("../../../fixtures/performance-library.bray");
const RUST_LIBRARY_SOURCE: &str = include_str!("../../../fixtures/performance-library.rs");
const CPP_LIBRARY_SOURCE: &str = include_str!("../../../fixtures/performance-library.cpp");

pub(super) fn build_application(
    root: &Path,
    compiler: &Path,
    output: &Path,
    target: NativeTarget,
    toolchain: &Path,
    runtime: &Path,
) -> Result<super::super::model::CompilationComparisonReport, String> {
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
    let profile = output.join("profile.json");
    let standard_library = toolchain.join("lib/bray/standard-library");

    std::fs::write(&source, BRAY_APPLICATION_SOURCE)
        .map_err(|error| format!("could not write {}: {error}", source.display()))?;

    let arguments = vec![
        "--profile".to_owned(),
        "summary".to_owned(),
        "--profile-output".to_owned(),
        crate::path::slash_separated(&profile),
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
        "--output".to_owned(),
        crate::path::slash_separated(&output),
        crate::path::slash_separated(&source),
    ];

    let run = super::compiler::run(
        compiler,
        arguments.clone(),
        &profile,
        "building matched Bray application",
    )?;

    let reused_artifacts = reused_artifacts(
        &[],
        [
            RetainedInput {
                artifact: crate::path::slash_separated(
                    &standard_library.join(bray_standard_library::STANDARD_LIBRARY_MANIFEST_FILE_NAME),
                ),
                member: None,
            },
            RetainedInput {
                artifact: crate::path::slash_separated(runtime),
                member: None,
            },
        ],
    );

    let compiler_identity = crate::path::slash_separated(compiler);

    Ok(CompilationBuildReport {
        toolchain: super::super::peer::command_identity(
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
        compiler: external_invocation(
            compiler_identity.clone(),
            arguments.clone(),
            BTreeMap::new(),
        ),
        linker: LinkerInvocationReport::IntegratedCompilerDriver {
            driver: compiler_identity,
            arguments,
        },
        reused_artifacts,
        profile: Some(run.profile),
    })
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
    let executable = output.join(crate::native_toolchain::executable_name("application"));
    let linker_map = output.join("application.map");

    std::fs::write(&source, RUST_APPLICATION_SOURCE)
        .map_err(|error| format!("could not write {}: {error}", source.display()))?;

    let (_linker, configuration) = super::super::peer::matched_rust_configuration(
        root,
        &source,
        &executable,
        &linker_map,
        target,
    )?;

    let started = Instant::now();

    super::super::peer::run_compiler(
        "rustc",
        &configuration,
        "building matched Rust application",
    )?;

    let elapsed_nanoseconds = super::super::peer::elapsed_nanoseconds(started);

    let artifact = super::super::retention::inspect(
        ArtifactKind::Executable,
        &executable,
        Some(&linker_map),
    )?;

    Ok(application_build(
        "rustc",
        super::super::peer::command_identity(Command::new("rustc").arg("--version"), "rustc")?,
        RUST_APPLICATION_SOURCE,
        elapsed_nanoseconds,
        ["performance_application".to_owned(), "std".to_owned()],
        ["crate".to_owned(), "std".to_owned()],
        configuration,
        reused_artifacts(&[artifact], []),
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
    let executable = output.join(crate::native_toolchain::executable_name("application"));
    let linker_map = output.join("application.map");

    std::fs::write(&source, CPP_APPLICATION_SOURCE)
        .map_err(|error| format!("could not write {}: {error}", source.display()))?;

    let clang =
        bray_tooling::llvm_tool_path(bray_diagnostics::DiagnosticLlvmToolRole::CompilerDriver)
            .unwrap_or_else(|_| bray_llvm_toolchain::tool_path(root, "clang"));

    let configuration = super::super::peer::matched_cpp_configuration(
        &source,
        &executable,
        &linker_map,
        target,
    )?;

    let compiler = crate::path::slash_separated(&clang);
    let started = Instant::now();

    super::super::peer::run_compiler(
        &compiler,
        &configuration,
        "building matched C++ application",
    )?;

    let elapsed_nanoseconds = super::super::peer::elapsed_nanoseconds(started);

    let artifact = super::super::retention::inspect(
        ArtifactKind::Executable,
        &executable,
        Some(&linker_map),
    )?;

    Ok(application_build(
        &compiler,
        super::super::peer::command_identity(
            Command::new(&clang).arg("--version"),
            "clang",
        )?,
        CPP_APPLICATION_SOURCE,
        elapsed_nanoseconds,
        ["cxx_runtime".to_owned(), "performance_application".to_owned()],
        ["cxx_runtime".to_owned(), "translation_unit".to_owned()],
        configuration,
        reused_artifacts(&[artifact], []),
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
    reused_artifacts: super::super::model::BoundedList<RetainedInput>,
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
        reused_artifacts,
        profile: None,
    })
}

pub(super) fn build_library(
    root: &Path,
    compiler: &Path,
    output: &Path,
    target: NativeTarget,
) -> Result<super::super::model::CompilationComparisonReport, String> {
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
    let profile = output.join("profile.json");

    std::fs::write(&source, BRAY_LIBRARY_SOURCE)
        .map_err(|error| format!("could not write {}: {error}", source.display()))?;

    let arguments = vec![
        "--profile".to_owned(),
        "summary".to_owned(),
        "--profile-output".to_owned(),
        crate::path::slash_separated(&profile),
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
        crate::path::slash_separated(&output),
        crate::path::slash_separated(&source),
    ];

    let run = super::compiler::run(
        compiler,
        arguments,
        &profile,
        "building matched Bray library",
    )?;

    require_object_in_directory(&output, target)?;

    Ok(CompilationBuildReport {
        toolchain: super::super::peer::command_identity(
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
        reused_artifacts: empty_reused_artifacts(),
        profile: Some(run.profile),
    })
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

    super::super::peer::run_compiler(
        "rustc",
        &configuration,
        "building matched Rust library",
    )?;

    require_object(&object)?;

    Ok(CompilationBuildReport {
        toolchain: super::super::peer::command_identity(
            Command::new("rustc").arg("--version"),
            "rustc",
        )?,
        source_sha256: source_digest(RUST_LIBRARY_SOURCE),
        elapsed_nanoseconds: super::super::peer::elapsed_nanoseconds(started),
        authority: library_authority(RUST_LIBRARY_SOURCE)?,
        compiler: external_invocation(
            "rustc",
            arguments,
            BTreeMap::new(),
        ),
        linker: LinkerInvocationReport::NotApplicable,
        reused_artifacts: empty_reused_artifacts(),
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

    super::super::peer::run_compiler(
        &compiler,
        &configuration,
        "building matched C++ library",
    )?;

    require_object(&object)?;

    Ok(CompilationBuildReport {
        toolchain: super::super::peer::command_identity(
            Command::new(&clang).arg("--version"),
            "clang",
        )?,
        source_sha256: source_digest(CPP_LIBRARY_SOURCE),
        elapsed_nanoseconds: super::super::peer::elapsed_nanoseconds(started),
        authority: library_authority(CPP_LIBRARY_SOURCE)?,
        compiler: external_invocation(
            compiler,
            arguments,
            BTreeMap::new(),
        ),
        linker: LinkerInvocationReport::NotApplicable,
        reused_artifacts: empty_reused_artifacts(),
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

fn library_authority(source: &str) -> Result<super::super::model::CompilationAuthority, String> {
    Ok(authority(
        1,
        source_bytes(source)?,
        ["performance_library".to_owned()],
        ["performance_library".to_owned()],
        LibraryReuse::Source,
    ))
}

fn source_bytes(source: &str) -> Result<u64, String> {
    u64::try_from(source.len()).map_err(|_| "compilation source byte count exceeds u64".to_owned())
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
