use std::io::{self, Write};

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArgValue, DiagnosticBag, DiagnosticLabel, DiagnosticNote,
};
use bray_source::{SourceSpan, SourceStore};
use serde::Serialize;

use super::source_map::DiagnosticSourceMap;
use crate::output_path::path_to_output_string;
use crate::source_location_output::SourceLocationOutput;
use crate::source_origin_output::SourceOriginOutput;

pub(crate) fn write_json_diagnostics(
    diagnostics: &DiagnosticBag,
    sources: Option<&SourceStore>,
    writer: &mut impl Write,
) -> io::Result<()> {
    let source_map = DiagnosticSourceMap::new(sources);
    let report = DiagnosticJsonReport::from_bag(diagnostics, &source_map);

    serde_json::to_writer_pretty(&mut *writer, &report).map_err(io::Error::other)?;

    writeln!(writer)
}

pub(crate) fn diagnostic_jsons(
    diagnostics: &DiagnosticBag,
    sources: Option<&SourceStore>,
) -> Vec<DiagnosticJson> {
    let source_map = DiagnosticSourceMap::new(sources);

    diagnostic_jsons_from_map(diagnostics, &source_map)
}

fn diagnostic_jsons_from_map(
    diagnostics: &DiagnosticBag,
    source_map: &DiagnosticSourceMap<'_>,
) -> Vec<DiagnosticJson> {
    diagnostics
        .iter()
        .map(|diagnostic| DiagnosticJson::from_diagnostic(diagnostic, source_map))
        .collect()
}

#[derive(Serialize)]
struct DiagnosticJsonReport {
    has_errors: bool,
    diagnostics: Vec<DiagnosticJson>,
}

