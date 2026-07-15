use bray_diagnostics::{
    DiagnosticArgValue, DiagnosticInterfaceLimit, DiagnosticInterfaceSection,
    DiagnosticIoErrorKind, DiagnosticModuleTrust, DiagnosticNameKind,
};
use bray_source::{SourceInputKind, SourceLocation, SourceOrigin, SourceSpan};
use bray_syntax::SyntaxKind;

pub(crate) fn format_value(value: &DiagnosticArgValue) -> String {
    match value {
        DiagnosticArgValue::Count(count) => count.to_string(),
        DiagnosticArgValue::Byte(byte) => format!("0x{byte:02X}"),
        DiagnosticArgValue::ByteCount(byte_count) => byte_count.to_string(),
        DiagnosticArgValue::Character(character) => format_english_character(*character),
        DiagnosticArgValue::DeclarationName(name) => format_english_quoted_text(name),
        DiagnosticArgValue::ReferencedName(name) => format_english_quoted_text(name),
        DiagnosticArgValue::NameKind(kind) => format_english_name_kind(*kind).to_owned(),
        DiagnosticArgValue::FilePath(path) => path.display().to_string(),
        DiagnosticArgValue::InputIndex(input_index) => input_index.to_string(),
        DiagnosticArgValue::InterfaceLimit(limit) => {
            format_english_interface_limit(*limit).to_owned()
        }
        DiagnosticArgValue::InterfaceSection(section) => {
            format_english_interface_section(*section).to_owned()
        }
        DiagnosticArgValue::IoErrorKind(kind) => format_english_io_error_kind(*kind).to_owned(),
        DiagnosticArgValue::Visibility(visibility) => visibility.as_str().to_owned(),
        DiagnosticArgValue::ModuleTrust(trust) => format_english_module_trust(*trust).to_owned(),
        DiagnosticArgValue::SourceName(name) => name.clone(),
        DiagnosticArgValue::SourceCount(source_count) => source_count.to_string(),
        DiagnosticArgValue::SourceInputKind(kind) => {
            format_english_source_input_kind(*kind).to_owned()
        }
        DiagnosticArgValue::SyntaxKind(kind) => format_english_syntax_kind(*kind),
        DiagnosticArgValue::TextOffset(offset) => offset.bytes().to_string(),
        DiagnosticArgValue::TokenText(text) => format_english_quoted_text(text),
        DiagnosticArgValue::Uri(uri) => uri.clone(),
        DiagnosticArgValue::SourceSpan(span) => format_source_span(*span),
        DiagnosticArgValue::WorkerCount(worker_count) => worker_count.to_string(),
        DiagnosticArgValue::Revision(revision) => revision.to_string(),
    }
}

const fn format_english_name_kind(kind: DiagnosticNameKind) -> &'static str {
    match kind {
        DiagnosticNameKind::Symbol => "symbol",
        DiagnosticNameKind::Module => "module",
        DiagnosticNameKind::Type => "type",
        DiagnosticNameKind::Trait => "trait",
        DiagnosticNameKind::Value => "value",
        DiagnosticNameKind::Pattern => "pattern",
        DiagnosticNameKind::CallableOverload => "callable overload",
        DiagnosticNameKind::Member => "member",
    }
}

const fn format_english_interface_limit(limit: DiagnosticInterfaceLimit) -> &'static str {
    match limit {
        DiagnosticInterfaceLimit::FileSize => "file size",
        DiagnosticInterfaceLimit::SectionCount => "section count",
        DiagnosticInterfaceLimit::RecordCount => "record count",
        DiagnosticInterfaceLimit::StringLength => "string length",
        DiagnosticInterfaceLimit::BlobLength => "blob length",
        DiagnosticInterfaceLimit::DecodedAllocation => "decoded allocation",
        DiagnosticInterfaceLimit::SemanticTypeDepth => "semantic type depth",
        DiagnosticInterfaceLimit::TemplateGraphSize => "template graph size",
        DiagnosticInterfaceLimit::ExternalReferenceCount => "external reference count",
    }
}

