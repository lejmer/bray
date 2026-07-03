use bray_diagnostics::{
    DiagnosticArgName, DiagnosticKind, DiagnosticLabelKind, DiagnosticLabelStyle,
    DiagnosticNoteKind, SeverityKind,
};

use super::{MessageTemplate, MessageTemplatePart};

const SOURCE_FILE_READ_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not read source file "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const SOURCE_INVALID_UTF8: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("source input contains invalid UTF-8 at byte offset "),
    MessageTemplatePart::Arg(DiagnosticArgName::TextOffset),
];

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

const LEXICAL_UNTERMINATED_BLOCK_COMMENT: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("unterminated block comment")];

const LABEL_INVALID_UTF8_BYTES: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid UTF-8 bytes")];

const LABEL_INVALID_CHARACTER: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("invalid character "),
    MessageTemplatePart::Arg(DiagnosticArgName::Character),
];

const LABEL_UNTERMINATED_BLOCK_COMMENT_START: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("block comment starts here")];

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
    "this character is not accepted by the lexer",
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
        DiagnosticLabelStyle::Primary => "primary",
        DiagnosticLabelStyle::Secondary => "secondary",
    }
}

pub(super) const fn english_note_heading() -> &'static str {
    "note"
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
        DiagnosticKind::LexicalUnterminatedBlockComment => {
            MessageTemplate::new(LEXICAL_UNTERMINATED_BLOCK_COMMENT)
        }
    }
}

pub(super) const fn english_label_template(kind: DiagnosticLabelKind) -> MessageTemplate {
    match kind {
        DiagnosticLabelKind::InvalidUtf8Bytes => MessageTemplate::new(LABEL_INVALID_UTF8_BYTES),
        DiagnosticLabelKind::InvalidCharacter => MessageTemplate::new(LABEL_INVALID_CHARACTER),
        DiagnosticLabelKind::UnterminatedBlockCommentStart => {
            MessageTemplate::new(LABEL_UNTERMINATED_BLOCK_COMMENT_START)
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
        DiagnosticNoteKind::BlockCommentNeedsTerminator => {
            MessageTemplate::new(NOTE_BLOCK_COMMENT_NEEDS_TERMINATOR)
        }
    }
}
