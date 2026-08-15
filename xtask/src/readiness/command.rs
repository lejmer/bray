use std::process::ExitCode;

use super::workspace::RustWorkspace;
use super::{codegen, emission, linker, lowering, memory, native_execution, semantic};
use crate::{command, workspace};

const USAGE: &str = "usage: cargo xtask readiness [semantic | diagnostics | lowering | memory | codegen | emission | linker | native-execution]";

pub(crate) fn run(mut arguments: impl Iterator<Item = String>) -> ExitCode {
    let audit = arguments.next();

    if audit.as_deref() == Some("native-execution") {
        return finish(
            command::reject_trailing_argument(arguments)
                .and_then(|()| workspace::root())
                .and_then(|root| {
                    crate::progress::run("Auditing native execution readiness", || {
                        native_execution::audit(&root)
                    })
                }),
        );
    }

    let result = command::reject_trailing_argument(arguments)
        .and_then(|()| workspace::root())
        .and_then(RustWorkspace::load)
        .and_then(|workspace| run_audit(audit.as_deref(), &workspace));

    finish(result)
}

fn finish(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");

            ExitCode::FAILURE
        }
    }
}

fn run_audit(audit: Option<&str>, workspace: &RustWorkspace) -> Result<(), String> {
    match audit {
        None => {
            audit_phase("Auditing semantic readiness", || semantic::audit_coverage(workspace))?;

            audit_phase("Auditing diagnostic readiness", || {
                semantic::audit_diagnostics(workspace)
            })?;

            audit_phase("Auditing lowering readiness", || lowering::audit(workspace))?;
            audit_phase("Auditing memory readiness", || memory::audit(workspace))?;
            audit_phase("Auditing code generation readiness", || codegen::audit(workspace))?;
            audit_phase("Auditing emission readiness", || emission::audit(workspace))?;

            audit_phase("Auditing linker readiness", || linker::audit(workspace))
        }
        Some("semantic") => audit_phase("Auditing semantic readiness", || {
            semantic::audit_coverage(workspace)
        }),
        Some("diagnostics") => audit_phase("Auditing diagnostic readiness", || {
            semantic::audit_diagnostics(workspace)
        }),
        Some("lowering") => {
            audit_phase("Auditing lowering readiness", || lowering::audit(workspace))
        }
        Some("memory") => audit_phase("Auditing memory readiness", || memory::audit(workspace)),
        Some("codegen") => audit_phase("Auditing code generation readiness", || {
            codegen::audit(workspace)
        }),
        Some("emission") => {
            audit_phase("Auditing emission readiness", || emission::audit(workspace))
        }
        Some("linker") => audit_phase("Auditing linker readiness", || linker::audit(workspace)),
        Some("native-execution") => unreachable!("native execution does not load the Rust corpus"),
        Some(_) => Err(USAGE.to_owned()),
    }
}

fn audit_phase(label: &str, audit: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
    crate::progress::run(label, audit)
}
