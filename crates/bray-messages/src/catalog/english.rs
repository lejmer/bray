use bray_diagnostics::{
    DiagnosticArgName, DiagnosticKind, DiagnosticLabelKind, DiagnosticLabelStyle,
    DiagnosticNoteKind, SeverityKind,
};

use crate::rendered_diagnostic::RenderedDiagnosticNoteKind;

use super::{MessageTemplate, MessageTemplatePart};

const SOURCE_FILE_READ_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not read source file "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const SOURCE_INVALID_UTF8: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "source input contains invalid UTF-8",
)];

const SOURCE_TOO_MANY_INPUTS: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("too many source inputs: "),
    MessageTemplatePart::Arg(DiagnosticArgName::SourceCount),
];

const SOURCE_TEXT_TOO_LARGE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("source text is too large: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ByteCount),
    MessageTemplatePart::Text(" bytes"),
];

const REQUEST_MISSING_SOURCE_INPUT: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("no source inputs were provided")];

const REQUEST_INVALID_SOURCE_INPUT: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("source input is invalid")];

const REQUEST_INVALID_WORKER_BUDGET: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "worker budget must be greater than zero",
)];

const LEXICAL_INVALID_CHARACTER: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("invalid character "),
    MessageTemplatePart::Arg(DiagnosticArgName::Character),
];

const LEXICAL_MISPLACED_BOM: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "byte order mark is not at the start of the source",
)];

const LEXICAL_LONE_CARRIAGE_RETURN: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("lone carriage return")];

const LEXICAL_NON_ASCII_IDENTIFIER: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("non-ASCII identifier character "),
    MessageTemplatePart::Arg(DiagnosticArgName::Character),
];

const LEXICAL_INVALID_IDENTIFIER: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid identifier")];

const LEXICAL_INVALID_OPERATOR_OR_PUNCTUATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid operator or punctuation")];

const LEXICAL_MALFORMED_NUMERIC_LITERAL: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("malformed numeric literal")];

const LEXICAL_INVALID_NUMERIC_SUFFIX: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid numeric literal suffix")];

const LEXICAL_MALFORMED_CHARACTER_LITERAL: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("malformed character literal")];

const LEXICAL_UNTERMINATED_CHARACTER_LITERAL: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("unterminated character literal")];

const LEXICAL_UNTERMINATED_STRING_LITERAL: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("unterminated string literal")];

const LEXICAL_UNKNOWN_ESCAPE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("unknown escape sequence")];

const LEXICAL_INVALID_UNICODE_ESCAPE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid Unicode escape")];

const LEXICAL_UNTERMINATED_BLOCK_COMMENT: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("unterminated block comment")];

const SYNTAX_EXPECTED_TOKEN: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("expected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedSyntaxKind),
];

const SYNTAX_EXPECTED_EXPRESSION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("expected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedSyntaxKind),
];

const SYNTAX_UNEXPECTED_EOF: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("unexpected end of file while expecting "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedSyntaxKind),
];

const DECLARATION_DUPLICATE_NAME: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("duplicate declaration of "),
    MessageTemplatePart::Arg(DiagnosticArgName::DeclarationName),
];

const DECLARATION_CONFLICTING_MODULE_VISIBILITY: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("module "),
    MessageTemplatePart::Arg(DiagnosticArgName::DeclarationName),
    MessageTemplatePart::Text(" has conflicting visibility: expected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedVisibility),
    MessageTemplatePart::Text(", found "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualVisibility),
];

const DECLARATION_CONFLICTING_MODULE_TRUST: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("module "),
    MessageTemplatePart::Arg(DiagnosticArgName::DeclarationName),
    MessageTemplatePart::Text(" has conflicting trust state: expected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedModuleTrust),
    MessageTemplatePart::Text(", found "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualModuleTrust),
];

const LABEL_INVALID_UTF8_BYTES: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid UTF-8 bytes")];

const LABEL_INVALID_CHARACTER: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("invalid character "),
    MessageTemplatePart::Arg(DiagnosticArgName::Character),
];

const LABEL_MISPLACED_BOM: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("misplaced byte order mark")];

const LABEL_LONE_CARRIAGE_RETURN: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("lone carriage return")];

const LABEL_NON_ASCII_IDENTIFIER: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("non-ASCII identifier")];

const LABEL_INVALID_IDENTIFIER: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid identifier")];

const LABEL_INVALID_OPERATOR_OR_PUNCTUATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid operator or punctuation")];

