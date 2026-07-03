use std::ffi::OsString;
use std::process::ExitCode;

use bray_compilation::Compilation;

use crate::command::DriverInvocation;

/// Runs the Bray compiler driver for the provided process arguments.
pub fn run(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    let invocation = match DriverInvocation::try_from_arguments(arguments) {
        Ok(invocation) => invocation,
        Err(error) => return error.exit_code(),
    };

    let request = match invocation.into_compilation_request() {
        Ok(request) => request,
        Err(_) => return ExitCode::FAILURE,
    };

    let compilation = match Compilation::build(request) {
        Ok(compilation) => compilation,
        Err(_) => return ExitCode::FAILURE,
    };

    if compilation.diagnostics().has_errors() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::process::ExitCode;

    use super::run;
    use crate::test_support::TemporaryFile;

    #[test]
    fn run_fails_when_final_compilation_diagnostics_have_errors() {
        let file = TemporaryFile::write("bad.bray", &[0xff]);

        assert_eq!(
            run([
                OsString::from("brayc"),
                OsString::from("check"),
                file.path().as_os_str().to_os_string(),
            ]),
            ExitCode::FAILURE
        );
    }
}
