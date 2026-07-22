use std::fmt::Write as _;

use bray_diagnostics::{
    DiagnosticAlignmentKind, DiagnosticArgValue, DiagnosticArtifactDigest,
    DiagnosticArtifactDigestAlgorithm, DiagnosticArtifactKind, DiagnosticCallableAbi,
    DiagnosticInterfaceLimit, DiagnosticInterfaceSection, DiagnosticIoErrorKind,
    DiagnosticModuleTrust, DiagnosticNameKind, DiagnosticOutputSink,
    DiagnosticTargetRepresentation,
};
use bray_source::{SourceInputKind, SourceLocation, SourceOrigin, SourceSpan};
use bray_syntax::SyntaxKind;

pub(crate) fn format_value(value: &DiagnosticArgValue) -> String {
    match value {
        DiagnosticArgValue::Count(count) => count.to_string(),
        DiagnosticArgValue::Byte(byte) => format!("0x{byte:02X}"),
        DiagnosticArgValue::ByteCount(byte_count) => byte_count.to_string(),
        DiagnosticArgValue::ArtifactDigest(digest) => format_english_artifact_digest(digest),
        DiagnosticArgValue::ArtifactKind(kind) => format_english_artifact_kind(*kind).to_owned(),
        DiagnosticArgValue::ArtifactOrdinal(ordinal) => ordinal.to_string(),
        DiagnosticArgValue::TargetRepresentation(kind) => {
            format_english_target_representation(*kind).to_owned()
        }
        DiagnosticArgValue::CallableAbi(abi) => format_english_callable_abi(*abi).to_owned(),
        DiagnosticArgValue::AlignmentKind(kind) => format_english_alignment_kind(*kind).to_owned(),
        DiagnosticArgValue::Character(character) => format_english_character(*character),
        DiagnosticArgValue::DeclarationName(name) => format_english_quoted_text(name),
        DiagnosticArgValue::ReferencedName(name) => format_english_quoted_text(name),
        DiagnosticArgValue::PackageIdentity(identity) => format_english_quoted_text(identity),
        DiagnosticArgValue::ProductIdentity(identity) => format_english_quoted_text(identity),
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
        DiagnosticArgValue::OutputSink(sink) => format_english_output_sink(sink),
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
        DiagnosticArgValue::Type(ty) => format_english_type(*ty),
        DiagnosticArgValue::SelectionKind(kind) => format_english_selection_kind(*kind).to_owned(),
    }
}

const fn format_english_callable_abi(abi: DiagnosticCallableAbi) -> &'static str {
    match abi {
        DiagnosticCallableAbi::C => "C",
        DiagnosticCallableAbi::System => "system",
    }
}

const fn format_english_alignment_kind(kind: DiagnosticAlignmentKind) -> &'static str {
    match kind {
        DiagnosticAlignmentKind::Storage => "storage",
        DiagnosticAlignmentKind::Allocation => "allocation",
        DiagnosticAlignmentKind::CallableAbi => "callable ABI",
    }
}

const fn format_english_target_representation(
    kind: DiagnosticTargetRepresentation,
) -> &'static str {
    match kind {
        DiagnosticTargetRepresentation::Bool => "bool",
        DiagnosticTargetRepresentation::Char => "char",
        DiagnosticTargetRepresentation::I8 => "i8",
        DiagnosticTargetRepresentation::I16 => "i16",
        DiagnosticTargetRepresentation::I32 => "i32",
        DiagnosticTargetRepresentation::I64 => "i64",
        DiagnosticTargetRepresentation::I128 => "i128",
        DiagnosticTargetRepresentation::U8 => "u8",
        DiagnosticTargetRepresentation::U16 => "u16",
        DiagnosticTargetRepresentation::U32 => "u32",
        DiagnosticTargetRepresentation::U64 => "u64",
        DiagnosticTargetRepresentation::U128 => "u128",
        DiagnosticTargetRepresentation::Isize => "isize",
        DiagnosticTargetRepresentation::Usize => "usize",
        DiagnosticTargetRepresentation::R16 => "r16",
        DiagnosticTargetRepresentation::R32 => "r32",
        DiagnosticTargetRepresentation::R64 => "r64",
        DiagnosticTargetRepresentation::R128 => "r128",
        DiagnosticTargetRepresentation::C32 => "c32",
        DiagnosticTargetRepresentation::C64 => "c64",
        DiagnosticTargetRepresentation::C128 => "c128",
        DiagnosticTargetRepresentation::C256 => "c256",
        DiagnosticTargetRepresentation::RawPointer => "raw pointer",
        DiagnosticTargetRepresentation::AbiQualifiedCallable => "ABI-qualified callable",
        DiagnosticTargetRepresentation::DefaultLayoutAggregate => "default-layout aggregate",
        DiagnosticTargetRepresentation::StableLayoutAggregate => "stable-layout aggregate",
        DiagnosticTargetRepresentation::CLayoutAggregate => "C-layout aggregate",
        DiagnosticTargetRepresentation::TransparentLayoutAggregate => {
            "transparent-layout aggregate"
        }
    }
}