const LABEL_MALFORMED_LITERAL: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("malformed literal")];

const LABEL_INVALID_NUMERIC_SUFFIX: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("suffix starts here")];

const LABEL_UNTERMINATED_CHARACTER_LITERAL_START: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("character literal starts here")];

const LABEL_UNTERMINATED_STRING_LITERAL_START: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("string literal starts here")];

const LABEL_UNKNOWN_ESCAPE: &[MessageTemplatePart] = &[MessageTemplatePart::Text("unknown escape")];

const LABEL_INVALID_UNICODE_ESCAPE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid Unicode escape")];

const LABEL_UNTERMINATED_BLOCK_COMMENT_START: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("block comment starts here")];

const LABEL_EXPECTED_TOKEN_INSERTION_POINT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("insert "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedSyntaxKind),
    MessageTemplatePart::Text(" here"),
];

const LABEL_EXPECTED_EXPRESSION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("expected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedSyntaxKind),
    MessageTemplatePart::Text(" here"),
];

const LABEL_UNEXPECTED_EOF: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("source ends here")];

const LABEL_DUPLICATE_DECLARATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("duplicate declaration")];

const LABEL_FIRST_DECLARATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("first declared here")];

const LABEL_CONFLICTING_MODULE_DECLARATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("conflicting module declaration")];

const LABEL_FIRST_MODULE_DECLARATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("module surface established here")];

const NOTE_SOURCE_FILE_MUST_BE_READABLE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "source files must be readable before compilation",
)];

const NOTE_SOURCE_MUST_BE_UTF8: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "source inputs must be valid UTF-8",
)];

const NOTE_SOURCE_IDS_ARE_COMPACT: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "source IDs use compact 32-bit storage",
)];

const NOTE_SOURCE_TEXT_OFFSETS_ARE_COMPACT: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "source byte offsets use compact 32-bit storage",
)];

const NOTE_SOURCE_INPUT_REQUIRED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "provide at least one source input",
)];

const NOTE_SOURCE_INPUT_NEEDS_STABLE_IDENTITY: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "source input order must fit in a stable source identity",
    )];

const NOTE_WORKER_BUDGET_MUST_BE_POSITIVE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "use a worker count greater than zero",
)];

const NOTE_CHARACTER_NOT_ACCEPTED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "remove this character or replace it with valid Bray syntax",
)];

const NOTE_BOM_ONLY_ALLOWED_AT_START: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "a UTF-8 byte order mark is accepted only at the beginning of a source unit",
)];

const NOTE_LINE_BREAKS_MUST_BE_LF_OR_CRLF: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "line breaks must be LF or CRLF outside block comments",
)];

const NOTE_IDENTIFIERS_MUST_BE_ASCII: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "identifiers use ASCII characters",
)];

const NOTE_IDENTIFIER_SPELLING_MUST_BE_VALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text(
        "identifiers must start with an ASCII letter and continue with ASCII letters, digits, or underscores",
    ),
];

const NOTE_ONLY_IMAGINARY_NUMERIC_SUFFIX: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "numeric suffixes are not supported except imaginary i",
)];

const NOTE_CHARACTER_LITERAL_MUST_CONTAIN_ONE_SCALAR: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "character literals must contain exactly one Unicode scalar value",
    )];

const NOTE_CHARACTER_LITERAL_NEEDS_TERMINATOR: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "character literals must be closed",
    )];

const NOTE_STRING_LITERAL_NEEDS_TERMINATOR: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("string literals must be closed")];

const NOTE_ESCAPE_MUST_BE_KNOWN: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "use one of the supported escape sequences",
)];

const NOTE_UNICODE_ESCAPE_MUST_BE_SCALAR: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "Unicode escapes must have one to six hexadecimal digits and denote a Unicode scalar value",
)];

const NOTE_BLOCK_COMMENT_NEEDS_TERMINATOR: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("block comments must be closed")];

pub(super) const fn english_severity_label(severity: SeverityKind) -> &'static str {
    match severity {
        SeverityKind::Error => "error",
        SeverityKind::Warning => "warning",
        SeverityKind::Note => "note",
        SeverityKind::Help => "help",
    }
}

pub(super) const fn english_label_style(style: DiagnosticLabelStyle) -> &'static str {
    match style {
        DiagnosticLabelStyle::Primary => "primary source",
        DiagnosticLabelStyle::Secondary => "secondary source",
    }
}