impl DiagnosticJsonReport {
    fn from_bag(bag: &DiagnosticBag, source_map: &DiagnosticSourceMap<'_>) -> Self {
        Self {
            has_errors: bag.has_errors(),
            diagnostics: diagnostic_jsons_from_map(bag, source_map),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct DiagnosticJson {
    id: u32,
    code: u32,
    kind: &'static str,
    severity: &'static str,
    primary_span: Option<SourceSpanJson>,
    labels: Vec<DiagnosticLabelJson>,
    notes: Vec<DiagnosticNoteJson>,
    args: Vec<DiagnosticArgJson>,
}

impl DiagnosticJson {
    fn from_diagnostic(diagnostic: &Diagnostic, source_map: &DiagnosticSourceMap<'_>) -> Self {
        Self {
            id: diagnostic.id().raw(),
            code: diagnostic.kind().code().raw(),
            kind: diagnostic.kind().as_str(),
            severity: diagnostic.severity().as_str(),
            primary_span: diagnostic
                .primary_span()
                .map(|span| SourceSpanJson::from_span(span, source_map)),
            labels: diagnostic
                .labels()
                .iter()
                .map(|label| DiagnosticLabelJson::from_label(label, source_map))
                .collect(),
            notes: diagnostic
                .notes()
                .iter()
                .map(|note| DiagnosticNoteJson::from_note(note, source_map))
                .collect(),
            args: diagnostic
                .args()
                .iter()
                .map(|arg| DiagnosticArgJson::from_arg(arg, source_map))
                .collect(),
        }
    }

    pub(crate) const fn code(&self) -> u32 {
        self.code
    }

    pub(crate) const fn kind(&self) -> &'static str {
        self.kind
    }

    pub(crate) const fn severity(&self) -> &'static str {
        self.severity
    }

    pub(crate) const fn primary_span(&self) -> Option<&SourceSpanJson> {
        self.primary_span.as_ref()
    }
}

#[derive(Serialize)]
struct DiagnosticLabelJson {
    kind: &'static str,
    style: &'static str,
    span: SourceSpanJson,
    args: Vec<DiagnosticArgJson>,
}

impl DiagnosticLabelJson {
    fn from_label(label: &DiagnosticLabel, source_map: &DiagnosticSourceMap<'_>) -> Self {
        Self {
            kind: label.kind().as_str(),
            style: label.style().as_str(),
            span: SourceSpanJson::from_span(label.span(), source_map),
            args: label
                .args()
                .iter()
                .map(|arg| DiagnosticArgJson::from_arg(arg, source_map))
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct DiagnosticNoteJson {
    kind: &'static str,
    args: Vec<DiagnosticArgJson>,
}

impl DiagnosticNoteJson {
    fn from_note(note: &DiagnosticNote, source_map: &DiagnosticSourceMap<'_>) -> Self {
        Self {
            kind: note.kind().as_str(),
            args: note
                .args()
                .iter()
                .map(|arg| DiagnosticArgJson::from_arg(arg, source_map))
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct DiagnosticArgJson {
    name: &'static str,
    value: DiagnosticArgValueJson,
}

impl DiagnosticArgJson {
    fn from_arg(arg: &DiagnosticArg, source_map: &DiagnosticSourceMap<'_>) -> Self {
        Self {
            name: arg.name().as_str(),
            value: DiagnosticArgValueJson::from_value(arg.value(), source_map),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum DiagnosticArgValueJson {
    Count(u64),
    Byte(u8),
    ByteCount(u64),
    ArtifactKind(&'static str),
    ArtifactOrdinal(u32),
    Character(char),
    DeclarationName(String),
    ReferencedName(String),
    PackageIdentity(String),
    ProductIdentity(String),
    NameKind(&'static str),
    FilePath(String),
    InputIndex(u64),
    InterfaceLimit(&'static str),
    InterfaceSection(&'static str),
    IoErrorKind(&'static str),
    OutputSink(DiagnosticOutputSinkJson),
    Visibility(&'static str),
    ModuleTrust(&'static str),
    SourceName(String),
    SourceCount(u64),
    SourceInputKind(&'static str),
    SyntaxKind(&'static str),
    TextOffset(u32),
    TokenText(String),
    Uri(String),
    SourceSpan(SourceSpanJson),
    WorkerCount(u64),
    Revision(u64),
}

impl DiagnosticArgValueJson {
    fn from_value(value: &DiagnosticArgValue, source_map: &DiagnosticSourceMap<'_>) -> Self {
        match value {
            DiagnosticArgValue::Count(count) => Self::Count(*count),
            DiagnosticArgValue::Byte(byte) => Self::Byte(*byte),
            DiagnosticArgValue::ByteCount(byte_count) => Self::ByteCount(*byte_count),
            DiagnosticArgValue::ArtifactKind(kind) => Self::ArtifactKind((*kind).as_str()),
            DiagnosticArgValue::ArtifactOrdinal(ordinal) => Self::ArtifactOrdinal(*ordinal),
            DiagnosticArgValue::Character(character) => Self::Character(*character),
            DiagnosticArgValue::DeclarationName(name) => Self::DeclarationName(name.to_owned()),
            DiagnosticArgValue::ReferencedName(name) => Self::ReferencedName(name.to_owned()),
            DiagnosticArgValue::PackageIdentity(identity) => {
                Self::PackageIdentity(identity.to_owned())
            }
            DiagnosticArgValue::ProductIdentity(identity) => {
                Self::ProductIdentity(identity.to_owned())
            }
            DiagnosticArgValue::NameKind(kind) => Self::NameKind((*kind).as_str()),
            DiagnosticArgValue::FilePath(path) => Self::FilePath(path_to_output_string(path)),
            DiagnosticArgValue::InputIndex(input_index) => Self::InputIndex(*input_index),
            DiagnosticArgValue::InterfaceLimit(limit) => Self::InterfaceLimit((*limit).as_str()),
            DiagnosticArgValue::InterfaceSection(section) => {
                Self::InterfaceSection((*section).as_str())
            }
            DiagnosticArgValue::IoErrorKind(kind) => Self::IoErrorKind((*kind).as_str()),
            DiagnosticArgValue::OutputSink(sink) => {
                Self::OutputSink(DiagnosticOutputSinkJson::from_sink(sink))
            }
            DiagnosticArgValue::Visibility(visibility) => Self::Visibility((*visibility).as_str()),
            DiagnosticArgValue::ModuleTrust(trust) => Self::ModuleTrust((*trust).as_str()),
            DiagnosticArgValue::SourceName(name) => Self::SourceName(name.clone()),
            DiagnosticArgValue::SourceCount(source_count) => Self::SourceCount(*source_count),
            DiagnosticArgValue::SourceInputKind(kind) => Self::SourceInputKind((*kind).as_str()),
            DiagnosticArgValue::SyntaxKind(kind) => Self::SyntaxKind((*kind).as_str()),
            DiagnosticArgValue::TextOffset(offset) => Self::TextOffset(offset.bytes()),
            DiagnosticArgValue::TokenText(text) => Self::TokenText(text.clone()),
            DiagnosticArgValue::Uri(uri) => Self::Uri(uri.clone()),
            DiagnosticArgValue::SourceSpan(span) => {
                Self::SourceSpan(SourceSpanJson::from_span(*span, source_map))
            }
            DiagnosticArgValue::WorkerCount(worker_count) => Self::WorkerCount(*worker_count),
            DiagnosticArgValue::Revision(revision) => Self::Revision(*revision),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum DiagnosticOutputSinkJson {
    Filesystem(String),
    Memory(String),
    Stream(String),
}

impl DiagnosticOutputSinkJson {
    fn from_sink(sink: &bray_diagnostics::DiagnosticOutputSink) -> Self {
        match sink {
            bray_diagnostics::DiagnosticOutputSink::Filesystem(path) => {
                Self::Filesystem(path_to_output_string(path))
            }
            bray_diagnostics::DiagnosticOutputSink::Memory(identity) => {
                Self::Memory(identity.to_owned())
            }
            bray_diagnostics::DiagnosticOutputSink::Stream(identity) => {
                Self::Stream(identity.to_owned())
            }
        }
    }
}

#[derive(Serialize)]
pub(crate) struct SourceSpanJson {
    source_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_origin: Option<SourceOriginOutput>,
    start: u32,
    end: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<SourceLocationOutput>,
}

impl SourceSpanJson {
    fn from_span(span: SourceSpan, source_map: &DiagnosticSourceMap<'_>) -> Self {
        Self {
            source_id: span.source_id().raw(),
            source_origin: source_map.source_origin(span),
            start: span.start().bytes(),
            end: span.end().bytes(),
            location: source_map
                .resolve(span)
                .map(SourceLocationOutput::from_location),
        }
    }

    pub(crate) const fn start(&self) -> u32 {
        self.start
    }

    pub(crate) const fn end(&self) -> u32 {
        self.end
    }

    pub(crate) const fn location(&self) -> Option<SourceLocationOutput> {
        self.location
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        Diagnostic, DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticBag,
        DiagnosticId, DiagnosticInterfaceLimit, DiagnosticInterfaceSection, DiagnosticKind,
        DiagnosticModuleTrust, DiagnosticNameKind, DiagnosticNote, DiagnosticNoteKind,
        DiagnosticOutputSink, DiagnosticVisibility, SeverityKind,
    };
    use bray_source::{SourceSpan, TextRange, TextSize};
    use bray_syntax::SyntaxKind;

    use super::{DiagnosticOutputSinkJson, write_json_diagnostics};
    use crate::diagnostic_output::test_support::file_source_store;

    #[test]
    fn json_output_serializes_structured_diagnostics() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(3),
            DiagnosticKind::SourceInvalidUtf8,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::text_offset(TextSize::new(5)))
        .with_note(DiagnosticNote::new(DiagnosticNoteKind::SourceMustBeUtf8));

        let bag = DiagnosticBag::single(diagnostic);

        let mut output = Vec::new();

        match write_json_diagnostics(&bag, None, &mut output) {
            Ok(()) => {}
            Err(error) => panic!("JSON diagnostics should write: {error:?}"),
        }

        let output = match String::from_utf8(output) {
            Ok(output) => output,
            Err(error) => panic!("JSON diagnostics should be UTF-8: {error:?}"),
        };

        let output_json: serde_json::Value = match serde_json::from_str(&output) {
            Ok(value) => value,
            Err(error) => panic!("JSON diagnostics should parse: {error:?}"),
        };

        assert_eq!(output_json["has_errors"], true);

        let diagnostic_json = &output_json["diagnostics"][0];

        assert_eq!(diagnostic_json["id"], 3);
        assert_eq!(diagnostic_json["code"], 1002);
        assert_eq!(diagnostic_json["kind"], "source_invalid_utf8");
        assert_eq!(diagnostic_json["severity"], "error");

        assert!(diagnostic_json.get("message").is_none());

        let arg_json = &diagnostic_json["args"][0];

        assert_eq!(arg_json["name"], "text_offset");
        assert_eq!(arg_json["value"]["kind"], "text_offset");
        assert_eq!(arg_json["value"]["value"], 5);

        assert!(!output.contains("source input contains invalid UTF-8 at byte offset"));
    }

    #[test]
    fn json_output_preserves_typed_binding_arguments() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::BindingWrongNameKind,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::referenced_name("Size"))
        .with_arg(DiagnosticArg::expected_name_kind(DiagnosticNameKind::Type));

        let mut output = Vec::new();

        match write_json_diagnostics(&DiagnosticBag::single(diagnostic), None, &mut output) {
            Ok(()) => {}
            Err(error) => panic!("JSON diagnostics should write: {error:?}"),
        }

        let output: serde_json::Value = match serde_json::from_slice(&output) {
            Ok(value) => value,
            Err(error) => panic!("JSON diagnostics should parse: {error:?}"),
        };

        let arguments = &output["diagnostics"][0]["args"];

        assert_eq!(arguments[0]["name"], "referenced_name");
        assert_eq!(arguments[0]["value"]["kind"], "referenced_name");
        assert_eq!(arguments[0]["value"]["value"], "Size");
        assert_eq!(arguments[1]["name"], "expected_name_kind");
        assert_eq!(arguments[1]["value"]["kind"], "name_kind");
        assert_eq!(arguments[1]["value"]["value"], "type");
    }

    #[test]
    fn json_output_serializes_package_interface_diagnostic_args() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::InterfaceResourceLimitExceeded,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::new(
            DiagnosticArgName::InterfaceLimit,
            DiagnosticArgValue::InterfaceLimit(DiagnosticInterfaceLimit::RecordCount),
        ))
        .with_arg(DiagnosticArg::new(
            DiagnosticArgName::InterfaceSection,
            DiagnosticArgValue::InterfaceSection(DiagnosticInterfaceSection::Contracts),
        ))
        .with_arg(DiagnosticArg::new(
            DiagnosticArgName::ActualCount,
            DiagnosticArgValue::Count(12),
        ))
        .with_arg(DiagnosticArg::new(
            DiagnosticArgName::ExpectedRevision,
            DiagnosticArgValue::Revision(1),
        ))
        .with_note(
            DiagnosticNote::new(DiagnosticNoteKind::InterfaceDependencyContext)
                .with_arg(DiagnosticArg::expected_package_identity(
                    "example.dependency",
                ))
                .with_arg(DiagnosticArg::expected_product_identity("library"))
                .with_arg(DiagnosticArg::artifact_path("dependency.brayi")),
        );

        let bag = DiagnosticBag::single(diagnostic);

        let mut output = Vec::new();

        match write_json_diagnostics(&bag, None, &mut output) {
            Ok(()) => {}
            Err(error) => panic!("JSON diagnostics should write: {error:?}"),
        }

        let output: serde_json::Value = match serde_json::from_slice(&output) {
            Ok(value) => value,
            Err(error) => panic!("JSON diagnostics should parse: {error:?}"),
        };

        let args = &output["diagnostics"][0]["args"];

        assert_eq!(args[0]["value"]["kind"], "interface_limit");
        assert_eq!(args[0]["value"]["value"], "record_count");
        assert_eq!(args[1]["value"]["kind"], "interface_section");
        assert_eq!(args[1]["value"]["value"], "contracts");
        assert_eq!(args[2]["value"]["kind"], "count");
        assert_eq!(args[2]["value"]["value"], 12);
        assert_eq!(args[3]["value"]["kind"], "revision");
        assert_eq!(args[3]["value"]["value"], 1);

        let note_args = &output["diagnostics"][0]["notes"][0]["args"];

        assert_eq!(note_args[0]["value"]["kind"], "package_identity");
        assert_eq!(note_args[0]["value"]["value"], "example.dependency");
        assert_eq!(note_args[1]["value"]["kind"], "product_identity");
        assert_eq!(note_args[1]["value"]["value"], "library");
        assert_eq!(note_args[2]["value"]["kind"], "file_path");
        assert_eq!(note_args[2]["value"]["value"], "dependency.brayi");
    }

    #[test]
    fn json_output_serializes_syntax_diagnostic_args() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::SyntaxExpectedToken,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::expected_syntax_kind(SyntaxKind::FuncKeyword))
        .with_arg(DiagnosticArg::actual_syntax_kind(
            SyntaxKind::IdentifierToken,
        ))
        .with_arg(DiagnosticArg::token_text("main"));

        let bag = DiagnosticBag::single(diagnostic);

        let mut output = Vec::new();

        match write_json_diagnostics(&bag, None, &mut output) {
            Ok(()) => {}
            Err(error) => panic!("JSON diagnostics should write: {error:?}"),
        }

        let output = match String::from_utf8(output) {
            Ok(output) => output,
            Err(error) => panic!("JSON diagnostics should be UTF-8: {error:?}"),
        };

        let output_json: serde_json::Value = match serde_json::from_str(&output) {
            Ok(value) => value,
            Err(error) => panic!("JSON diagnostics should parse: {error:?}"),
        };

        let args = &output_json["diagnostics"][0]["args"];

        assert_eq!(
            output_json["diagnostics"][0]["kind"],
            "syntax_expected_token"
        );

        assert_eq!(output_json["diagnostics"][0]["severity"], "error");

        assert_eq!(args[0]["name"], "expected_syntax_kind");
        assert_eq!(args[0]["value"]["kind"], "syntax_kind");
        assert_eq!(args[0]["value"]["value"], "func_keyword");

        assert_eq!(args[1]["name"], "actual_syntax_kind");
        assert_eq!(args[1]["value"]["value"], "identifier_token");

        assert_eq!(args[2]["name"], "token_text");
        assert_eq!(args[2]["value"]["kind"], "token_text");
        assert_eq!(args[2]["value"]["value"], "main");
    }

    #[test]
    fn json_output_serializes_declaration_diagnostic_args() {
        let visibility = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::DeclarationConflictingModuleVisibility,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::declaration_name("core"))
        .with_arg(DiagnosticArg::expected_visibility(
            DiagnosticVisibility::Public,
        ))
        .with_arg(DiagnosticArg::actual_visibility(
            DiagnosticVisibility::Internal,
        ));

        let trust = Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::DeclarationConflictingModuleTrust,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::expected_module_trust(
            DiagnosticModuleTrust::Trusted,
        ))
        .with_arg(DiagnosticArg::actual_module_trust(
            DiagnosticModuleTrust::Ordinary,
        ));

        let bag = DiagnosticBag::from(vec![visibility, trust]);
        let mut output = Vec::new();

        match write_json_diagnostics(&bag, None, &mut output) {
            Ok(()) => {}
            Err(error) => panic!("JSON diagnostics should write: {error:?}"),
        }

        let output = match String::from_utf8(output) {
            Ok(output) => output,
            Err(error) => panic!("JSON diagnostics should be UTF-8: {error:?}"),
        };

        let output_json: serde_json::Value = match serde_json::from_str(&output) {
            Ok(value) => value,
            Err(error) => panic!("JSON diagnostics should parse: {error:?}"),
        };

        let visibility_args = &output_json["diagnostics"][0]["args"];

        assert_eq!(visibility_args[0]["name"], "declaration_name");
        assert_eq!(visibility_args[0]["value"]["kind"], "declaration_name");
        assert_eq!(visibility_args[0]["value"]["value"], "core");
        assert_eq!(visibility_args[1]["value"]["kind"], "visibility");
        assert_eq!(visibility_args[1]["value"]["value"], "public");
        assert_eq!(visibility_args[2]["value"]["value"], "internal");

        let trust_args = &output_json["diagnostics"][1]["args"];

        assert_eq!(trust_args[0]["value"]["kind"], "module_trust");
        assert_eq!(trust_args[0]["value"]["value"], "trusted");
        assert_eq!(trust_args[1]["value"]["value"], "ordinary");
    }

    #[test]
    fn json_output_serializes_typed_artifact_sinks() {
        let sink = DiagnosticOutputSink::Memory("host.output".to_owned());
        let value = DiagnosticOutputSinkJson::from_sink(&sink);

        let Ok(value) = serde_json::to_value(value) else {
            panic!("diagnostic output sink must serialize");
        };

        assert_eq!(value["kind"], "memory");
        assert_eq!(value["value"], "host.output");
    }

    #[test]
    fn json_output_serializes_resolved_source_locations() {
        let sources = file_source_store("ok\n$");

        let span = SourceSpan::new(
            bray_source::SourceId::new(0),
            TextRange::new(TextSize::new(3), TextSize::new(4)),
        );

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Error,
        )
        .with_primary_span(span);

        let bag = DiagnosticBag::single(diagnostic);

        let mut output = Vec::new();

        match write_json_diagnostics(&bag, Some(&sources), &mut output) {
            Ok(()) => {}
            Err(error) => panic!("JSON diagnostics should write: {error:?}"),
        }

        let output = match String::from_utf8(output) {
            Ok(output) => output,
            Err(error) => panic!("JSON diagnostics should be UTF-8: {error:?}"),
        };

        let output_json: serde_json::Value = match serde_json::from_str(&output) {
            Ok(value) => value,
            Err(error) => panic!("JSON diagnostics should parse: {error:?}"),
        };

        let location = &output_json["diagnostics"][0]["primary_span"]["location"];

        assert_eq!(
            output_json["diagnostics"][0]["primary_span"]["source_origin"]["kind"],
            "file"
        );

        assert_eq!(
            output_json["diagnostics"][0]["primary_span"]["source_origin"]["file_path"],
            "main.bray"
        );

        assert_eq!(location["start"]["line"], 2);
        assert_eq!(location["start"]["column"], 1);

        assert_eq!(location["end"]["line"], 2);
        assert_eq!(location["end"]["column"], 1);

        assert_eq!(location["lsp_start"]["line"], 1);
        assert_eq!(location["lsp_start"]["character"], 0);

        assert_eq!(location["lsp_end"]["line"], 1);
        assert_eq!(location["lsp_end"]["character"], 1);
    }
}