const fn format_english_selection_kind(
    kind: bray_diagnostics::DiagnosticSelectionKind,
) -> &'static str {
    use bray_diagnostics::DiagnosticSelectionKind;

    match kind {
        DiagnosticSelectionKind::Callable => "callable",
        DiagnosticSelectionKind::Member => "member",
        DiagnosticSelectionKind::Operator => "operator",
        DiagnosticSelectionKind::Index => "index operation",
        DiagnosticSelectionKind::Construction => "construction operation",
        DiagnosticSelectionKind::Conversion => "conversion",
        DiagnosticSelectionKind::Implementation => "implementation",
    }
}

fn format_english_type(ty: bray_diagnostics::DiagnosticType) -> String {
    use bray_diagnostics::DiagnosticType;

    match ty {
        DiagnosticType::Error => "error type".to_owned(),
        DiagnosticType::Boolean => "bool".to_owned(),
        DiagnosticType::Character => "char".to_owned(),
        DiagnosticType::I8 => "i8".to_owned(),
        DiagnosticType::I16 => "i16".to_owned(),
        DiagnosticType::I32 => "i32".to_owned(),
        DiagnosticType::I64 => "i64".to_owned(),
        DiagnosticType::I128 => "i128".to_owned(),
        DiagnosticType::U8 => "u8".to_owned(),
        DiagnosticType::U16 => "u16".to_owned(),
        DiagnosticType::U32 => "u32".to_owned(),
        DiagnosticType::U64 => "u64".to_owned(),
        DiagnosticType::U128 => "u128".to_owned(),
        DiagnosticType::Isize => "isize".to_owned(),
        DiagnosticType::Usize => "usize".to_owned(),
        DiagnosticType::Unit => "unit".to_owned(),
        DiagnosticType::Never => "never".to_owned(),
        DiagnosticType::String => "string".to_owned(),
        DiagnosticType::R16 => "r16".to_owned(),
        DiagnosticType::R32 => "r32".to_owned(),
        DiagnosticType::R64 => "r64".to_owned(),
        DiagnosticType::R128 => "r128".to_owned(),
        DiagnosticType::C32 => "c32".to_owned(),
        DiagnosticType::C64 => "c64".to_owned(),
        DiagnosticType::C128 => "c128".to_owned(),
        DiagnosticType::C256 => "c256".to_owned(),
        DiagnosticType::Named => "named type".to_owned(),
        DiagnosticType::TypeParameter => "type parameter".to_owned(),
        DiagnosticType::ContextualSelf => "Self".to_owned(),
        DiagnosticType::AssociatedType => "associated type".to_owned(),
        DiagnosticType::Tuple(1) => "tuple type with 1 element".to_owned(),
        DiagnosticType::Tuple(count) => format!("tuple type with {count} elements"),
        DiagnosticType::Array => "array type".to_owned(),
        DiagnosticType::Slice => "slice type".to_owned(),
        DiagnosticType::Nullable => "nullable type".to_owned(),
        DiagnosticType::Borrow => "borrow type".to_owned(),
        DiagnosticType::TraitView => "trait view type".to_owned(),
        DiagnosticType::OwnedIndirection => "owned indirection type".to_owned(),
        DiagnosticType::Callable => "callable type".to_owned(),
    }
}

fn format_english_artifact_digest(digest: &DiagnosticArtifactDigest) -> String {
    let algorithm = match digest.algorithm() {
        DiagnosticArtifactDigestAlgorithm::Blake3 => "BLAKE3",
        DiagnosticArtifactDigestAlgorithm::Sha256 => "SHA-256",
    };

    let mut formatted = String::with_capacity(algorithm.len() + 1 + digest.bytes().len() * 2);

    formatted.push_str(algorithm);
    formatted.push(' ');

    for byte in digest.bytes() {
        let _ = write!(formatted, "{byte:02x}");
    }

    formatted
}

const fn format_english_artifact_kind(kind: DiagnosticArtifactKind) -> &'static str {
    match kind {
        DiagnosticArtifactKind::Assembly => "assembly",
        DiagnosticArtifactKind::BackendIr => "backend IR",
        DiagnosticArtifactKind::BackendBitcode => "backend bitcode",
        DiagnosticArtifactKind::RelocatableObject => "relocatable object",
        DiagnosticArtifactKind::ExecutableModule => "executable module",
        DiagnosticArtifactKind::DebugCompanion => "debug companion",
        DiagnosticArtifactKind::PackageInterface => "package interface",
        DiagnosticArtifactKind::DependencyMetadata => "dependency metadata",
        DiagnosticArtifactKind::Executable => "executable",
        DiagnosticArtifactKind::StaticLibrary => "static library",
        DiagnosticArtifactKind::SharedLibrary => "shared library",
        DiagnosticArtifactKind::LinkedCompanion => "linked companion",
    }
}

fn format_english_output_sink(sink: &DiagnosticOutputSink) -> String {
    match sink {
        DiagnosticOutputSink::Filesystem(path) => path.display().to_string(),
        DiagnosticOutputSink::Memory(identity) => {
            format!("memory collector {}", format_english_quoted_text(identity))
        }
        DiagnosticOutputSink::Stream(identity) => {
            format!("stream {}", format_english_quoted_text(identity))
        }
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