pub(super) const fn english_note_kind(kind: DiagnosticNoteKind) -> RenderedDiagnosticNoteKind {
    match kind {
        DiagnosticNoteKind::SourceIdsAreCompact
        | DiagnosticNoteKind::SourceTextOffsetsAreCompact
        | DiagnosticNoteKind::BomOnlyAllowedAtStart
        | DiagnosticNoteKind::IdentifiersMustBeAscii
        | DiagnosticNoteKind::OnlyImaginaryNumericSuffix
        | DiagnosticNoteKind::CharacterLiteralMustContainOneScalar
        | DiagnosticNoteKind::UnicodeEscapeMustBeScalar => RenderedDiagnosticNoteKind::Note,
        DiagnosticNoteKind::SourceFileMustBeReadable
        | DiagnosticNoteKind::SourceMustBeUtf8
        | DiagnosticNoteKind::SourceInputRequired
        | DiagnosticNoteKind::SourceInputNeedsStableIdentity
        | DiagnosticNoteKind::WorkerBudgetMustBePositive
        | DiagnosticNoteKind::CharacterNotAccepted
        | DiagnosticNoteKind::LineBreaksMustBeLfOrCrlf
        | DiagnosticNoteKind::IdentifierSpellingMustBeValid
        | DiagnosticNoteKind::CharacterLiteralNeedsTerminator
        | DiagnosticNoteKind::StringLiteralNeedsTerminator
        | DiagnosticNoteKind::EscapeMustBeKnown
        | DiagnosticNoteKind::BlockCommentNeedsTerminator => RenderedDiagnosticNoteKind::Help,
    }
}

pub(super) const fn english_note_heading(kind: RenderedDiagnosticNoteKind) -> &'static str {
    match kind {
        RenderedDiagnosticNoteKind::Note => "note",
        RenderedDiagnosticNoteKind::Help => "help",
    }
}

pub(super) const fn english_diagnostic_template(kind: DiagnosticKind) -> MessageTemplate {
    match kind {
        DiagnosticKind::SourceFileReadFailed => MessageTemplate::new(SOURCE_FILE_READ_FAILED),
        DiagnosticKind::SourceInvalidUtf8 => MessageTemplate::new(SOURCE_INVALID_UTF8),
        DiagnosticKind::SourceTooManyInputs => MessageTemplate::new(SOURCE_TOO_MANY_INPUTS),
        DiagnosticKind::SourceTextTooLarge => MessageTemplate::new(SOURCE_TEXT_TOO_LARGE),
        DiagnosticKind::RequestMissingSourceInput => {
            MessageTemplate::new(REQUEST_MISSING_SOURCE_INPUT)
        }
        DiagnosticKind::RequestInvalidSourceInput => {
            MessageTemplate::new(REQUEST_INVALID_SOURCE_INPUT)
        }
        DiagnosticKind::RequestInvalidWorkerBudget => {
            MessageTemplate::new(REQUEST_INVALID_WORKER_BUDGET)
        }
        DiagnosticKind::LexicalInvalidCharacter => MessageTemplate::new(LEXICAL_INVALID_CHARACTER),
        DiagnosticKind::LexicalMisplacedBom => MessageTemplate::new(LEXICAL_MISPLACED_BOM),
        DiagnosticKind::LexicalLoneCarriageReturn => {
            MessageTemplate::new(LEXICAL_LONE_CARRIAGE_RETURN)
        }
        DiagnosticKind::LexicalNonAsciiIdentifier => {
            MessageTemplate::new(LEXICAL_NON_ASCII_IDENTIFIER)
        }
        DiagnosticKind::LexicalInvalidIdentifier => {
            MessageTemplate::new(LEXICAL_INVALID_IDENTIFIER)
        }
        DiagnosticKind::LexicalInvalidOperatorOrPunctuation => {
            MessageTemplate::new(LEXICAL_INVALID_OPERATOR_OR_PUNCTUATION)
        }
        DiagnosticKind::LexicalMalformedNumericLiteral => {
            MessageTemplate::new(LEXICAL_MALFORMED_NUMERIC_LITERAL)
        }
        DiagnosticKind::LexicalInvalidNumericSuffix => {
            MessageTemplate::new(LEXICAL_INVALID_NUMERIC_SUFFIX)
        }
        DiagnosticKind::LexicalMalformedCharacterLiteral => {
            MessageTemplate::new(LEXICAL_MALFORMED_CHARACTER_LITERAL)
        }
        DiagnosticKind::LexicalUnterminatedCharacterLiteral => {
            MessageTemplate::new(LEXICAL_UNTERMINATED_CHARACTER_LITERAL)
        }
        DiagnosticKind::LexicalUnterminatedStringLiteral => {
            MessageTemplate::new(LEXICAL_UNTERMINATED_STRING_LITERAL)
        }
        DiagnosticKind::LexicalUnknownEscape => MessageTemplate::new(LEXICAL_UNKNOWN_ESCAPE),
        DiagnosticKind::LexicalInvalidUnicodeEscape => {
            MessageTemplate::new(LEXICAL_INVALID_UNICODE_ESCAPE)
        }
        DiagnosticKind::LexicalUnterminatedBlockComment => {
            MessageTemplate::new(LEXICAL_UNTERMINATED_BLOCK_COMMENT)
        }
        DiagnosticKind::SyntaxExpectedToken => MessageTemplate::new(SYNTAX_EXPECTED_TOKEN),
        DiagnosticKind::SyntaxExpectedExpression => {
            MessageTemplate::new(SYNTAX_EXPECTED_EXPRESSION)
        }
        DiagnosticKind::SyntaxUnexpectedEof => MessageTemplate::new(SYNTAX_UNEXPECTED_EOF),
        DiagnosticKind::DeclarationDuplicateName => {
            MessageTemplate::new(DECLARATION_DUPLICATE_NAME)
        }
        DiagnosticKind::DeclarationConflictingModuleVisibility => {
            MessageTemplate::new(DECLARATION_CONFLICTING_MODULE_VISIBILITY)
        }
        DiagnosticKind::DeclarationConflictingModuleTrust => {
            MessageTemplate::new(DECLARATION_CONFLICTING_MODULE_TRUST)
        }
    }
}

