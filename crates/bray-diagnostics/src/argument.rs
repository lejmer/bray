use std::path::PathBuf;

use bray_source::{SourceInputKind, SourceSpan, TextSize};
use bray_syntax::SyntaxKind;

/// Stable typed argument attached to a diagnostic message component.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticArg {
    name: DiagnosticArgName,
    value: DiagnosticArgValue,
}

impl DiagnosticArg {
    /// Creates a typed diagnostic argument.
    pub const fn new(name: DiagnosticArgName, value: DiagnosticArgValue) -> Self {
        Self { name, value }
    }

    /// Creates a byte-count argument from a platform byte count.
    pub fn byte_count(byte_count: usize) -> Option<Self> {
        let byte_count = u64::try_from(byte_count).ok()?;

        Some(Self::new(
            DiagnosticArgName::ByteCount,
            DiagnosticArgValue::ByteCount(byte_count),
        ))
    }

    /// Creates a file-path argument.
    pub fn file_path(path: impl Into<PathBuf>) -> Self {
        Self::new(
            DiagnosticArgName::FilePath,
            DiagnosticArgValue::FilePath(path.into()),
        )
    }

    /// Creates a zero-based input-index argument from a platform index.
    pub fn input_index(input_index: usize) -> Option<Self> {
        let input_index = u64::try_from(input_index).ok()?;

        Some(Self::new(
            DiagnosticArgName::InputIndex,
            DiagnosticArgValue::InputIndex(input_index),
        ))
    }

    /// Creates an I/O error-kind argument.
    pub const fn io_error_kind(kind: DiagnosticIoErrorKind) -> Self {
        Self::new(
            DiagnosticArgName::IoErrorKind,
            DiagnosticArgValue::IoErrorKind(kind),
        )
    }

    /// Creates a source-count argument.
    pub const fn source_count(source_count: u64) -> Self {
        Self::new(
            DiagnosticArgName::SourceCount,
            DiagnosticArgValue::SourceCount(source_count),
        )
    }

    /// Creates a source-input-kind argument.
    pub const fn source_input_kind(kind: SourceInputKind) -> Self {
        Self::new(
            DiagnosticArgName::SourceInputKind,
            DiagnosticArgValue::SourceInputKind(kind),
        )
    }

    /// Creates an expected syntax-kind argument.
    pub const fn expected_syntax_kind(kind: SyntaxKind) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedSyntaxKind,
            DiagnosticArgValue::SyntaxKind(kind),
        )
    }

    /// Creates an actual syntax-kind argument.
    pub const fn actual_syntax_kind(kind: SyntaxKind) -> Self {
        Self::new(
            DiagnosticArgName::ActualSyntaxKind,
            DiagnosticArgValue::SyntaxKind(kind),
        )
    }

    /// Creates a declaration-name argument.
    pub fn declaration_name(name: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::DeclarationName,
            DiagnosticArgValue::DeclarationName(name.into()),
        )
    }

    /// Creates an expected declaration-visibility argument.
    pub const fn expected_visibility(visibility: DiagnosticVisibility) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedVisibility,
            DiagnosticArgValue::Visibility(visibility),
        )
    }

    /// Creates an actual declaration-visibility argument.
    pub const fn actual_visibility(visibility: DiagnosticVisibility) -> Self {
        Self::new(
            DiagnosticArgName::ActualVisibility,
            DiagnosticArgValue::Visibility(visibility),
        )
    }

    /// Creates an expected module-trust argument.
    pub const fn expected_module_trust(trust: DiagnosticModuleTrust) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedModuleTrust,
            DiagnosticArgValue::ModuleTrust(trust),
        )
    }

    /// Creates an actual module-trust argument.
    pub const fn actual_module_trust(trust: DiagnosticModuleTrust) -> Self {
        Self::new(
            DiagnosticArgName::ActualModuleTrust,
            DiagnosticArgValue::ModuleTrust(trust),
        )
    }

    /// Creates a source-name argument.
    pub fn source_name(name: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::SourceName,
            DiagnosticArgValue::SourceName(name.into()),
        )
    }

    /// Creates a source text-offset argument.
    pub const fn text_offset(offset: TextSize) -> Self {
        Self::new(
            DiagnosticArgName::TextOffset,
            DiagnosticArgValue::TextOffset(offset),
        )
    }

    /// Creates an exact source-token text argument.
    pub fn token_text(text: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::TokenText,
            DiagnosticArgValue::TokenText(text.into()),
        )
    }

    /// Creates a URI argument.
    pub fn uri(uri: impl Into<String>) -> Self {
        Self::new(DiagnosticArgName::Uri, DiagnosticArgValue::Uri(uri.into()))
    }

    /// Creates a worker-count argument.
    pub const fn worker_count(worker_count: u64) -> Self {
        Self::new(
            DiagnosticArgName::WorkerCount,
            DiagnosticArgValue::WorkerCount(worker_count),
        )
    }

    /// Returns the stable argument name.
    pub const fn name(&self) -> DiagnosticArgName {
        self.name
    }

    /// Returns the typed argument value.
    pub const fn value(&self) -> &DiagnosticArgValue {
        &self.value
    }
}

