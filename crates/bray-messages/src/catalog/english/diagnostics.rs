use bray_diagnostics::{DiagnosticArgName, DiagnosticKind, DiagnosticNoteKind, SeverityKind};

use crate::catalog::{MessageTemplate, MessageTemplatePart};
use crate::rendered_diagnostic::RenderedDiagnosticNoteKind;

use super::interface::diagnostic_template as interface_diagnostic_template;

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

const REQUEST_DUPLICATE_SOURCE_INPUT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("source input "),
    MessageTemplatePart::Arg(DiagnosticArgName::InputIndex),
    MessageTemplatePart::Text(" selects a source that is already present"),
];

const REQUEST_INVALID_WORKER_BUDGET: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "worker budget must be greater than zero",
)];

const CHECKING_NO_APPLICABLE_CANDIDATE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("no applicable "),
    MessageTemplatePart::Arg(DiagnosticArgName::SelectionKind),
    MessageTemplatePart::Text(" candidate"),
];

const CHECKING_AMBIGUOUS_CANDIDATE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::SelectionKind),
    MessageTemplatePart::Text(" selection is ambiguous"),
];

const CHECKING_INACCESSIBLE_CANDIDATE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("matching "),
    MessageTemplatePart::Arg(DiagnosticArgName::SelectionKind),
    MessageTemplatePart::Text(" candidate is inaccessible"),
];

const CHECKING_INCOMPATIBLE_CANDIDATE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::SelectionKind),
    MessageTemplatePart::Text(" candidate is incompatible with the supplied expressions"),
];

const CHECKING_TARGET_REPRESENTATION_UNAVAILABLE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected target does not provide the required "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetRepresentation),
    MessageTemplatePart::Text(" representation"),
];

const CHECKING_TARGET_CALLABLE_ABI_UNAVAILABLE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected target does not provide the required "),
    MessageTemplatePart::Arg(DiagnosticArgName::CallableAbi),
    MessageTemplatePart::Text(" callable ABI"),
];

const CHECKING_TARGET_ALIGNMENT_UNSUPPORTED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("required "),
    MessageTemplatePart::Arg(DiagnosticArgName::AlignmentKind),
    MessageTemplatePart::Text(" alignment "),
    MessageTemplatePart::Arg(DiagnosticArgName::RequiredAlignment),
    MessageTemplatePart::Text(" exceeds the selected target maximum of "),
    MessageTemplatePart::Arg(DiagnosticArgName::MaximumAlignment),
];

const CHECKING_TARGET_ABI_REPRESENTATION_UNSUPPORTED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("the selected "),
    MessageTemplatePart::Arg(DiagnosticArgName::CallableAbi),
    MessageTemplatePart::Text(" callable ABI does not accept "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetRepresentation),
    MessageTemplatePart::Text(" values by value"),
];

const BINDING_INVALID_CALLABLE_ABI: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid callable ABI directive")];

const BINDING_DUPLICATE_CALLABLE_ABI: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "callable ABI directive is repeated",
)];

const BINDING_PREDICATE_BODY_REQUIRED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "ordinary predicate declaration requires a body",
)];

const BINDING_TRUSTED_PREDICATE_BODY_NOT_ALLOWED: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "trusted predicate declaration cannot have a body",
    )];

const BINDING_CYCLIC_MODULE_EXPORT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("module export path through "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" forms a cycle"),
];

const BINDING_CONFLICTING_MODULE_EXPORT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("module export name "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" conflicts with another declaration"),
];

const BINDING_INVALID_MODULE_EXPORT_TARGET: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("name "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" cannot be re-exported"),
];

const BINDING_MALFORMED_DIRECTIVE_ARGUMENT: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("directive argument is malformed")];

const CHECKING_DUPLICATE_MODULE_CONTRIBUTION_DIRECTIVE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("duplicate "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualSyntaxKind),
];

const EMISSION_MISSING_CONTRIBUTION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("missing required "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact contribution"),
];

