use std::process::ExitCode;

use super::workspace::RustWorkspace;
use super::{codegen, emission, linker, lowering, native_execution, semantic};
use crate::{command, workspace};

const USAGE: &str =
    "usage: cargo xtask readiness [semantic | diagnostics | lowering | codegen | emission | linker | native-execution]";

pub(crate) fn run(mut arguments: impl Iterator<Item = String>) -> ExitCode {
    let audit = arguments.next();

    if audit.as_deref() == Some("native-execution") {
        return finish(
            command::reject_trailing_argument(arguments)
                .and_then(|()| workspace::root())
                .and_then(|root| native_execution::audit(&root)),
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
            semantic::audit_coverage(workspace)?;
            semantic::audit_diagnostics(workspace)?;
            lowering::audit(workspace)?;
            codegen::audit(workspace)?;
            emission::audit(workspace)?;

            linker::audit(workspace)
        }
        Some("semantic") => semantic::audit_coverage(workspace),
        Some("diagnostics") => semantic::audit_diagnostics(workspace),
        Some("lowering") => lowering::audit(workspace),
        Some("codegen") => codegen::audit(workspace),
        Some("emission") => emission::audit(workspace),
        Some("linker") => linker::audit(workspace),
        Some("native-execution") => unreachable!("native execution does not load the Rust corpus"),
        Some(_) => Err(USAGE.to_owned()),
    }
}
