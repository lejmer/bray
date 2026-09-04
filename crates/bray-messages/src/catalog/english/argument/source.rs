use bray_diagnostics::{
    DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm, DiagnosticArtifactKind,
    DiagnosticIoErrorKind, DiagnosticModuleTrust, DiagnosticNameKind, DiagnosticOutputSink,
};
use bray_source::{SourceInputKind, SourceLocation, SourceOrigin, SourceSpan};
use bray_syntax::SyntaxKind;
use std::fmt::Write as _;
use std::path::Path;

pub(super) fn format_english_type(ty: &bray_diagnostics::DiagnosticType) -> String {
    use bray_diagnostics::{DiagnosticType, DiagnosticTypeArgument};

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
        DiagnosticType::Named(named) => {
            let path = named.path().join(".");

            if named.arguments().is_empty() {
                return path;
            }

            let arguments = named
                .arguments()
                .iter()
                .map(|argument| match argument {
                    DiagnosticTypeArgument::Type(ty) => format_english_type(ty),
                    DiagnosticTypeArgument::Constant => String::from("<const>"),
                })
                .collect::<Vec<_>>()
                .join(", ");

            format!("{path}<{arguments}>")
        }
        DiagnosticType::Unknown => "unknown type".to_owned(),
        DiagnosticType::TypeParameter => "type parameter".to_owned(),
        DiagnosticType::ContextualSelf => "Self".to_owned(),
        DiagnosticType::TypeValuedMember => "type-valued member".to_owned(),
        DiagnosticType::Tuple(1) => "tuple type with 1 element".to_owned(),
        DiagnosticType::Tuple(count) => format!("tuple type with {count} elements"),
        DiagnosticType::Array => "array type".to_owned(),
        DiagnosticType::Slice => "slice type".to_owned(),
        DiagnosticType::Generator => "generator type".to_owned(),
        DiagnosticType::Nullable => "nullable type".to_owned(),
        DiagnosticType::Borrow => "borrow type".to_owned(),
        DiagnosticType::TraitView => "trait view type".to_owned(),
        DiagnosticType::OwnedIndirection => "owned indirection type".to_owned(),
        DiagnosticType::Callable => "callable type".to_owned(),
    }
}

pub(super) fn format_english_artifact_digest(digest: &DiagnosticArtifactDigest) -> String {
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

pub(super) const fn format_english_artifact_kind(kind: DiagnosticArtifactKind) -> &'static str {
    match kind {
        DiagnosticArtifactKind::Assembly => "assembly",
        DiagnosticArtifactKind::BackendIr => "backend IR",
        DiagnosticArtifactKind::BackendBitcode => "backend bitcode",
        DiagnosticArtifactKind::RelocatableObject => "relocatable object",
        DiagnosticArtifactKind::ExecutableModule => "executable module",
        DiagnosticArtifactKind::DebugCompanion => "debug companion",
        DiagnosticArtifactKind::PackageInterface => "package interface",
        DiagnosticArtifactKind::PackageImplementation => "package implementation",
        DiagnosticArtifactKind::DependencyMetadata => "dependency metadata",
        DiagnosticArtifactKind::TestCatalog => "test catalog",
        DiagnosticArtifactKind::Executable => "executable",
        DiagnosticArtifactKind::StaticLibrary => "static library",
        DiagnosticArtifactKind::SharedLibrary => "shared library",
        DiagnosticArtifactKind::LinkedCompanion => "linked companion",
    }
}

pub(super) fn format_english_output_sink(sink: &DiagnosticOutputSink) -> String {
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

pub(super) const fn format_english_name_kind(kind: DiagnosticNameKind) -> &'static str {
    match kind {
        DiagnosticNameKind::Symbol => "symbol",
        DiagnosticNameKind::Module => "module",
        DiagnosticNameKind::Type => "type",
        DiagnosticNameKind::Trait => "trait",
        DiagnosticNameKind::Value => "value",
        DiagnosticNameKind::Pattern => "pattern",
        DiagnosticNameKind::CallableOverload => "callable overload",
        DiagnosticNameKind::Member => "member",
        DiagnosticNameKind::TrustedCapability => "trusted capability",
    }
}

pub(super) fn format_english_character(character: char) -> String {
    if character.is_control() {
        return format!("U+{:04X}", u32::from(character));
    }

    format!("'{}'", character.escape_default())
}

pub(super) fn format_english_syntax_kind(kind: SyntaxKind) -> String {
    kind.as_str().replace('_', " ")
}

pub(super) fn format_english_syntax_kind_without_suffix(kind: SyntaxKind, suffix: &str) -> String {
    kind.as_str()
        .strip_suffix(suffix)
        .unwrap_or(kind.as_str())
        .replace('_', " ")
}

pub(super) fn format_english_quoted_text(text: &str) -> String {
    format!("'{}'", text.escape_default())
}

pub(super) fn format_english_path(path: &Path) -> String {
    path.display().to_string()
}

pub(super) const fn format_english_module_trust(trust: DiagnosticModuleTrust) -> &'static str {
    match trust {
        DiagnosticModuleTrust::Trusted => "trusted",
        DiagnosticModuleTrust::Ordinary => "non-trusted",
    }
}

pub(super) const fn format_english_io_error_kind(kind: DiagnosticIoErrorKind) -> &'static str {
    match kind {
        DiagnosticIoErrorKind::AlreadyExists => "already exists",
        DiagnosticIoErrorKind::IsDirectory => "is a directory",
        DiagnosticIoErrorKind::InvalidData => "invalid data",
        DiagnosticIoErrorKind::InvalidInput => "invalid input",
        DiagnosticIoErrorKind::Interrupted => "interrupted",
        DiagnosticIoErrorKind::NotDirectory => "not a directory",
        DiagnosticIoErrorKind::NotFound => "not found",
        DiagnosticIoErrorKind::StorageFull => "storage is full",
        DiagnosticIoErrorKind::QuotaExceeded => "storage quota exceeded",
        DiagnosticIoErrorKind::FileTooLarge => "file exceeds the supported size",
        DiagnosticIoErrorKind::ReadOnlyFilesystem => "filesystem is read-only",
        DiagnosticIoErrorKind::ResourceBusy => "resource is busy",
        DiagnosticIoErrorKind::ExecutableFileBusy => "executable file is in use",
        DiagnosticIoErrorKind::CrossesDevices => "operation crosses filesystem devices",
        DiagnosticIoErrorKind::TooManyLinks => "file has too many hard links",
        DiagnosticIoErrorKind::InvalidFilename => "filename violates filesystem requirements",
        DiagnosticIoErrorKind::WriteZero => "the write made no progress",
        DiagnosticIoErrorKind::OutOfMemory => "host memory is exhausted",
        DiagnosticIoErrorKind::NotSeekable => "resource does not support seeking",
        DiagnosticIoErrorKind::DirectoryNotEmpty => "directory is not empty",
        DiagnosticIoErrorKind::Unsupported => "filesystem does not support the operation",
        DiagnosticIoErrorKind::Other => "other I/O error",
        DiagnosticIoErrorKind::PermissionDenied => "permission denied",
        DiagnosticIoErrorKind::TimedOut => "timed out",
        DiagnosticIoErrorKind::UnexpectedEof => "unexpected end of file",
        DiagnosticIoErrorKind::WouldBlock => "would block",
    }
}

pub(super) const fn format_english_source_input_kind(kind: SourceInputKind) -> &'static str {
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
