use std::path::Path;
use std::process::Command;

use bray_base::lowercase_hex;
use bray_target::NativeTarget;
use sha2::{Digest as _, Sha256};

use super::super::corpus::{
    BatchingPolicy, CALIBRATION_SAMPLE_COUNT, CALIBRATION_SEED_INNER_ITERATIONS,
    CALIBRATION_TARGET_NANOSECONDS, CORPUS_REVISION, ExpectedOutput, ExpectedSideEffects, Workload,
};
use super::super::model::{ReportIdentity, WorkloadCategory};
use super::options::Options;

pub(super) fn report_identity(
    root: &Path,
    options: &Options,
    workloads: &[&Workload],
    timer_resolution_nanoseconds: u64,
    standard_library: &Path,
    runtime: &Path,
) -> Result<ReportIdentity, String> {
    let corpus_sha256 = corpus_digest(workloads);

    let mut source_revision = command_text(
        Command::new("git")
            .current_dir(root)
            .args(["rev-parse", "HEAD"]),
    )?;

    let worktree_status = command_text(Command::new("git").current_dir(root).args([
        "status",
        "--porcelain=v1",
        "--untracked-files=normal",
    ]))?;

    if !worktree_status.is_empty() {
        source_revision.push_str("+modified");
    }

    let llvm =
        bray_tooling::llvm_tool_path(bray_diagnostics::DiagnosticLlvmToolRole::CompilerDriver)
            .map_err(|error| format!("LLVM compiler driver is unavailable: {error}"))?;

    let llvm_version = command_text(Command::new(llvm).arg("--version"))?;

    Ok(ReportIdentity {
        corpus_revision: CORPUS_REVISION,
        corpus_sha256,
        target: options.target.as_str().to_owned(),
        host: NativeTarget::current()
            .map_or_else(|| "unsupported".to_owned(), |host| host.as_str().to_owned()),
        host_class: options.host_class.clone(),
        worker_budget: bray_compilation::WorkerBudget::available_parallelism().get(),
        rustc_version: command_text(Command::new("rustc").arg("--version"))?,
        standard_library_sha256: file_digest(&standard_library.join("manifest.json"))?,
        runtime_sha256: runtime_digest(runtime)?,
        build_configuration: "release".to_owned(),
        compiler_version: env!("CARGO_PKG_VERSION").to_owned(),
        source_revision,
        measured_at_unix_nanoseconds: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("measurement clock precedes Unix epoch: {error}"))?
            .as_nanos(),
        llvm_version: llvm_version
            .lines()
            .next()
            .unwrap_or(&llvm_version)
            .to_owned(),
        runtime_linkage: super::super::peer::runtime_linkage(options.target)?,
        warmup_iterations: options.warmup,
        sample_iterations: options.samples,
        timer_resolution_nanoseconds,
    })
}

fn file_digest(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("could not fingerprint {}: {error}", path.display()))?;

    Ok(hex(Sha256::digest(bytes).into()))
}

