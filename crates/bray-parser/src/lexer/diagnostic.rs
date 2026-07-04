use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticId, DiagnosticKind,
    DiagnosticLabel, DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};
use bray_source::{SourceSnapshot, SourceSpan, TextRange, TextSize};

pub(super) fn misplaced_bom(snapshot: &SourceSnapshot, range: TextRange) -> Diagnostic {
    lexical_diagnostic(
        snapshot,
        DiagnosticKind::LexicalMisplacedBom,
        DiagnosticLabelKind::MisplacedBom,
        range,
    )
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::BomOnlyAllowedAtStart,
    ))
}

pub(super) fn lone_carriage_return(snapshot: &SourceSnapshot, range: TextRange) -> Diagnostic {
    lexical_diagnostic(
        snapshot,
        DiagnosticKind::LexicalLoneCarriageReturn,
        DiagnosticLabelKind::LoneCarriageReturn,
        range,
    )
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::LineBreaksMustBeLfOrCrlf,
    ))
}

pub(super) fn invalid_character(
    snapshot: &SourceSnapshot,
    range: TextRange,
    character: char,
) -> Diagnostic {
    let span = span(snapshot, range);
    let arg = character_arg(character);

    Diagnostic::new(
        diagnostic_id(range.start()),
        DiagnosticKind::LexicalInvalidCharacter,
        SeverityKind::Error,
    )
    .with_primary_span(span)
    .with_arg(arg.clone())
    .with_label(DiagnosticLabel::primary(DiagnosticLabelKind::InvalidCharacter, span).with_arg(arg))
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::CharacterNotAccepted,
    ))
}

pub(super) fn non_ascii_identifier(
    snapshot: &SourceSnapshot,
    range: TextRange,
    character: char,
) -> Diagnostic {
    let span = span(snapshot, range);
    let arg = character_arg(character);

    Diagnostic::new(
        diagnostic_id(range.start()),
        DiagnosticKind::LexicalNonAsciiIdentifier,
        SeverityKind::Error,
    )
    .with_primary_span(span)
    .with_arg(arg.clone())
    .with_label(
        DiagnosticLabel::primary(DiagnosticLabelKind::NonAsciiIdentifier, span).with_arg(arg),
    )
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::IdentifiersMustBeAscii,
    ))
}

pub(super) fn invalid_identifier(snapshot: &SourceSnapshot, range: TextRange) -> Diagnostic {
    lexical_diagnostic(
        snapshot,
        DiagnosticKind::LexicalInvalidIdentifier,
        DiagnosticLabelKind::InvalidIdentifier,
        range,
    )
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::IdentifierSpellingMustBeValid,
    ))
}

pub(super) fn invalid_operator_or_punctuation(
    snapshot: &SourceSnapshot,
    range: TextRange,
) -> Diagnostic {
    lexical_diagnostic(
        snapshot,
        DiagnosticKind::LexicalInvalidOperatorOrPunctuation,
        DiagnosticLabelKind::InvalidOperatorOrPunctuation,
        range,
    )
}

pub(super) fn malformed_numeric_literal(snapshot: &SourceSnapshot, range: TextRange) -> Diagnostic {
    lexical_diagnostic(
        snapshot,
        DiagnosticKind::LexicalMalformedNumericLiteral,
        DiagnosticLabelKind::MalformedLiteral,
        range,
    )
}

pub(super) fn invalid_numeric_suffix(
    snapshot: &SourceSnapshot,
    range: TextRange,
    suffix_start: Option<char>,
) -> Diagnostic {
    let mut diagnostic = lexical_diagnostic(
        snapshot,
        DiagnosticKind::LexicalInvalidNumericSuffix,
        DiagnosticLabelKind::InvalidNumericSuffix,
        range,
    )
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::OnlyImaginaryNumericSuffix,
    ));

    if let Some(character) = suffix_start {
        diagnostic = diagnostic.with_arg(character_arg(character));
    }

    diagnostic
}

pub(super) fn malformed_character_literal(
    snapshot: &SourceSnapshot,
    range: TextRange,
) -> Diagnostic {
    lexical_diagnostic(
        snapshot,
        DiagnosticKind::LexicalMalformedCharacterLiteral,
        DiagnosticLabelKind::MalformedLiteral,
        range,
    )
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::CharacterLiteralMustContainOneScalar,
    ))
}

pub(super) fn unterminated_character_literal(
    snapshot: &SourceSnapshot,
    range: TextRange,
) -> Diagnostic {
    lexical_diagnostic(
        snapshot,
        DiagnosticKind::LexicalUnterminatedCharacterLiteral,
        DiagnosticLabelKind::UnterminatedCharacterLiteralStart,
        range,
    )
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::CharacterLiteralNeedsTerminator,
    ))
}

pub(super) fn unterminated_string_literal(
    snapshot: &SourceSnapshot,
    range: TextRange,
) -> Diagnostic {
    lexical_diagnostic(
        snapshot,
        DiagnosticKind::LexicalUnterminatedStringLiteral,
        DiagnosticLabelKind::UnterminatedStringLiteralStart,
        range,
    )
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::StringLiteralNeedsTerminator,
    ))
}

pub(super) fn unknown_escape(
    snapshot: &SourceSnapshot,
    range: TextRange,
    escaped: Option<char>,
) -> Diagnostic {
    let mut diagnostic = lexical_diagnostic(
        snapshot,
        DiagnosticKind::LexicalUnknownEscape,
        DiagnosticLabelKind::UnknownEscape,
        range,
    )
    .with_note(DiagnosticNote::new(DiagnosticNoteKind::EscapeMustBeKnown));

    if let Some(character) = escaped {
        diagnostic = diagnostic.with_arg(character_arg(character));
    }

    diagnostic
}

pub(super) fn invalid_unicode_escape(snapshot: &SourceSnapshot, range: TextRange) -> Diagnostic {
    lexical_diagnostic(
        snapshot,
        DiagnosticKind::LexicalInvalidUnicodeEscape,
        DiagnosticLabelKind::InvalidUnicodeEscape,
        range,
    )
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::UnicodeEscapeMustBeScalar,
    ))
}

pub(super) fn unterminated_block_comment(
    snapshot: &SourceSnapshot,
    range: TextRange,
) -> Diagnostic {
    lexical_diagnostic(
        snapshot,
        DiagnosticKind::LexicalUnterminatedBlockComment,
        DiagnosticLabelKind::UnterminatedBlockCommentStart,
        range,
    )
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::BlockCommentNeedsTerminator,
    ))
}

fn lexical_diagnostic(
    snapshot: &SourceSnapshot,
    kind: DiagnosticKind,
    label_kind: DiagnosticLabelKind,
    range: TextRange,
) -> Diagnostic {
    let span = span(snapshot, range);

    Diagnostic::new(diagnostic_id(range.start()), kind, SeverityKind::Error)
        .with_primary_span(span)
        .with_label(DiagnosticLabel::primary(label_kind, span))
}

fn span(snapshot: &SourceSnapshot, range: TextRange) -> SourceSpan {
    SourceSpan::new(snapshot.source_id(), range)
}

fn diagnostic_id(start: TextSize) -> DiagnosticId {
    DiagnosticId::new(start.bytes())
}

fn character_arg(character: char) -> DiagnosticArg {
    DiagnosticArg::new(
        DiagnosticArgName::Character,
        DiagnosticArgValue::Character(character),
    )
}