const fn format_english_interface_section(section: DiagnosticInterfaceSection) -> &'static str {
    match section {
        DiagnosticInterfaceSection::Strings => "strings",
        DiagnosticInterfaceSection::PackageMetadata => "package metadata",
        DiagnosticInterfaceSection::Dependencies => "dependencies",
        DiagnosticInterfaceSection::SymbolIdentities => "symbol identities",
        DiagnosticInterfaceSection::Relationships => "relationships",
        DiagnosticInterfaceSection::ExportedLookup => "exported lookup",
        DiagnosticInterfaceSection::SymbolFactDirectory => "symbol fact directory",
        DiagnosticInterfaceSection::SemanticTypes => "semantic types",
        DiagnosticInterfaceSection::Constants => "constants",
        DiagnosticInterfaceSection::Contracts => "contracts",
        DiagnosticInterfaceSection::DeclarationTemplates => "declaration templates",
        DiagnosticInterfaceSection::Implementations => "implementations",
        DiagnosticInterfaceSection::TargetDependencies => "target dependencies",
        DiagnosticInterfaceSection::SourceProvenance => "source provenance",
        DiagnosticInterfaceSection::SupportGraph => "support graph",
    }
}

fn format_english_character(character: char) -> String {
    if character.is_control() {
        return format!("U+{:04X}", u32::from(character));
    }

    format!("'{}'", character.escape_default())
}

fn format_english_syntax_kind(kind: SyntaxKind) -> String {
    kind.as_str().replace('_', " ")
}

fn format_english_quoted_text(text: &str) -> String {
    format!("'{}'", text.escape_default())
}

const fn format_english_module_trust(trust: DiagnosticModuleTrust) -> &'static str {
    match trust {
        DiagnosticModuleTrust::Trusted => "trusted",
        DiagnosticModuleTrust::Ordinary => "non-trusted",
    }
}

const fn format_english_io_error_kind(kind: DiagnosticIoErrorKind) -> &'static str {
    match kind {
        DiagnosticIoErrorKind::AlreadyExists => "already exists",
        DiagnosticIoErrorKind::IsDirectory => "is a directory",
        DiagnosticIoErrorKind::InvalidData => "invalid data",
        DiagnosticIoErrorKind::InvalidInput => "invalid input",
        DiagnosticIoErrorKind::Interrupted => "interrupted",
        DiagnosticIoErrorKind::NotDirectory => "not a directory",
        DiagnosticIoErrorKind::NotFound => "not found",
        DiagnosticIoErrorKind::Other => "other I/O error",
        DiagnosticIoErrorKind::PermissionDenied => "permission denied",
        DiagnosticIoErrorKind::TimedOut => "timed out",
        DiagnosticIoErrorKind::UnexpectedEof => "unexpected end of file",
        DiagnosticIoErrorKind::WouldBlock => "would block",
    }
}

const fn format_english_source_input_kind(kind: SourceInputKind) -> &'static str {
    match kind {
        SourceInputKind::File => "file",
        SourceInputKind::VirtualText => "virtual text",
        SourceInputKind::GeneratedText => "generated text",
        SourceInputKind::LspOpenDocument => "LSP open document",
    }
}

pub(crate) fn format_source_span(span: SourceSpan) -> String {
    format!(
        "source {}:{}..{}",
        span.source_id().raw(),
        span.start().bytes(),
        span.end().bytes()
    )
}

pub(crate) fn format_source_location(location: SourceLocation<'_>) -> String {
    let start = location.start();
    let end = location.end();

    format!(
        "{}:{}:{}..{}:{}",
        format_english_source_origin(location.source_id().raw(), location.origin()),
        start.line(),
        start.column(),
        end.line(),
        end.column()
    )
}