pub(super) const fn english_label_template(kind: DiagnosticLabelKind) -> MessageTemplate {
    match kind {
        DiagnosticLabelKind::InvalidUtf8Bytes => MessageTemplate::new(LABEL_INVALID_UTF8_BYTES),
        DiagnosticLabelKind::InvalidCharacter => MessageTemplate::new(LABEL_INVALID_CHARACTER),
        DiagnosticLabelKind::MisplacedBom => MessageTemplate::new(LABEL_MISPLACED_BOM),
        DiagnosticLabelKind::LoneCarriageReturn => MessageTemplate::new(LABEL_LONE_CARRIAGE_RETURN),
        DiagnosticLabelKind::NonAsciiIdentifier => MessageTemplate::new(LABEL_NON_ASCII_IDENTIFIER),
        DiagnosticLabelKind::InvalidIdentifier => MessageTemplate::new(LABEL_INVALID_IDENTIFIER),
        DiagnosticLabelKind::InvalidOperatorOrPunctuation => {
            MessageTemplate::new(LABEL_INVALID_OPERATOR_OR_PUNCTUATION)
        }
        DiagnosticLabelKind::MalformedLiteral => MessageTemplate::new(LABEL_MALFORMED_LITERAL),
        DiagnosticLabelKind::InvalidNumericSuffix => {
            MessageTemplate::new(LABEL_INVALID_NUMERIC_SUFFIX)
        }
        DiagnosticLabelKind::UnterminatedCharacterLiteralStart => {
            MessageTemplate::new(LABEL_UNTERMINATED_CHARACTER_LITERAL_START)
        }
        DiagnosticLabelKind::UnterminatedStringLiteralStart => {
            MessageTemplate::new(LABEL_UNTERMINATED_STRING_LITERAL_START)
        }
        DiagnosticLabelKind::UnknownEscape => MessageTemplate::new(LABEL_UNKNOWN_ESCAPE),
        DiagnosticLabelKind::InvalidUnicodeEscape => {
            MessageTemplate::new(LABEL_INVALID_UNICODE_ESCAPE)
        }
        DiagnosticLabelKind::UnterminatedBlockCommentStart => {
            MessageTemplate::new(LABEL_UNTERMINATED_BLOCK_COMMENT_START)
        }
        DiagnosticLabelKind::ExpectedTokenInsertionPoint => {
            MessageTemplate::new(LABEL_EXPECTED_TOKEN_INSERTION_POINT)
        }
        DiagnosticLabelKind::ExpectedExpression => MessageTemplate::new(LABEL_EXPECTED_EXPRESSION),
        DiagnosticLabelKind::UnexpectedEof => MessageTemplate::new(LABEL_UNEXPECTED_EOF),
        DiagnosticLabelKind::DuplicateDeclaration => {
            MessageTemplate::new(LABEL_DUPLICATE_DECLARATION)
        }
        DiagnosticLabelKind::FirstDeclaration => MessageTemplate::new(LABEL_FIRST_DECLARATION),
        DiagnosticLabelKind::ConflictingModuleDeclaration => {
            MessageTemplate::new(LABEL_CONFLICTING_MODULE_DECLARATION)
        }
        DiagnosticLabelKind::FirstModuleDeclaration => {
            MessageTemplate::new(LABEL_FIRST_MODULE_DECLARATION)
        }
    }
}