fn runtime_digest(metadata: &Path) -> Result<String, String> {
    let directory = metadata
        .parent()
        .expect("prepared runtime metadata has a package directory");

    let mut paths = std::fs::read_dir(directory)
        .map_err(|error| format!("could not inspect {}: {error}", directory.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("could not inspect runtime inputs: {error}"))?;

    paths.sort();

    let mut digest = Sha256::new();

    for path in paths {
        if path.is_file() && path.file_name().is_some_and(|name| name != "input.sha256") {
            digest.update(
                path.file_name()
                    .expect("runtime input has a name")
                    .as_encoded_bytes(),
            );

            digest.update([0]);
            digest.update(file_digest(&path)?);
        }
    }

    Ok(hex(digest.finalize().into()))
}

pub(in crate::performance) fn expected_output_digest(
    output: ExpectedOutput,
) -> Result<String, String> {
    let mut digest = Sha256::new();

    match output {
        ExpectedOutput::Empty => {}
        ExpectedOutput::Repeated { byte, count } => {
            let bytes = [byte; 1024];

            let chunk_size = u64::try_from(bytes.len())
                .map_err(|_| "output digest chunk size is not representable".to_owned())?;

            let full_chunks = count / chunk_size;

            let remainder = usize::try_from(count % chunk_size)
                .map_err(|_| "output digest remainder is not representable".to_owned())?;

            for _ in 0..full_chunks {
                digest.update(bytes);
            }

            digest.update(&bytes[..remainder]);
        }
    }

    Ok(hex(digest.finalize().into()))
}

fn corpus_digest(workloads: &[&Workload]) -> String {
    let mut digest = Sha256::new();

    for workload in workloads {
        digest.update(workload.id.as_bytes());
        digest.update([0]);
        digest.update(category_name(workload.category).as_bytes());
        digest.update([0]);
        digest.update(workload.scale.to_le_bytes());

        match workload.batching {
            BatchingPolicy::SingleExecution => digest.update([0]),
            BatchingPolicy::Calibrated => {
                digest.update([1]);
                digest.update(CALIBRATION_SEED_INNER_ITERATIONS.to_le_bytes());
                digest.update(CALIBRATION_SAMPLE_COUNT.to_le_bytes());
                digest.update(CALIBRATION_TARGET_NANOSECONDS.to_le_bytes());
            }
        }

        digest.update(workload.units.as_bytes());
        digest.update([0]);
        digest.update(workload.source.as_bytes());
        digest.update([0]);
        digest.update(super::super::peer::corpus_contract(workload.id).as_bytes());
        digest.update([0]);

        match workload.expected_output {
            ExpectedOutput::Empty => digest.update([0]),
            ExpectedOutput::Repeated { byte, count } => {
                digest.update([1, byte]);
                digest.update(count.to_le_bytes());
            }
        }

        match workload.expected_side_effects {
            ExpectedSideEffects::None => digest.update([0]),
            ExpectedSideEffects::AbsentPath(path) => {
                digest.update([1]);
                digest.update(path.as_bytes());
                digest.update([0]);
            }
        }

        for operation in workload.platform_operations {
            digest.update(operation.as_bytes());
            digest.update([0]);
        }

        for identities in [
            workload.retention.required_symbols,
            workload.retention.forbidden_symbols,
            workload.retention.required_provenance,
            workload.retention.forbidden_provenance,
        ] {
            for identity in identities {
                digest.update(identity.as_bytes());
                digest.update([0]);
            }

            digest.update([0xff]);
        }
    }

    hex(digest.finalize().into())
}

const fn category_name(category: WorkloadCategory) -> &'static str {
    match category {
        WorkloadCategory::Small => "small",
        WorkloadCategory::CoreData => "core_data",
        WorkloadCategory::Formatting => "formatting",
        WorkloadCategory::Streaming => "streaming",
        WorkloadCategory::Concurrent => "concurrent",
        WorkloadCategory::Filesystem => "filesystem",
        WorkloadCategory::Process => "process",
        WorkloadCategory::Time => "time",
    }
}

fn command_text(command: &mut Command) -> Result<String, String> {
    let output = command
        .output()
        .map_err(|error| format!("could not run tool: {error}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn hex(bytes: [u8; 32]) -> String {
    lowercase_hex(&bytes)
}

#[cfg(test)]
mod tests {
    #[test]
    fn runtime_identity_fingerprints_the_metadata_package_and_ignores_cache_stamps() {
        let package = tempfile::tempdir().expect("runtime package");
        let metadata = package.path().join("bray-runtime.brayrt");
        let archive = package.path().join("libbray_runtime.a");

        std::fs::write(&metadata, b"metadata").expect("runtime metadata");
        std::fs::write(&archive, b"archive").expect("runtime archive");

        let original = super::runtime_digest(&metadata).expect("metadata file identifies package");

        std::fs::write(package.path().join("input.sha256"), b"cache stamp")
            .expect("runtime cache stamp");

        assert_eq!(
            super::runtime_digest(&metadata).expect("package identity"),
            original
        );

        std::fs::write(&archive, b"changed archive").expect("changed runtime archive");

        let changed = super::runtime_digest(&metadata).expect("changed package identity");

        assert_ne!(changed, original);

        std::fs::write(&metadata, b"changed metadata").expect("changed runtime metadata");

        assert_ne!(
            super::runtime_digest(&metadata).expect("changed metadata identity"),
            changed
        );
    }
}
