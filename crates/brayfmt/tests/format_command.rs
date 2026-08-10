use std::ffi::OsStr;
use std::io::Write;
use std::process::{Command, Output, Stdio};

use bray_diagnostics::DiagnosticKind;
use bray_testing::TemporaryFile;

const UNFORMATTED: &[u8] = b"module app;func main(){return;}";
const FORMATTED: &str = concat!(
    "module app;\n",
    "\n",
    "func main()\n",
    "{\n",
    "    return;\n",
    "}\n",
);

#[test]
fn standard_input_write_formats_to_standard_output() {
    let output = run(["-"], UNFORMATTED);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), FORMATTED);
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn standard_input_check_reports_changed_and_unchanged_source() {
    let changed = run(["--check", "-"], UNFORMATTED);

    assert!(!changed.status.success(), "{changed:?}");
    assert!(changed.stdout.is_empty(), "{changed:?}");

    assert!(
        String::from_utf8_lossy(&changed.stderr).contains("source needs formatting: -"),
        "{changed:?}"
    );

    let unchanged = run(["--check", "-"], FORMATTED.as_bytes());

    assert!(unchanged.status.success(), "{unchanged:?}");
    assert!(unchanged.stdout.is_empty(), "{unchanged:?}");
    assert!(unchanged.stderr.is_empty(), "{unchanged:?}");
}

#[test]
fn file_write_and_check_modes_publish_only_when_requested() {
    let file = TemporaryFile::write("main.bray", UNFORMATTED);
    let write = run([file.path().as_os_str().to_owned()], &[]);

    assert!(write.status.success(), "{write:?}");
    assert!(write.stdout.is_empty(), "{write:?}");
    assert!(write.stderr.is_empty(), "{write:?}");
    assert_eq!(read(file.path()), FORMATTED.as_bytes());

    std::fs::write(file.path(), UNFORMATTED)
        .unwrap_or_else(|error| panic!("test source should be reset: {error:?}"));

    let check = run(["--check".into(), file.path().as_os_str().to_owned()], &[]);

    assert!(!check.status.success(), "{check:?}");
    assert_eq!(read(file.path()), UNFORMATTED);

    assert!(
        String::from_utf8_lossy(&check.stderr).contains("source needs formatting:"),
        "{check:?}"
    );
}

#[test]
fn standard_input_rejects_invalid_utf8_through_structured_messages() {
    let output = run(["-"], &[b'm', 0xff, b'x']);

    assert!(!output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");

    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("formatter source contains invalid UTF-8: -"),
        "{output:?}"
    );
}

#[test]
fn explicit_configuration_files_are_validated_before_formatting() {
    let valid = TemporaryFile::write(
        "valid-brayfmt.json",
        br#"{"maximum_line_width":88,"rules":{"indentation":false}}"#,
    );

    let valid_output = run(
        [
            "--config".into(),
            valid.path().as_os_str().to_owned(),
            "-".into(),
        ],
        UNFORMATTED,
    );

    assert!(valid_output.status.success(), "{valid_output:?}");

    let unknown = TemporaryFile::write(
        "unknown-brayfmt.json",
        br#"{"rules":{"unknown-rule":true}}"#,
    );

    let unknown_output = run(
        [
            "--config".into(),
            unknown.path().as_os_str().to_owned(),
            "-".into(),
        ],
        UNFORMATTED,
    );

    assert!(!unknown_output.status.success(), "{unknown_output:?}");

    assert!(
        String::from_utf8_lossy(&unknown_output.stderr)
            .contains("unknown formatter rule 'unknown-rule'"),
        "{unknown_output:?}"
    );
}

#[test]
fn invalid_configuration_is_available_as_structured_json() {
    let invalid = TemporaryFile::write("invalid-brayfmt.json", br#"{"maximum_line_width":0}"#);

    let output = run(
        [
            "--format".into(),
            "json".into(),
            "--config".into(),
            invalid.path().as_os_str().to_owned(),
            "-".into(),
        ],
        UNFORMATTED,
    );

    assert!(!output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");

    let json = String::from_utf8_lossy(&output.stdout);

    assert!(
        json.contains("formatter_configuration_invalid_maximum_width"),
        "{output:?}"
    );

    assert!(json.contains("actual_count"), "{output:?}");
}

#[test]
fn every_configuration_failure_preserves_its_structured_diagnostic_contract() {
    let anchor = TemporaryFile::write("configuration-anchor.json", b"{}");
    let missing = anchor.path().with_extension("missing");
    let malformed = TemporaryFile::write("malformed-brayfmt.json", b"{");

    let unknown = TemporaryFile::write(
        "unknown-structured-brayfmt.json",
        br#"{"rules":{"unknown-rule":true}}"#,
    );

    let invalid = TemporaryFile::write(
        "invalid-structured-brayfmt.json",
        br#"{"maximum_line_width":0}"#,
    );

    let cases = [
        (
            missing.as_path(),
            DiagnosticKind::FormatterConfigurationReadFailed,
            &["file_path", "io_error_kind"][..],
        ),
        (
            malformed.path(),
            DiagnosticKind::FormatterConfigurationMalformed,
            &["file_path"][..],
        ),
        (
            unknown.path(),
            DiagnosticKind::FormatterConfigurationUnknownRule,
            &["file_path", "referenced_name"][..],
        ),
        (
            invalid.path(),
            DiagnosticKind::FormatterConfigurationInvalidMaximumWidth,
            &["file_path", "actual_count"][..],
        ),
    ];

    for (path, kind, expected_argument_names) in cases {
        let output = run(
            [
                OsStr::new("--format"),
                OsStr::new("json"),
                OsStr::new("--config"),
                path.as_os_str(),
                OsStr::new("-"),
            ],
            UNFORMATTED,
        );

        assert!(!output.status.success(), "{kind:?}: {output:?}");
        assert!(output.stderr.is_empty(), "{kind:?}: {output:?}");

        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|error| panic!("{kind:?} must publish JSON diagnostics: {error:?}"));

        let diagnostic = &json["diagnostics"][0];

        let argument_names = diagnostic["args"]
            .as_array()
            .unwrap_or_else(|| panic!("{kind:?} arguments must be an array"))
            .iter()
            .map(|argument| {
                argument["name"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{kind:?} argument name must be text"))
            })
            .collect::<Vec<_>>();

        assert_eq!(diagnostic["kind"], kind.as_str(), "{kind:?}: {output:?}");
        assert_eq!(argument_names, expected_argument_names);
    }
}

fn run<I, S>(arguments: I, stdin: &[u8]) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut child = Command::new(env!("CARGO_BIN_EXE_brayfmt"))
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("brayfmt command should start: {error:?}"));

    child
        .stdin
        .as_mut()
        .unwrap_or_else(|| panic!("brayfmt stdin should be piped"))
        .write_all(stdin)
        .unwrap_or_else(|error| panic!("brayfmt stdin should accept test bytes: {error:?}"));

    child
        .wait_with_output()
        .unwrap_or_else(|error| panic!("brayfmt command should finish: {error:?}"))
}

fn read(path: &std::path::Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| panic!("test source should be readable: {error:?}"))
}