/// Stable name for a diagnostic argument.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticArgName {
    /// Byte that participates in the diagnostic.
    Byte,
    /// Number of bytes that participate in the diagnostic.
    ByteCount,
    /// Character that participates in the diagnostic.
    Character,
    /// Start location of a block comment or another paired source construct.
    ConstructStart,
    /// Source-level declaration name.
    DeclarationName,
    /// Syntax kind that was present in source.
    ActualSyntaxKind,
    /// Syntax kind that was expected by the compiler phase.
    ExpectedSyntaxKind,
    /// Effective visibility established by an earlier declaration.
    ExpectedVisibility,
    /// Effective visibility supplied by the current declaration.
    ActualVisibility,
    /// Module trust state established by an earlier declaration.
    ExpectedModuleTrust,
    /// Module trust state supplied by the current declaration.
    ActualModuleTrust,
    /// Path of a source file or external artifact.
    FilePath,
    /// Zero-based source input index from the request boundary.
    InputIndex,
    /// Stable I/O error category from the host.
    IoErrorKind,
    /// Name of a virtual, generated, or test-fixture source.
    SourceName,
    /// Number of source inputs involved in the diagnostic.
    SourceCount,
    /// Stable source input category.
    SourceInputKind,
    /// Byte offset inside a source input.
    TextOffset,
    /// Exact token source text that participates in the diagnostic.
    TokenText,
    /// URI of an LSP or other URI-backed source input.
    Uri,
    /// Source span that participates in the diagnostic.
    SourceSpan,
    /// Worker count requested at the driver or compilation boundary.
    WorkerCount,
}

impl DiagnosticArgName {
    /// Returns the stable machine key for this argument name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Byte => "byte",
            Self::ByteCount => "byte_count",
            Self::Character => "character",
            Self::ConstructStart => "construct_start",
            Self::DeclarationName => "declaration_name",
            Self::ActualSyntaxKind => "actual_syntax_kind",
            Self::ExpectedSyntaxKind => "expected_syntax_kind",
            Self::ExpectedVisibility => "expected_visibility",
            Self::ActualVisibility => "actual_visibility",
            Self::ExpectedModuleTrust => "expected_module_trust",
            Self::ActualModuleTrust => "actual_module_trust",
            Self::FilePath => "file_path",
            Self::InputIndex => "input_index",
            Self::IoErrorKind => "io_error_kind",
            Self::SourceName => "source_name",
            Self::SourceCount => "source_count",
            Self::SourceInputKind => "source_input_kind",
            Self::TextOffset => "text_offset",
            Self::TokenText => "token_text",
            Self::Uri => "uri",
            Self::SourceSpan => "source_span",
            Self::WorkerCount => "worker_count",
        }
    }
}

/// Locale-neutral typed value for a diagnostic argument.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticArgValue {
    /// Raw source byte.
    Byte(u8),
    /// Source byte count.
    ByteCount(u64),
    /// Source character.
    Character(char),
    /// Source-level declaration name.
    DeclarationName(String),
    /// Source file or external artifact path.
    FilePath(PathBuf),
    /// Zero-based source input index.
    InputIndex(u64),
    /// Stable I/O error category from the host.
    IoErrorKind(DiagnosticIoErrorKind),
    /// Effective declaration visibility.
    Visibility(DiagnosticVisibility),
    /// Effective module trust state.
    ModuleTrust(DiagnosticModuleTrust),
    /// Source name.
    SourceName(String),
    /// Source input count.
    SourceCount(u64),
    /// Stable source input category.
    SourceInputKind(SourceInputKind),
    /// Syntax vocabulary kind.
    SyntaxKind(SyntaxKind),
    /// Byte offset inside source text.
    TextOffset(TextSize),
    /// Exact token source text.
    TokenText(String),
    /// URI string.
    Uri(String),
    /// Source span.
    SourceSpan(SourceSpan),
    /// Requested worker count.
    WorkerCount(u64),
}

/// Locale-neutral effective visibility used by declaration diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticVisibility {
    /// Public declaration visibility.
    Public,
    /// Internal declaration visibility.
    Internal,
}

impl DiagnosticVisibility {
    /// Returns the stable machine key for this visibility.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
        }
    }
}

/// Locale-neutral module trust state used by declaration diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticModuleTrust {
    /// Module permits trusted declarations.
    Trusted,
    /// Module does not permit trusted declarations.
    Ordinary,
}

impl DiagnosticModuleTrust {
    /// Returns the stable machine key for this trust state.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trusted => "trusted",
            Self::Ordinary => "ordinary",
        }
    }
}

