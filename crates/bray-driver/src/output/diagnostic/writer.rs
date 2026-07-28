use std::io::{self, Write};

use crate::command::DriverOutputFormat;
use crate::run::DriverRunResult;

use super::json::write_json_diagnostics;
use super::text::write_text_diagnostics;

pub(crate) fn write_driver_output(
    result: &DriverRunResult,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> io::Result<()> {
    if let Some(path) = result.report_file() {
        std::fs::write(path, result.stdout().as_bytes())?;
    }

    stdout.write_all(result.stdout().as_bytes())?;
    stderr.write_all(result.stderr().as_bytes())?;

    if result.has_terminal_output() {
        return Ok(());
    }

    match result.output_format() {
        DriverOutputFormat::Text => {
            write_text_diagnostics(result.diagnostics(), result.sources(), stderr)
        }
        DriverOutputFormat::Json => {
            write_json_diagnostics(result.diagnostics(), result.sources(), stdout)
        }
    }
}