const EMISSION_INVALID_CONTRIBUTION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact contribution does not match the emission plan"),
];

const EMISSION_ARTIFACT_READ_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not read "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact content: "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const EMISSION_ARTIFACT_OPEN_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not open artifact output "),
    MessageTemplatePart::Arg(DiagnosticArgName::OutputSink),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const EMISSION_ARTIFACT_WRITE_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not write artifact output "),
    MessageTemplatePart::Arg(DiagnosticArgName::OutputSink),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const EMISSION_ARTIFACT_FLUSH_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not flush artifact output "),
    MessageTemplatePart::Arg(DiagnosticArgName::OutputSink),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const EMISSION_ARTIFACT_COMMIT_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not commit artifact output "),
    MessageTemplatePart::Arg(DiagnosticArgName::OutputSink),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const EMISSION_ARTIFACT_DIGEST_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact declared digest "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedArtifactDigest),
    MessageTemplatePart::Text(", but content digest was "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualArtifactDigest),
];

const EMISSION_ARTIFACT_LENGTH_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact declared "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedByteCount),
    MessageTemplatePart::Text(" bytes, but content contained "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualByteCount),
    MessageTemplatePart::Text(" bytes"),
];

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

const BINDING_UNRESOLVED_NAME: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not resolve "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
];

const BINDING_NAME_ALREADY_DEFINED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("name is already defined: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
];

const BINDING_INCOHERENT_ALTERNATIVE_PATTERN: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "alternative patterns must bind the same names",
    )];

const CHECKING_INCOMPATIBLE_EXPRESSION_TYPE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("expected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedType),
    MessageTemplatePart::Text(", but found "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualType),
];

const CHECKING_INCOMPATIBLE_PATTERN: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("pattern is incompatible with "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualType),
];

const CHECKING_REFUTABLE_PATTERN: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("pattern must be irrefutable")];

const CHECKING_NON_EXHAUSTIVE_MATCH: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "match does not cover every possible value",
)];

const CHECKING_UNREACHABLE_MATCH_ARM: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("match arm is unreachable")];

const CHECKING_CANNOT_INFER_EXPRESSION_TYPE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("cannot infer expression type")];

const CHECKING_INVALID_CONSTANT_EXPRESSION: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "expression is not valid in compile-time constant context",
)];

const CHECKING_ARRAY_LENGTH_NOT_POSITIVE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "array length must be greater than zero",
)];

const CHECKING_ARRAY_GENERATOR_CARDINALITY_NOT_PROVABLE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "array generator element count cannot be proven",
    )];

const CHECKING_CONSTANT_LITERAL_NOT_REPRESENTABLE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "literal value cannot be represented by its selected type",
    )];

const CHECKING_CONSTANT_EVALUATION_STEP_LIMIT_EXCEEDED: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "constant evaluation exceeded its operation limit",
    )];

const CHECKING_CONSTANT_AGGREGATE_LIMIT_EXCEEDED: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "constant evaluation exceeded its aggregate element limit",
    )];

const CHECKING_CONSTANT_LITERAL_SIZE_LIMIT_EXCEEDED: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "constant evaluation exceeded its literal size limit",
    )];

const CHECKING_CONSTANT_INTEGER_SIZE_LIMIT_EXCEEDED: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "constant evaluation exceeded its exact integer size limit",
    )];

const CHECKING_CYCLIC_CONSTANT_DEFINITION: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "constant definition depends on itself through a cycle",
)];

const CHECKING_CONSTANT_DIVISION_BY_ZERO: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "constant operation divides by zero",
)];

const CHECKING_CONSTANT_VALUE_NOT_REPRESENTABLE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "constant result cannot be represented by its selected type",
    )];

const BINDING_AMBIGUOUS_NAME: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("name "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" is ambiguous"),
];

const BINDING_INACCESSIBLE_NAME: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("name "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" is not visible here"),
];