/// Stable subset of host I/O error categories used in diagnostics.
///
/// This avoids storing localized or platform-specific error text in compiler
/// diagnostics while still preserving the relevant failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticIoErrorKind {
    /// A path or resource already exists.
    AlreadyExists,
    /// A path names a directory where a file was expected.
    IsDirectory,
    /// Input data was not accepted by the host API.
    InvalidData,
    /// An input argument was not accepted by the host API.
    InvalidInput,
    /// An operation was interrupted.
    Interrupted,
    /// A path component was not a directory.
    NotDirectory,
    /// A path or resource was not found.
    NotFound,
    /// Any I/O error category not yet modeled explicitly.
    Other,
    /// The host denied access to the resource.
    PermissionDenied,
    /// The operation timed out.
    TimedOut,
    /// The input ended unexpectedly.
    UnexpectedEof,
    /// The operation would have blocked.
    WouldBlock,
}

impl DiagnosticIoErrorKind {
    /// Returns the stable machine key for this I/O error category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AlreadyExists => "already_exists",
            Self::IsDirectory => "is_directory",
            Self::InvalidData => "invalid_data",
            Self::InvalidInput => "invalid_input",
            Self::Interrupted => "interrupted",
            Self::NotDirectory => "not_directory",
            Self::NotFound => "not_found",
            Self::Other => "other",
            Self::PermissionDenied => "permission_denied",
            Self::TimedOut => "timed_out",
            Self::UnexpectedEof => "unexpected_eof",
            Self::WouldBlock => "would_block",
        }
    }
}

impl From<std::io::ErrorKind> for DiagnosticIoErrorKind {
    fn from(kind: std::io::ErrorKind) -> Self {
        match kind {
            std::io::ErrorKind::AlreadyExists => Self::AlreadyExists,
            std::io::ErrorKind::IsADirectory => Self::IsDirectory,
            std::io::ErrorKind::InvalidData => Self::InvalidData,
            std::io::ErrorKind::InvalidInput => Self::InvalidInput,
            std::io::ErrorKind::Interrupted => Self::Interrupted,
            std::io::ErrorKind::NotADirectory => Self::NotDirectory,
            std::io::ErrorKind::NotFound => Self::NotFound,
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            std::io::ErrorKind::TimedOut => Self::TimedOut,
            std::io::ErrorKind::UnexpectedEof => Self::UnexpectedEof,
            std::io::ErrorKind::WouldBlock => Self::WouldBlock,
            _ => Self::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use bray_syntax::SyntaxKind;

    use super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticIoErrorKind};

    #[test]
    fn diagnostic_args_pair_stable_names_with_typed_values() {
        let arg = DiagnosticArg::new(
            DiagnosticArgName::Character,
            DiagnosticArgValue::Character('\u{0}'),
        );

        assert_eq!(arg.name(), DiagnosticArgName::Character);
        assert_eq!(arg.name().as_str(), "character");
        assert_eq!(arg.value(), &DiagnosticArgValue::Character('\u{0}'));
    }

    #[test]
    fn syntax_args_keep_syntax_kind_values_typed() {
        let arg = DiagnosticArg::expected_syntax_kind(SyntaxKind::FuncKeyword);

        assert_eq!(arg.name(), DiagnosticArgName::ExpectedSyntaxKind);
        assert_eq!(arg.name().as_str(), "expected_syntax_kind");

        assert_eq!(
            arg.value(),
            &DiagnosticArgValue::SyntaxKind(SyntaxKind::FuncKeyword)
        );
    }

    #[test]
    fn declaration_args_keep_names_and_module_surface_values_typed() {
        let name = DiagnosticArg::declaration_name("Point");
        let visibility = DiagnosticArg::expected_visibility(super::DiagnosticVisibility::Public);
        let trust = DiagnosticArg::actual_module_trust(super::DiagnosticModuleTrust::Ordinary);

        assert_eq!(name.name(), DiagnosticArgName::DeclarationName);
        assert_eq!(name.name().as_str(), "declaration_name");

        assert_eq!(
            name.value(),
            &DiagnosticArgValue::DeclarationName(String::from("Point"))
        );

        assert_eq!(
            visibility.value(),
            &DiagnosticArgValue::Visibility(super::DiagnosticVisibility::Public)
        );

        assert_eq!(
            trust.value(),
            &DiagnosticArgValue::ModuleTrust(super::DiagnosticModuleTrust::Ordinary)
        );

        assert_eq!(super::DiagnosticVisibility::Internal.as_str(), "internal");
        assert_eq!(super::DiagnosticModuleTrust::Trusted.as_str(), "trusted");
    }

    #[test]
    fn diagnostic_io_error_kinds_keep_stable_categories() {
        assert_eq!(
            DiagnosticIoErrorKind::from(ErrorKind::NotFound),
            DiagnosticIoErrorKind::NotFound
        );
        assert_eq!(
            DiagnosticIoErrorKind::from(ErrorKind::ConnectionReset),
            DiagnosticIoErrorKind::Other
        );
        assert_eq!(DiagnosticIoErrorKind::NotFound.as_str(), "not_found");
    }
}
