use std::process::ExitCode;

use crate::{command, workspace};

pub(crate) fn run(mut arguments: impl Iterator<Item = String>) -> ExitCode {
    let result = match arguments.next().as_deref() {
        None => workspace::root()
            .and_then(|root| rust_style::fix_workspace(&root).map(|()| root))
            .and_then(|root| crate::source_format::write_workspace(&root)),
        Some("check") => command::reject_trailing_argument(arguments)
            .and_then(|()| workspace::root())
            .and_then(|root| rust_style::check_workspace(&root).map(|()| root))
            .and_then(|root| crate::source_format::check_workspace(&root)),
        Some(action) => Err(format!("unexpected style command: {action}")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");

            ExitCode::FAILURE
        }
    }
}
