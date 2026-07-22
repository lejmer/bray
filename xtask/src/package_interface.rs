use std::fmt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bray_messages::DiagnosticRenderer;
use bray_package_interface::{
    InterfaceHeader, InterfaceInspectionRecord, InterfaceInspectionSection,
    InterfaceLanguageRevision, InterfaceSectionIndexEntry, InterfaceSectionTag,
    InterfaceValidationError, InterfaceValidationPolicy, PackageInterfaceInspection,
    ValidatedPackageInterface,
};
use serde::Serialize;

const USAGE: &str =
    "usage: cargo xtask package-interface <inspect <path> [--section <name>]... | validate <path>>";
const LANGUAGE_REVISION: InterfaceLanguageRevision = InterfaceLanguageRevision::new(0);

pub(crate) fn run(arguments: impl Iterator<Item = String>) -> ExitCode {
    match execute(arguments) {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn execute(mut arguments: impl Iterator<Item = String>) -> Result<String, CommandError> {
    let action = arguments.next().ok_or(CommandError::Usage)?;

    match action.as_str() {
        "inspect" => inspect_command(arguments),
        "validate" => validate_command(arguments),
        _ => Err(CommandError::UnexpectedAction(action)),
    }
}

fn inspect_command(mut arguments: impl Iterator<Item = String>) -> Result<String, CommandError> {
    let path = required_path(&mut arguments)?;
    let sections = parse_sections(arguments)?;
    let interface = load_interface(&path)?;
    let inspection = interface.inspect(&sections)?;
    let output = InspectionOutput::new(&path, &inspection);

    render_json(&output)
}

fn validate_command(mut arguments: impl Iterator<Item = String>) -> Result<String, CommandError> {
    let path = required_path(&mut arguments)?;

    if let Some(argument) = arguments.next() {
        return Err(CommandError::UnexpectedArgument(argument));
    }

    let interface = load_interface(&path)?;

    interface.validate_complete()?;

    render_json(&ValidationOutput::new(&path, &interface))
}

fn required_path(arguments: &mut impl Iterator<Item = String>) -> Result<PathBuf, CommandError> {
    arguments
        .next()
        .map(PathBuf::from)
        .ok_or(CommandError::Usage)
}

fn parse_sections(
    mut arguments: impl Iterator<Item = String>,
) -> Result<Vec<InterfaceSectionTag>, CommandError> {
    let mut sections = Vec::new();

    while let Some(argument) = arguments.next() {
        if argument != "--section" {
            return Err(CommandError::UnexpectedArgument(argument));
        }

        let name = arguments.next().ok_or(CommandError::MissingSection)?;
        let section =
            InterfaceSectionTag::from_name(&name).ok_or(CommandError::UnknownSection(name))?;

        sections.push(section);
    }

    if sections.is_empty() {
        sections.extend(InterfaceSectionTag::ALL);
    }

    Ok(sections)
}

fn load_interface(path: &Path) -> Result<ValidatedPackageInterface, CommandError> {
    let bytes = std::fs::read(path).map_err(|error| CommandError::Read {
        path: path.to_path_buf(),
        error,
    })?;

    ValidatedPackageInterface::try_new(bytes, InterfaceValidationPolicy::new(LANGUAGE_REVISION))
        .map_err(CommandError::Validation)
}

fn render_json(output: &impl Serialize) -> Result<String, CommandError> {
    let mut rendered = serde_json::to_string_pretty(output).map_err(CommandError::Json)?;

    rendered.push('\n');

    Ok(rendered)
}

#[derive(Serialize)]
struct InspectionOutput {
    kind: &'static str,
    path: String,
    byte_len: u64,
    section_count: usize,
    header: HeaderOutput,
    section_index: Vec<SectionIndexOutput>,
    sections: Vec<SectionOutput>,
}

impl InspectionOutput {
    fn new(path: &Path, inspection: &PackageInterfaceInspection) -> Self {
        Self {
            kind: "package_interface_inspection",
            path: path.display().to_string(),
            byte_len: inspection.byte_len(),
            section_count: inspection.section_count(),
            header: HeaderOutput::new(inspection.header()),
            section_index: inspection
                .section_index()
                .iter()
                .copied()
                .map(SectionIndexOutput::new)
                .collect(),
            sections: inspection
                .sections()
                .iter()
                .map(SectionOutput::new)
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct ValidationOutput {
    kind: &'static str,
    path: String,
    valid: bool,
    byte_len: u64,
    section_count: usize,
    header: HeaderOutput,
}

impl ValidationOutput {
    fn new(path: &Path, interface: &ValidatedPackageInterface) -> Self {
        Self {
            kind: "package_interface_validation",
            path: path.display().to_string(),
            valid: true,
            byte_len: interface.byte_len(),
            section_count: interface.section_count(),
            header: HeaderOutput::new(interface.header()),
        }
    }
}

#[derive(Serialize)]
struct HeaderOutput {
    format_revision: u16,
    language_revision: u16,
    required_flags: u64,
    content_hash: String,
    artifact_hash: String,
}

impl HeaderOutput {
    fn new(header: InterfaceHeader) -> Self {
        Self {
            format_revision: header.format_revision().raw(),
            language_revision: header.language_revision().raw(),
            required_flags: header.required_flags().bits(),
            content_hash: header.content_hash().to_string(),
            artifact_hash: header.artifact_hash().to_string(),
        }
    }
}

#[derive(Serialize)]
struct SectionIndexOutput {
    section: &'static str,
    record_count: u64,
}

impl SectionIndexOutput {
    fn new(entry: InterfaceSectionIndexEntry) -> Self {
        Self {
            section: entry.section().as_str(),
            record_count: entry.record_count(),
        }
    }
}

#[derive(Serialize)]
struct SectionOutput {
    section: &'static str,
    record_count: u64,
    records: Vec<RecordOutput>,
}

impl SectionOutput {
    fn new(section: &InterfaceInspectionSection) -> Self {
        Self {
            section: section.section().as_str(),
            record_count: section.record_count(),
            records: section
                .records()
                .iter()
                .copied()
                .map(RecordOutput::new)
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct RecordOutput {
    kind: &'static str,
    count: u64,
}

impl RecordOutput {
    fn new(record: InterfaceInspectionRecord) -> Self {
        Self {
            kind: record.kind().as_str(),
            count: record.count(),
        }
    }
}

#[derive(Debug)]
enum CommandError {
    Usage,
    UnexpectedAction(String),
    UnexpectedArgument(String),
    MissingSection,
    UnknownSection(String),
    Read {
        path: PathBuf,
        error: std::io::Error,
    },
    Validation(InterfaceValidationError),
    Json(serde_json::Error),
}

impl From<InterfaceValidationError> for CommandError {
    fn from(error: InterfaceValidationError) -> Self {
        Self::Validation(error)
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage => formatter.write_str(USAGE),
            Self::UnexpectedAction(action) => {
                write!(formatter, "unexpected package-interface command: {action}")
            }
            Self::UnexpectedArgument(argument) => {
                write!(formatter, "unexpected argument: {argument}")
            }
            Self::MissingSection => formatter.write_str("--section requires a section name"),
            Self::UnknownSection(section) => {
                write!(formatter, "unknown package-interface section: {section}")
            }
            Self::Read { path, error } => {
                write!(formatter, "failed to read {}: {error}", path.display())
            }
            Self::Validation(error) => formatter.write_str(&validation_message(*error)),
            Self::Json(error) => write!(formatter, "failed to render structured output: {error}"),
        }
    }
}

fn validation_message(error: InterfaceValidationError) -> String {
    let diagnostics = error.into_diagnostic_bag();
    let rendered = DiagnosticRenderer::english().render_bag(&diagnostics);

    let [diagnostic] = rendered.as_slice() else {
        unreachable!("one validation error always produces one diagnostic")
    };

    diagnostic.message().to_owned()
}

#[cfg(test)]
mod tests {
    use bray_package_interface::test_support::encoded_semantic_test_interface;

    use super::{CommandError, execute};

    #[test]
    fn inspect_selects_one_section_and_keeps_the_complete_known_index() {
        let path = fixture_path("inspect");
        let fixture = encoded_semantic_test_interface();

        write_fixture(&path, &fixture.bytes);

        let output = execute(
            [
                "inspect".to_owned(),
                path.display().to_string(),
                "--section".to_owned(),
                "target_dependencies".to_owned(),
            ]
            .into_iter(),
        )
        .unwrap_or_else(|error| panic!("inspection command must succeed: {error}"));

        let output: serde_json::Value = serde_json::from_str(&output)
            .unwrap_or_else(|error| panic!("inspection output must be JSON: {error}"));

        assert_eq!(output["kind"], "package_interface_inspection");
        assert_eq!(output["section_index"].as_array().map(Vec::len), Some(15));
        assert_eq!(output["sections"].as_array().map(Vec::len), Some(1));
        assert_eq!(output["sections"][0]["section"], "target_dependencies");
        assert!(output["sections"][0].get("offset").is_none());
        assert!(output["sections"][0].get("payload").is_none());

        remove_fixture(&path);
    }

    #[test]
    fn validate_requests_the_complete_interface_closure() {
        let path = fixture_path("validate");
        let fixture = encoded_semantic_test_interface();

        write_fixture(&path, &fixture.bytes);

        let output = execute(["validate".to_owned(), path.display().to_string()].into_iter())
            .unwrap_or_else(|error| panic!("validation command must succeed: {error}"));

        let output: serde_json::Value = serde_json::from_str(&output)
            .unwrap_or_else(|error| panic!("validation output must be JSON: {error}"));

        assert_eq!(output["kind"], "package_interface_validation");
        assert_eq!(output["valid"], true);
        assert_eq!(output["section_count"], 15);

        remove_fixture(&path);
    }

    #[test]
    fn malformed_artifacts_use_structured_localized_validation_errors() {
        let path = fixture_path("malformed");

        write_fixture(&path, b"not an interface");

        let error = match execute(["validate".to_owned(), path.display().to_string()].into_iter()) {
            Ok(_) => panic!("malformed validation must fail"),
            Err(error) => error,
        };

        assert!(matches!(error, CommandError::Validation(_)));
        assert_eq!(error.to_string(), "package interface is truncated");

        remove_fixture(&path);
    }

    fn fixture_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "bray-package-interface-{name}-{}.brayi",
            std::process::id()
        ))
    }

    fn write_fixture(path: &std::path::Path, bytes: &[u8]) {
        std::fs::write(path, bytes)
            .unwrap_or_else(|error| panic!("test fixture must be writable: {error}"));
    }

    fn remove_fixture(path: &std::path::Path) {
        std::fs::remove_file(path)
            .unwrap_or_else(|error| panic!("test fixture must be removable: {error}"));
    }
}
