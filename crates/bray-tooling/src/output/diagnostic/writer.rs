use std::io::{self, Write};

use bray_diagnostics::DiagnosticBag;

use crate::OutputFormat;

use super::json::{write_json_diagnostic_groups, write_json_diagnostics};
use super::text::write_text_diagnostics;

/// Writes one diagnostic bag to the stream selected by the output format.
pub fn write_diagnostics(
    diagnostics: &DiagnosticBag,
    sources: Option<&bray_source::SourceStore>,
    output_format: OutputFormat,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> io::Result<()> {
    match output_format {
        OutputFormat::Text => write_text_diagnostics(diagnostics, sources, stderr),
        OutputFormat::Json => write_json_diagnostics(diagnostics, sources, stdout),
    }
}

/// Writes compilation-scoped diagnostic groups without merging source identities.
pub fn write_diagnostic_groups<'diagnostic>(
    groups: impl IntoIterator<
        Item = (
            &'diagnostic DiagnosticBag,
            Option<&'diagnostic bray_source::SourceStore>,
        ),
    >,
    output_format: OutputFormat,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> io::Result<()> {
    match output_format {
        OutputFormat::Text => {
            let mut wrote_diagnostics = false;

            for (diagnostics, sources) in groups {
                if diagnostics.is_empty() {
                    continue;
                }

                if wrote_diagnostics {
                    writeln!(stderr)?;
                }

                write_text_diagnostics(diagnostics, sources, stderr)?;
                wrote_diagnostics = true;
            }

            Ok(())
        }
        OutputFormat::Json => write_json_diagnostic_groups(groups, stdout),
    }
}
