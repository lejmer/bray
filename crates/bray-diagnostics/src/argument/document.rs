use super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates a structured document parse-failure argument.
    pub const fn document_parse_kind(kind: DiagnosticDocumentParseKind) -> Self {
        Self::new(
            DiagnosticArgName::DocumentParseKind,
            DiagnosticArgValue::DocumentParseKind(kind),
        )
    }

    /// Creates a one-based document line number.
    pub const fn document_line(line: u64) -> Self {
        Self::new(
            DiagnosticArgName::DocumentLine,
            DiagnosticArgValue::Count(line),
        )
    }

    /// Creates a one-based document column number.
    pub const fn document_column(column: u64) -> Self {
        Self::new(
            DiagnosticArgName::DocumentColumn,
            DiagnosticArgValue::Count(column),
        )
    }
}

/// Locale-neutral structured document parse category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticDocumentParseKind {
    /// Malformed document syntax.
    Syntax,
    /// Syntactically valid data does not match the required schema.
    Schema,
    /// The document ended before a value was complete.
    UnexpectedEnd,
    /// The parser's input stream failed.
    Input,
    /// A compiler-owned value could not be serialized.
    Serialization,
}

impl DiagnosticDocumentParseKind {
    /// Returns the stable machine key for this parse category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Syntax => "syntax",
            Self::Schema => "schema",
            Self::UnexpectedEnd => "unexpected_end",
            Self::Input => "input",
            Self::Serialization => "serialization",
        }
    }
}
