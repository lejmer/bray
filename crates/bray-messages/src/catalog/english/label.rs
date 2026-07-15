use bray_diagnostics::{DiagnosticArgName, DiagnosticLabelKind, DiagnosticLabelStyle};

use crate::catalog::{MessageTemplate, MessageTemplatePart};

const INVALID_UTF8_BYTES: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid UTF-8 bytes")];

const INVALID_CHARACTER: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("invalid character "),
    MessageTemplatePart::Arg(DiagnosticArgName::Character),
];

const MISPLACED_BOM: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("misplaced byte order mark")];

const LONE_CARRIAGE_RETURN: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("lone carriage return")];

const NON_ASCII_IDENTIFIER: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("non-ASCII identifier")];

const INVALID_IDENTIFIER: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid identifier")];

const INVALID_OPERATOR_OR_PUNCTUATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid operator or punctuation")];

const MALFORMED_LITERAL: &[MessageTemplatePart] = &[MessageTemplatePart::Text("malformed literal")];

const INVALID_NUMERIC_SUFFIX: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("suffix starts here")];

const UNTERMINATED_CHARACTER_LITERAL_START: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("character literal starts here")];

const UNTERMINATED_STRING_LITERAL_START: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("string literal starts here")];

const UNKNOWN_ESCAPE: &[MessageTemplatePart] = &[MessageTemplatePart::Text("unknown escape")];

const INVALID_UNICODE_ESCAPE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid Unicode escape")];

const UNTERMINATED_BLOCK_COMMENT_START: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("block comment starts here")];

const EXPECTED_TOKEN_INSERTION_POINT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("insert "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedSyntaxKind),
    MessageTemplatePart::Text(" here"),
];

const EXPECTED_EXPRESSION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("expected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedSyntaxKind),
    MessageTemplatePart::Text(" here"),
];

const UNEXPECTED_EOF: &[MessageTemplatePart] = &[MessageTemplatePart::Text("source ends here")];

const DUPLICATE_DECLARATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("duplicate declaration")];

const FIRST_DECLARATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("first declared here")];

const CONFLICTING_MODULE_DECLARATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("conflicting module declaration")];

const FIRST_MODULE_DECLARATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("module surface established here")];

pub(crate) const fn style(style: DiagnosticLabelStyle) -> &'static str {
    match style {
        DiagnosticLabelStyle::Primary => "primary source",
        DiagnosticLabelStyle::Secondary => "secondary source",
    }
}

pub(crate) const fn template(kind: DiagnosticLabelKind) -> MessageTemplate {
    match kind {
        DiagnosticLabelKind::InvalidUtf8Bytes => MessageTemplate::new(INVALID_UTF8_BYTES),
        DiagnosticLabelKind::InvalidCharacter => MessageTemplate::new(INVALID_CHARACTER),
        DiagnosticLabelKind::MisplacedBom => MessageTemplate::new(MISPLACED_BOM),
        DiagnosticLabelKind::LoneCarriageReturn => MessageTemplate::new(LONE_CARRIAGE_RETURN),
        DiagnosticLabelKind::NonAsciiIdentifier => MessageTemplate::new(NON_ASCII_IDENTIFIER),
        DiagnosticLabelKind::InvalidIdentifier => MessageTemplate::new(INVALID_IDENTIFIER),
        DiagnosticLabelKind::InvalidOperatorOrPunctuation => {
            MessageTemplate::new(INVALID_OPERATOR_OR_PUNCTUATION)
        }
        DiagnosticLabelKind::MalformedLiteral => MessageTemplate::new(MALFORMED_LITERAL),
        DiagnosticLabelKind::InvalidNumericSuffix => MessageTemplate::new(INVALID_NUMERIC_SUFFIX),
        DiagnosticLabelKind::UnterminatedCharacterLiteralStart => {
            MessageTemplate::new(UNTERMINATED_CHARACTER_LITERAL_START)
        }
        DiagnosticLabelKind::UnterminatedStringLiteralStart => {
            MessageTemplate::new(UNTERMINATED_STRING_LITERAL_START)
        }
        DiagnosticLabelKind::UnknownEscape => MessageTemplate::new(UNKNOWN_ESCAPE),
        DiagnosticLabelKind::InvalidUnicodeEscape => MessageTemplate::new(INVALID_UNICODE_ESCAPE),
        DiagnosticLabelKind::UnterminatedBlockCommentStart => {
            MessageTemplate::new(UNTERMINATED_BLOCK_COMMENT_START)
        }
        DiagnosticLabelKind::ExpectedTokenInsertionPoint => {
            MessageTemplate::new(EXPECTED_TOKEN_INSERTION_POINT)
        }
        DiagnosticLabelKind::ExpectedExpression => MessageTemplate::new(EXPECTED_EXPRESSION),
        DiagnosticLabelKind::UnexpectedEof => MessageTemplate::new(UNEXPECTED_EOF),
        DiagnosticLabelKind::DuplicateDeclaration => MessageTemplate::new(DUPLICATE_DECLARATION),
        DiagnosticLabelKind::FirstDeclaration => MessageTemplate::new(FIRST_DECLARATION),
        DiagnosticLabelKind::ConflictingModuleDeclaration => {
            MessageTemplate::new(CONFLICTING_MODULE_DECLARATION)
        }
        DiagnosticLabelKind::FirstModuleDeclaration => {
            MessageTemplate::new(FIRST_MODULE_DECLARATION)
        }
    }
}
