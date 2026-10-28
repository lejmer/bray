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

#[cfg(all(test, feature = "analysis"))]
mod tests {
    use bray_compilation::{Compilation, CompilationRequest};
    use bray_diagnostics::DiagnosticKind;
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::PackageIdentity;

    use super::write_diagnostics;
    use crate::OutputFormat;

    #[test]
    fn ignored_result_warnings_preserve_context_in_text_and_json() {
        let compilation = Compilation::load(CompilationRequest::new(
            PackageIdentity::try_new("test.package")
                .expect("fixture package identity must be valid"),
            vec![SourceInput::virtual_text(
                SourceIdentity::new(0),
                "discard.bray",
                SourceVersion::new(1),
                "module app; func main() { 1; }",
            )],
        ))
        .expect("fixture input must load");

        let diagnostics = compilation.check_diagnostics();

        bray_testing::assert_goal_state_diagnostic_kind(
            diagnostics,
            DiagnosticKind::CheckingIgnoredExpressionResult,
        );

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        write_diagnostics(
            diagnostics,
            Some(compilation.sources()),
            OutputFormat::Text,
            &mut stdout,
            &mut stderr,
        )
        .expect("fixture `write_diagnostics` call must succeed");

        let text = String::from_utf8(stderr).expect("fixture text must be valid UTF-8");

        assert!(stdout.is_empty());
        assert!(text.contains("warning"), "{text}");
        assert!(text.contains("i32"), "{text}");
        assert!(text.contains("_ = expression;"), "{text}");

        let mut stderr = Vec::new();

        write_diagnostics(
            diagnostics,
            Some(compilation.sources()),
            OutputFormat::Json,
            &mut stdout,
            &mut stderr,
        )
        .expect("fixture `write_diagnostics` call must succeed");

        let json: serde_json::Value =
            serde_json::from_slice(&stdout).expect("fixture data must deserialize");

        assert!(stderr.is_empty());
        assert_eq!(json["has_errors"], false);

        assert_eq!(
            json["diagnostics"][0]["kind"],
            "checking_ignored_expression_result"
        );

        assert_eq!(json["diagnostics"][0]["severity"], "warning");

        assert_eq!(
            json["diagnostics"][0]["suggestions"][0]["edits"][0]["replacement"],
            "_ = "
        );
    }
}