pub(super) const fn english_note_template(kind: DiagnosticNoteKind) -> MessageTemplate {
    match kind {
        DiagnosticNoteKind::SourceFileMustBeReadable => {
            MessageTemplate::new(NOTE_SOURCE_FILE_MUST_BE_READABLE)
        }
        DiagnosticNoteKind::SourceMustBeUtf8 => MessageTemplate::new(NOTE_SOURCE_MUST_BE_UTF8),
        DiagnosticNoteKind::SourceIdsAreCompact => {
            MessageTemplate::new(NOTE_SOURCE_IDS_ARE_COMPACT)
        }
        DiagnosticNoteKind::SourceTextOffsetsAreCompact => {
            MessageTemplate::new(NOTE_SOURCE_TEXT_OFFSETS_ARE_COMPACT)
        }
        DiagnosticNoteKind::SourceInputRequired => MessageTemplate::new(NOTE_SOURCE_INPUT_REQUIRED),
        DiagnosticNoteKind::SourceInputNeedsStableIdentity => {
            MessageTemplate::new(NOTE_SOURCE_INPUT_NEEDS_STABLE_IDENTITY)
        }
        DiagnosticNoteKind::WorkerBudgetMustBePositive => {
            MessageTemplate::new(NOTE_WORKER_BUDGET_MUST_BE_POSITIVE)
        }
        DiagnosticNoteKind::CharacterNotAccepted => {
            MessageTemplate::new(NOTE_CHARACTER_NOT_ACCEPTED)
        }
        DiagnosticNoteKind::BomOnlyAllowedAtStart => {
            MessageTemplate::new(NOTE_BOM_ONLY_ALLOWED_AT_START)
        }
        DiagnosticNoteKind::LineBreaksMustBeLfOrCrlf => {
            MessageTemplate::new(NOTE_LINE_BREAKS_MUST_BE_LF_OR_CRLF)
        }
        DiagnosticNoteKind::IdentifiersMustBeAscii => {
            MessageTemplate::new(NOTE_IDENTIFIERS_MUST_BE_ASCII)
        }
        DiagnosticNoteKind::IdentifierSpellingMustBeValid => {
            MessageTemplate::new(NOTE_IDENTIFIER_SPELLING_MUST_BE_VALID)
        }
        DiagnosticNoteKind::OnlyImaginaryNumericSuffix => {
            MessageTemplate::new(NOTE_ONLY_IMAGINARY_NUMERIC_SUFFIX)
        }
        DiagnosticNoteKind::CharacterLiteralMustContainOneScalar => {
            MessageTemplate::new(NOTE_CHARACTER_LITERAL_MUST_CONTAIN_ONE_SCALAR)
        }
        DiagnosticNoteKind::CharacterLiteralNeedsTerminator => {
            MessageTemplate::new(NOTE_CHARACTER_LITERAL_NEEDS_TERMINATOR)
        }
        DiagnosticNoteKind::StringLiteralNeedsTerminator => {
            MessageTemplate::new(NOTE_STRING_LITERAL_NEEDS_TERMINATOR)
        }
        DiagnosticNoteKind::EscapeMustBeKnown => MessageTemplate::new(NOTE_ESCAPE_MUST_BE_KNOWN),
        DiagnosticNoteKind::UnicodeEscapeMustBeScalar => {
            MessageTemplate::new(NOTE_UNICODE_ESCAPE_MUST_BE_SCALAR)
        }
        DiagnosticNoteKind::BlockCommentNeedsTerminator => {
            MessageTemplate::new(NOTE_BLOCK_COMMENT_NEEDS_TERMINATOR)
        }
    }
}
