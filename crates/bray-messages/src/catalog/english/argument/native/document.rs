use bray_diagnostics::DiagnosticDocumentParseKind;

pub(crate) const fn format_english_document_parse_kind(
    kind: DiagnosticDocumentParseKind,
) -> &'static str {
    match kind {
        DiagnosticDocumentParseKind::Syntax => "invalid JSON syntax",
        DiagnosticDocumentParseKind::Schema => "a value that does not match the required schema",
        DiagnosticDocumentParseKind::UnexpectedEnd => "an incomplete JSON value",
        DiagnosticDocumentParseKind::Input => "an input failure",
        DiagnosticDocumentParseKind::Serialization => "a serialization failure",
    }
}
