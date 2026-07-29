use std::process::ExitCode;

use super::workspace::RustWorkspace;
use super::{codegen, linker, lowering, semantic};
use crate::{command, workspace};

const USAGE: &str =
    "usage: cargo xtask readiness [semantic | diagnostics | lowering | codegen | linker]";

pub(crate) fn run(mut arguments: impl Iterator<Item = String>) -> ExitCode {
    let audit = arguments.next();

    let result = command::reject_trailing_argument(arguments)
        .and_then(|()| workspace::root())
        .and_then(RustWorkspace::load)
        .and_then(|workspace| run_audit(audit.as_deref(), &workspace));

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
            semantic::audit_coverage(workspace)?;
            semantic::audit_diagnostics(workspace)?;
            lowering::audit(workspace)?;
            linker::audit(workspace)?;

            codegen::audit(workspace)
        }
        Some("semantic") => semantic::audit_coverage(workspace),
        Some("diagnostics") => semantic::audit_diagnostics(workspace),
        Some("lowering") => lowering::audit(workspace),
        Some("codegen") => codegen::audit(workspace),
        Some("linker") => linker::audit(workspace),
        Some(_) => Err(USAGE.to_owned()),
    }
}