const BINDING_WRONG_NAME_KIND: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("name "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" does not refer to a "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedNameKind),
];

const BINDING_MALFORMED_NAME: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("name "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" refers to a malformed declaration"),
];

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

const NOTE_INTERFACE_DEPENDENCY_CONTEXT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("while loading package "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedPackageIdentity),
    MessageTemplatePart::Text(" product "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedProductIdentity),
    MessageTemplatePart::Text(" from "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactPath),
];

pub(crate) const fn severity_label(severity: SeverityKind) -> &'static str {
    match severity {
        SeverityKind::Error => "error",
        SeverityKind::Warning => "warning",
        SeverityKind::Note => "note",
        SeverityKind::Help => "help",
    }
}

pub(crate) const fn note_kind(kind: DiagnosticNoteKind) -> RenderedDiagnosticNoteKind {
    match kind {
        DiagnosticNoteKind::SourceIdsAreCompact
        | DiagnosticNoteKind::SourceTextOffsetsAreCompact
        | DiagnosticNoteKind::BomOnlyAllowedAtStart
        | DiagnosticNoteKind::IdentifiersMustBeAscii
        | DiagnosticNoteKind::OnlyImaginaryNumericSuffix
        | DiagnosticNoteKind::CharacterLiteralMustContainOneScalar
        | DiagnosticNoteKind::UnicodeEscapeMustBeScalar
        | DiagnosticNoteKind::InterfaceDependencyContext => RenderedDiagnosticNoteKind::Note,
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

pub(crate) const fn note_heading(kind: RenderedDiagnosticNoteKind) -> &'static str {
    match kind {
        RenderedDiagnosticNoteKind::Note => "note",
        RenderedDiagnosticNoteKind::Help => "help",
    }
}

pub(crate) const fn diagnostic_template(kind: DiagnosticKind) -> MessageTemplate {
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
        DiagnosticKind::RequestDuplicateSourceInput => {
            MessageTemplate::new(REQUEST_DUPLICATE_SOURCE_INPUT)
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
        DiagnosticKind::BindingUnresolvedName => MessageTemplate::new(BINDING_UNRESOLVED_NAME),
        DiagnosticKind::BindingAmbiguousName => MessageTemplate::new(BINDING_AMBIGUOUS_NAME),
        DiagnosticKind::BindingInaccessibleName => MessageTemplate::new(BINDING_INACCESSIBLE_NAME),
        DiagnosticKind::BindingWrongNameKind => MessageTemplate::new(BINDING_WRONG_NAME_KIND),
        DiagnosticKind::BindingMalformedName => MessageTemplate::new(BINDING_MALFORMED_NAME),
        DiagnosticKind::BindingNameAlreadyDefined => {
            MessageTemplate::new(BINDING_NAME_ALREADY_DEFINED)
        }
        DiagnosticKind::BindingIncoherentAlternativePattern => {
            MessageTemplate::new(BINDING_INCOHERENT_ALTERNATIVE_PATTERN)
        }
        DiagnosticKind::CheckingIncompatibleExpressionType => {
            MessageTemplate::new(CHECKING_INCOMPATIBLE_EXPRESSION_TYPE)
        }
        DiagnosticKind::CheckingIncompatiblePattern => {
            MessageTemplate::new(CHECKING_INCOMPATIBLE_PATTERN)
        }
        DiagnosticKind::CheckingRefutablePattern => {
            MessageTemplate::new(CHECKING_REFUTABLE_PATTERN)
        }
        DiagnosticKind::CheckingNonExhaustiveMatch => {
            MessageTemplate::new(CHECKING_NON_EXHAUSTIVE_MATCH)
        }
        DiagnosticKind::CheckingUnreachableMatchArm => {
            MessageTemplate::new(CHECKING_UNREACHABLE_MATCH_ARM)
        }
        DiagnosticKind::CheckingCannotInferExpressionType => {
            MessageTemplate::new(CHECKING_CANNOT_INFER_EXPRESSION_TYPE)
        }
        DiagnosticKind::CheckingInvalidConstantExpression => {
            MessageTemplate::new(CHECKING_INVALID_CONSTANT_EXPRESSION)
        }
        DiagnosticKind::CheckingArrayLengthNotPositive => {
            MessageTemplate::new(CHECKING_ARRAY_LENGTH_NOT_POSITIVE)
        }
        DiagnosticKind::CheckingArrayGeneratorCardinalityNotProvable => {
            MessageTemplate::new(CHECKING_ARRAY_GENERATOR_CARDINALITY_NOT_PROVABLE)
        }
        DiagnosticKind::CheckingConstantLiteralNotRepresentable => {
            MessageTemplate::new(CHECKING_CONSTANT_LITERAL_NOT_REPRESENTABLE)
        }
        DiagnosticKind::CheckingConstantEvaluationStepLimitExceeded => {
            MessageTemplate::new(CHECKING_CONSTANT_EVALUATION_STEP_LIMIT_EXCEEDED)
        }
        DiagnosticKind::CheckingConstantAggregateLimitExceeded => {
            MessageTemplate::new(CHECKING_CONSTANT_AGGREGATE_LIMIT_EXCEEDED)
        }
        DiagnosticKind::CheckingConstantLiteralSizeLimitExceeded => {
            MessageTemplate::new(CHECKING_CONSTANT_LITERAL_SIZE_LIMIT_EXCEEDED)
        }
        DiagnosticKind::CheckingConstantIntegerSizeLimitExceeded => {
            MessageTemplate::new(CHECKING_CONSTANT_INTEGER_SIZE_LIMIT_EXCEEDED)
        }
        DiagnosticKind::CheckingCyclicConstantDefinition => {
            MessageTemplate::new(CHECKING_CYCLIC_CONSTANT_DEFINITION)
        }
        DiagnosticKind::CheckingConstantDivisionByZero => {
            MessageTemplate::new(CHECKING_CONSTANT_DIVISION_BY_ZERO)
        }
        DiagnosticKind::CheckingConstantValueNotRepresentable => {
            MessageTemplate::new(CHECKING_CONSTANT_VALUE_NOT_REPRESENTABLE)
        }
        DiagnosticKind::CheckingNoApplicableCandidate => {
            MessageTemplate::new(CHECKING_NO_APPLICABLE_CANDIDATE)
        }
        DiagnosticKind::CheckingAmbiguousCandidate => {
            MessageTemplate::new(CHECKING_AMBIGUOUS_CANDIDATE)
        }
        DiagnosticKind::CheckingInaccessibleCandidate => {
            MessageTemplate::new(CHECKING_INACCESSIBLE_CANDIDATE)
        }
        DiagnosticKind::CheckingIncompatibleCandidate => {
            MessageTemplate::new(CHECKING_INCOMPATIBLE_CANDIDATE)
        }
        DiagnosticKind::CheckingTargetRepresentationUnavailable => {
            MessageTemplate::new(CHECKING_TARGET_REPRESENTATION_UNAVAILABLE)
        }
        DiagnosticKind::CheckingTargetCallableAbiUnavailable => {
            MessageTemplate::new(CHECKING_TARGET_CALLABLE_ABI_UNAVAILABLE)
        }
        DiagnosticKind::CheckingTargetAlignmentUnsupported => {
            MessageTemplate::new(CHECKING_TARGET_ALIGNMENT_UNSUPPORTED)
        }
        DiagnosticKind::CheckingTargetAbiRepresentationUnsupported => {
            MessageTemplate::new(CHECKING_TARGET_ABI_REPRESENTATION_UNSUPPORTED)
        }
        DiagnosticKind::BindingInvalidCallableAbi => {
            MessageTemplate::new(BINDING_INVALID_CALLABLE_ABI)
        }
        DiagnosticKind::BindingDuplicateCallableAbi => {
            MessageTemplate::new(BINDING_DUPLICATE_CALLABLE_ABI)
        }
        DiagnosticKind::BindingPredicateBodyRequired => {
            MessageTemplate::new(BINDING_PREDICATE_BODY_REQUIRED)
        }
        DiagnosticKind::BindingTrustedPredicateBodyNotAllowed => {
            MessageTemplate::new(BINDING_TRUSTED_PREDICATE_BODY_NOT_ALLOWED)
        }
        DiagnosticKind::BindingCyclicModuleExport => {
            MessageTemplate::new(BINDING_CYCLIC_MODULE_EXPORT)
        }
        DiagnosticKind::BindingConflictingModuleExport => {
            MessageTemplate::new(BINDING_CONFLICTING_MODULE_EXPORT)
        }
        DiagnosticKind::BindingInvalidModuleExportTarget => {
            MessageTemplate::new(BINDING_INVALID_MODULE_EXPORT_TARGET)
        }
        DiagnosticKind::BindingMalformedDirectiveArgument => {
            MessageTemplate::new(BINDING_MALFORMED_DIRECTIVE_ARGUMENT)
        }
        DiagnosticKind::CheckingDuplicateModuleContributionDirective => {
            MessageTemplate::new(CHECKING_DUPLICATE_MODULE_CONTRIBUTION_DIRECTIVE)
        }
        DiagnosticKind::EmissionMissingContribution => {
            MessageTemplate::new(EMISSION_MISSING_CONTRIBUTION)
        }
        DiagnosticKind::EmissionInvalidContribution => {
            MessageTemplate::new(EMISSION_INVALID_CONTRIBUTION)
        }
        DiagnosticKind::EmissionArtifactReadFailed => {
            MessageTemplate::new(EMISSION_ARTIFACT_READ_FAILED)
        }
        DiagnosticKind::EmissionArtifactOpenFailed => {
            MessageTemplate::new(EMISSION_ARTIFACT_OPEN_FAILED)
        }
        DiagnosticKind::EmissionArtifactWriteFailed => {
            MessageTemplate::new(EMISSION_ARTIFACT_WRITE_FAILED)
        }
        DiagnosticKind::EmissionArtifactFlushFailed => {
            MessageTemplate::new(EMISSION_ARTIFACT_FLUSH_FAILED)
        }
        DiagnosticKind::EmissionArtifactDigestMismatch => {
            MessageTemplate::new(EMISSION_ARTIFACT_DIGEST_MISMATCH)
        }
        DiagnosticKind::EmissionArtifactLengthMismatch => {
            MessageTemplate::new(EMISSION_ARTIFACT_LENGTH_MISMATCH)
        }
        DiagnosticKind::EmissionArtifactCommitFailed => {
            MessageTemplate::new(EMISSION_ARTIFACT_COMMIT_FAILED)
        }
        DiagnosticKind::InterfaceInvalidMagic
        | DiagnosticKind::InterfaceUnsupportedFormatRevision
        | DiagnosticKind::InterfaceUnsupportedLanguageRevision
        | DiagnosticKind::InterfaceUnsupportedEncoding
        | DiagnosticKind::InterfaceTruncated
        | DiagnosticKind::InterfaceMalformed
        | DiagnosticKind::InterfaceHashMismatch
        | DiagnosticKind::InterfaceSectionChecksumMismatch
        | DiagnosticKind::InterfaceResourceLimitExceeded
        | DiagnosticKind::InterfacePackageIdentityMismatch
        | DiagnosticKind::InterfaceProductIdentityMismatch
        | DiagnosticKind::InterfaceDependencyGraphInvalid
        | DiagnosticKind::InterfaceSemanticFactsInvalid => interface_diagnostic_template(kind),
    }
}

pub(crate) const fn note_template(kind: DiagnosticNoteKind) -> MessageTemplate {
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
        DiagnosticNoteKind::InterfaceDependencyContext => {
            MessageTemplate::new(NOTE_INTERFACE_DEPENDENCY_CONTEXT)
        }
    }
}
