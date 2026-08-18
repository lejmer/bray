use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bray_target::{NativeTarget, ObjectFormat};

use crate::performance::compilation::{
    authority, external_invocation, source_bytes, source_digest,
};
use crate::performance::model::{
    CompilationBuildReport, CompilationEvidenceReport, CompilationKind, CompilationReuseEvidence,
    LibraryReuse, LinkerInvocationReport, PeerBatching, PeerCompilerConfiguration,
};

pub(super) struct PeerReportInput<'a> {
    pub(super) compiler: &'a str,
    pub(super) toolchain: String,
    pub(super) source: &'a str,
    pub(super) kind: CompilationKind,
    pub(super) elapsed_nanoseconds: u64,
    pub(super) packages: Vec<String>,
    pub(super) modules: Vec<String>,
    pub(super) configuration: PeerCompilerConfiguration,
    pub(super) evidence: Option<PeerCompilerConfiguration>,
    pub(super) linker_map: Option<crate::performance::model::LinkerMapReport>,
    pub(super) reuse: CompilationReuseEvidence,
}

pub(super) fn peer_report(input: PeerReportInput<'_>) -> Result<CompilationBuildReport, String> {
    let arguments = input.configuration.arguments;

    let (library_reuse, linker, evidence) = match input.kind {
        CompilationKind::Application => (
            LibraryReuse::Packaged,
            LinkerInvocationReport::IntegratedCompilerDriver {
                driver: input.compiler.to_owned(),
                arguments: arguments.clone(),
            },
            input
                .evidence
                .map(|configuration| CompilationEvidenceReport {
                    compiler: external_invocation(
                        input.compiler,
                        configuration.arguments,
                        configuration.environment,
                    ),
                    linker_map: input.linker_map,
                }),
        ),
        CompilationKind::Library => (
            LibraryReuse::Source,
            LinkerInvocationReport::NotApplicable,
            None,
        ),
    };

    Ok(CompilationBuildReport {
        toolchain: input.toolchain,
        source_sha256: source_digest(input.source),
        elapsed_nanoseconds: input.elapsed_nanoseconds,
        authority: authority(
            1,
            source_bytes(input.source)?,
            input.packages,
            input.modules,
            library_reuse,
        ),
        compiler: external_invocation(input.compiler, arguments, input.configuration.environment),
        linker,
        evidence,
        reuse: input.reuse,
        profile: None,
    })
}

pub(super) fn compiler_configuration(arguments: Vec<String>) -> PeerCompilerConfiguration {
    PeerCompilerConfiguration {
        arguments,
        environment: BTreeMap::new(),
        batching: PeerBatching::SingleExecution,
    }
}

pub(super) fn object_name(target: NativeTarget) -> PathBuf {
    PathBuf::from(match target.object_format() {
        ObjectFormat::Coff => "performance-library.obj",
        ObjectFormat::Elf | ObjectFormat::MachO => "performance-library.o",
        ObjectFormat::WebAssembly | ObjectFormat::Xcoff => "performance-library.o",
    })
}

pub(super) fn require_object(path: &Path) -> Result<(), String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;

    if metadata.is_file() && metadata.len() > 0 {
        Ok(())
    } else {
        Err(format!(
            "matched library object is empty: {}",
            path.display()
        ))
    }
}

pub(super) fn require_object_in_directory(
    directory: &Path,
    target: NativeTarget,
) -> Result<(), String> {
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
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == suffix)
                && entry.metadata().is_ok_and(|metadata| metadata.len() > 0)
        });

    if has_object {
        Ok(())
    } else {
        Err("matched Bray library produced no relocatable object".to_owned())
    }
}
