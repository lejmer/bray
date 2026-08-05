use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use bray_diagnostics::DiagnosticBag;
use bray_formatter::{FormatMode, FormatterConfiguration};
use bray_tooling::{
    OutputFormat, clap_styles, exit_code_from_diagnostics, render_styled_text, write_diagnostics,
};
use clap::Parser;
use clap::error::ErrorKind;

use crate::configuration::load_configuration;
use crate::diagnostic::{configuration_error_diagnostic, format_files, format_standard_input};

pub(crate) fn run(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();

    run_with_io(arguments, &mut io::stdin(), &mut stdout, &mut stderr)
}

fn run_with_io(
    arguments: impl IntoIterator<Item = OsString>,
    stdin: &mut impl Read,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> ExitCode {
    let cli = match Cli::try_parse_from(arguments) {
        Ok(cli) => cli,
        Err(error) => return write_clap_error(error, stdout, stderr),
    };

    let output_format = cli.format;

    let configuration = match cli.config.as_deref() {
        Some(path) => match load_configuration(path) {
            Ok(configuration) => configuration,
            Err(error) => {
                let diagnostics = DiagnosticBag::single(configuration_error_diagnostic(error));

                return write_result(&diagnostics, "", output_format, stdout, stderr);
            }
        },
        None => FormatterConfiguration::default(),
    };

    let mode = if cli.check {
        FormatMode::Check
    } else {
        FormatMode::Write
    };

    let (diagnostics, formatted) = if cli.files.as_slice() == [PathBuf::from("-")] {
        let mut bytes = Vec::new();

        if stdin.read_to_end(&mut bytes).is_err() {
            return ExitCode::FAILURE;
        }

        match format_standard_input(&bytes, mode, &configuration) {
            Ok(formatted) => (DiagnosticBag::new(), formatted),
            Err(diagnostics) => (diagnostics, String::new()),
        }
    } else {
        (
            format_files(&cli.files, mode, &configuration),
            String::new(),
        )
    };

    write_result(&diagnostics, &formatted, output_format, stdout, stderr)
}

fn write_result(
    diagnostics: &DiagnosticBag,
    formatted: &str,
    output_format: OutputFormat,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> ExitCode {
    if stdout.write_all(formatted.as_bytes()).is_err()
        || write_diagnostics(diagnostics, None, output_format, stdout, stderr).is_err()
    {
        return ExitCode::FAILURE;
    }

    exit_code_from_diagnostics(diagnostics)
}

fn write_clap_error(
    error: clap::Error,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> ExitCode {
    let exit_code = if matches!(
        error.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    ) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    };

    let output = render_styled_text(&error.render());

    let destination = if exit_code == ExitCode::SUCCESS {
        stdout as &mut dyn Write
    } else {
        stderr as &mut dyn Write
    };

    if destination.write_all(output.as_bytes()).is_err() {
        return ExitCode::FAILURE;
    }

    exit_code
}

#[derive(Debug, Parser)]
#[command(
    name = "brayfmt",
    version = env!("CARGO_PKG_VERSION"),
    about = "The Bray source formatter",
    styles = clap_styles()
)]
struct Cli {
    #[arg(long)]
    check: bool,
    #[arg(long, value_enum, default_value = "text")]
    format: OutputFormat,
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,
    #[arg(value_name = "FILE", required = true)]
    files: Vec<PathBuf>,
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::process::ExitCode;

    use super::run_with_io;

    #[test]
    fn formats_standard_input_for_pipeline_use() {
        let mut input = Cursor::new(b"module example;".to_vec());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_io(
            ["brayfmt".into(), "-".into()],
            &mut input,
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert_eq!(stdout, b"module example;\n");
        assert!(stderr.is_empty());
    }
}
