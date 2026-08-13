use std::path::Path;
use std::process::Command;

use bray_base::lowercase_hex;
use bray_target::NativeTarget;
use sha2::{Digest as _, Sha256};

use super::super::corpus::{CORPUS_REVISION, ExpectedOutput, Workload};
use super::super::model::{ReportIdentity, WorkloadCategory};
use super::options::Options;

pub(super) fn report_identity(
    root: &Path,
    options: &Options,
    workloads: &[&Workload],
) -> Result<ReportIdentity, String> {
    let corpus_sha256 = corpus_digest(workloads);

    let mut source_revision =
        command_text(Command::new("git").current_dir(root).args(["rev-parse", "HEAD"]))?;

    let worktree_status = command_text(
        Command::new("git")
            .current_dir(root)
            .args(["status", "--porcelain=v1", "--untracked-files=normal"]),
    )?;

    if !worktree_status.is_empty() {
        source_revision.push_str("+modified");
    }

    let llvm = bray_tooling::llvm_tool_path(bray_diagnostics::DiagnosticLlvmToolRole::CompilerDriver)
        .map_err(|error| format!("LLVM compiler driver is unavailable: {error}"))?;

    let llvm_version = command_text(Command::new(llvm).arg("--version"))?;

    Ok(ReportIdentity {
        corpus_revision: CORPUS_REVISION,
        corpus_sha256,
        target: options.target.as_str().to_owned(),
        host: NativeTarget::current()
            .map_or_else(|| "unsupported".to_owned(), |host| host.as_str().to_owned()),
        build_configuration: "release".to_owned(),
        compiler_version: env!("CARGO_PKG_VERSION").to_owned(),
        source_revision,
        llvm_version: llvm_version.lines().next().unwrap_or(&llvm_version).to_owned(),
        warmup_iterations: options.warmup,
        sample_iterations: options.samples,
    })
}

pub(in crate::standard_library::performance) fn expected_output_digest(
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
        digest.update(workload.units.as_bytes());
        digest.update([0]);
        digest.update(workload.source.as_bytes());
        digest.update([0]);
        digest.update(super::super::peer::corpus_contract(workload.id).as_bytes());
        digest.update([0]);

        for source in workload.standard_library_sources {
            digest.update(source.as_bytes());
            digest.update([0]);
        }

        match workload.expected_output {
            ExpectedOutput::Empty => digest.update([0]),
            ExpectedOutput::Repeated { byte, count } => {
                digest.update([1, byte]);
                digest.update(count.to_le_bytes());
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
    let output = command.output().map_err(|error| format!("could not run tool: {error}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn hex(bytes: [u8; 32]) -> String {
    lowercase_hex(&bytes)
}