fn format_english_source_origin(source_id: u32, origin: &SourceOrigin) -> String {
    match origin {
        SourceOrigin::File { path } => path.display().to_string(),
        SourceOrigin::Virtual { name }
        | SourceOrigin::Generated { name }
        | SourceOrigin::TestFixture { name } => name.clone(),
        SourceOrigin::LspDocument { uri } => uri.clone(),
        SourceOrigin::Stdin => format!("source {source_id}"),
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticIoErrorKind,
    };
    use bray_source::{
        LineIndex, SourceId, SourceIdentity, SourceInputKind, SourceLocation, SourceOrigin,
        SourceSnapshot, SourceSpan, SourceVersion, TextRange, TextSize,
    };
    use bray_syntax::SyntaxKind;

    use crate::DiagnosticLocale;
    use crate::argument::ArgumentFormatter;

    #[test]
    fn argument_formatter_formats_representative_english_values() {
        let span = SourceSpan::new(
            SourceId::new(2),
            TextRange::new(TextSize::new(3), TextSize::new(8)),
        );

        let args = vec![
            DiagnosticArg::new(DiagnosticArgName::Byte, DiagnosticArgValue::Byte(0xff)),
            DiagnosticArg::new(
                DiagnosticArgName::Character,
                DiagnosticArgValue::Character('x'),
            ),
            DiagnosticArg::new(
                DiagnosticArgName::IoErrorKind,
                DiagnosticArgValue::IoErrorKind(DiagnosticIoErrorKind::PermissionDenied),
            ),
            DiagnosticArg::new(
                DiagnosticArgName::SourceInputKind,
                DiagnosticArgValue::SourceInputKind(SourceInputKind::GeneratedText),
            ),
            DiagnosticArg::expected_syntax_kind(SyntaxKind::FuncKeyword),
            DiagnosticArg::token_text("main\n"),
            DiagnosticArg::new(
                DiagnosticArgName::SourceSpan,
                DiagnosticArgValue::SourceSpan(span),
            ),
            DiagnosticArg::new(
                DiagnosticArgName::WorkerCount,
                DiagnosticArgValue::WorkerCount(4),
            ),
        ];

        let formatter = ArgumentFormatter::new(DiagnosticLocale::English);

        assert_eq!(
            formatter.format_named_arg(&args, DiagnosticArgName::Byte),
            "0xFF"
        );

        assert_eq!(
            formatter.format_named_arg(&args, DiagnosticArgName::Character),
            "'x'"
        );

        assert_eq!(
            formatter.format_named_arg(&args, DiagnosticArgName::IoErrorKind),
            "permission denied"
        );

        assert_eq!(
            formatter.format_named_arg(&args, DiagnosticArgName::SourceInputKind),
            "generated text"
        );

        assert_eq!(
            formatter.format_named_arg(&args, DiagnosticArgName::ExpectedSyntaxKind),
            "func keyword"
        );

        assert_eq!(
            formatter.format_named_arg(&args, DiagnosticArgName::TokenText),
            r#"'main\n'"#
        );

        assert_eq!(
            formatter.format_named_arg(&args, DiagnosticArgName::SourceSpan),
            "source 2:3..8"
        );

        assert_eq!(
            formatter.format_named_arg(&args, DiagnosticArgName::WorkerCount),
            "4"
        );
    }

    #[test]
    fn source_locations_format_as_line_column_ranges() {
        let snapshot = match SourceSnapshot::new(
            SourceId::new(0),
            SourceIdentity::new(0),
            SourceOrigin::file("main.bray"),
            SourceVersion::new(0),
            "ok\n$",
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test source should fit in TextSize: {error:?}"),
        };

        let line_index = match LineIndex::new(snapshot.text()) {
            Ok(line_index) => line_index,
            Err(error) => panic!("test source should index: {error:?}"),
        };

        let span = SourceSpan::new(
            snapshot.source_id(),
            TextRange::new(TextSize::new(3), TextSize::new(4)),
        );

        let location = match SourceLocation::resolve(&snapshot, &line_index, span) {
            Some(location) => location,
            None => panic!("test span should resolve"),
        };

        assert_eq!(
            super::format_source_location(location),
            "main.bray:2:1..2:1"
        );
    }
}
