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
    fn panic_argument_diagnostics_preserve_text_and_json_context() {
        for (call, kind, code, message, help, argument, note, source_text) in [
            (
                "panic()",
                "binding_panic_argument_count_mismatch",
                6020,
                "panic expects one message argument, received 0",
                "provide exactly one message expression compatible with string",
                serde_json::json!({"name":"actual_count", "value":{"kind":"count", "value":0}}),
                "panic_requires_one_message",
                "()",
            ),
            (
                "panic(message = \"message\")",
                "binding_named_panic_argument",
                6021,
                "panic does not accept named argument 'message'",
                "pass the message as a positional argument",
                serde_json::json!({"name":"referenced_name", "value":{"kind":"referenced_name", "value":"message"}}),
                "panic_message_requires_positional_argument",
                "message",
            ),
        ] {
            let source = format!("module app; func main() {{ {call}; }}");

            let compilation = Compilation::load(CompilationRequest::new(
                PackageIdentity::try_new("test.package")
                    .expect("fixture package identity is valid"),
                vec![SourceInput::virtual_text(
                    SourceIdentity::new(0),
                    "panic.bray",
                    SourceVersion::new(0),
                    source,
                )],
            ))
            .expect("panic fixture loads");

            let diagnostics = compilation.check_diagnostics();
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();

            write_diagnostics(
                diagnostics,
                Some(compilation.sources()),
                OutputFormat::Text,
                &mut stdout,
                &mut stderr,
            )
            .expect("panic diagnostic text renders");

            let text = String::from_utf8(stderr).expect("diagnostic text is UTF-8");

            assert!(stdout.is_empty());

            for expected in [message, help, "panic arguments", "panic.bray"] {
                assert!(text.contains(expected), "{text}");
            }

            let mut stderr = Vec::new();

            write_diagnostics(
                diagnostics,
                Some(compilation.sources()),
                OutputFormat::Json,
                &mut stdout,
                &mut stderr,
            )
            .expect("panic diagnostic JSON renders");

            let json: serde_json::Value =
                serde_json::from_slice(&stdout).expect("diagnostic JSON parses");

            let diagnostic = json["diagnostics"]
                .as_array()
                .expect("JSON has diagnostics")
                .iter()
                .find(|diagnostic| diagnostic["kind"] == kind)
                .expect("JSON retains the panic argument cause");

            let snapshot = serde_json::json!({
                "code":diagnostic["code"], "kind":diagnostic["kind"], "severity":diagnostic["severity"],
                "args":diagnostic["args"], "label":diagnostic["labels"][0]["kind"],
                "note":diagnostic["notes"][0]["kind"],
            });

            assert!(stderr.is_empty());
            assert_eq!(json["has_errors"], true);

            assert_eq!(
                snapshot,
                serde_json::json!({
                    "code":code, "kind":kind, "severity":"error", "args":[argument],
                    "label":"panic_arguments", "note":note,
                })
            );

            let start = usize::try_from(
                diagnostic["primary_span"]["start"]
                    .as_u64()
                    .expect("span has its start"),
            )
            .expect("source offset fits the host");

            let end = usize::try_from(
                diagnostic["primary_span"]["end"]
                    .as_u64()
                    .expect("span has its end"),
            )
            .expect("source offset fits the host");

            let input = compilation
                .sources()
                .get(bray_source::SourceId::new(0))
                .expect("source is retained");

            assert_eq!(&input.text()[start..end], source_text);
        }
    }

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
