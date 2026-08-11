use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticKind,
    DiagnosticLabel, DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind, DiagnosticSourceEdit,
    DiagnosticSuggestion, DiagnosticSuggestionKind, SeverityKind,
};
use bray_source::{SourceSnapshot, SourceSpan, TextRange};

use crate::diagnostic::diagnostic_id;

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
    .with_suggestion(replacement_suggestion(
        span(snapshot, range),
        DiagnosticSuggestionKind::ReplaceWithLineFeed,
        "\n",
    ))
}

pub(super) fn invalid_character(
    snapshot: &SourceSnapshot,
    range: TextRange,
    character: char,
) -> Diagnostic {
    character_diagnostic(
        snapshot,
        range,
        character,
        DiagnosticKind::LexicalInvalidCharacter,
        DiagnosticLabelKind::InvalidCharacter,
        DiagnosticNoteKind::CharacterNotAccepted,
    )
}

pub(super) fn non_ascii_identifier(
    snapshot: &SourceSnapshot,
    range: TextRange,
    character: char,
) -> Diagnostic {
    character_diagnostic(
        snapshot,
        range,
        character,
        DiagnosticKind::LexicalNonAsciiIdentifier,
        DiagnosticLabelKind::NonAsciiIdentifier,
        DiagnosticNoteKind::IdentifiersMustBeAscii,
    )
}

fn character_diagnostic(
    snapshot: &SourceSnapshot,
    range: TextRange,
    character: char,
    kind: DiagnosticKind,
    label: DiagnosticLabelKind,
    note: DiagnosticNoteKind,
) -> Diagnostic {
    let span = span(snapshot, range);
    let arg = character_arg(character);

    Diagnostic::new(diagnostic_id(range.start()), kind, SeverityKind::Error)
    .with_primary_span(span)
    .with_arg(arg.clone())
    .with_label(DiagnosticLabel::primary(label, span).with_arg(arg))
    .with_note(DiagnosticNote::new(note))
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
    .with_suggestion(terminator_suggestion(snapshot, range, "'"))
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
    .with_suggestion(terminator_suggestion(snapshot, range, "\""))
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
    .with_suggestion(terminator_suggestion(snapshot, range, "*/"))
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

fn character_arg(character: char) -> DiagnosticArg {
    DiagnosticArg::new(
        DiagnosticArgName::Character,
        DiagnosticArgValue::Character(character),
    )
}

fn terminator_suggestion(
    snapshot: &SourceSnapshot,
    range: TextRange,
    terminator: &str,
) -> DiagnosticSuggestion {
    replacement_suggestion(
        SourceSpan::empty(snapshot.source_id(), range.end()),
        DiagnosticSuggestionKind::AddTerminator,
        terminator,
    )
}

fn replacement_suggestion(
    span: SourceSpan,
    kind: DiagnosticSuggestionKind,
    replacement: &str,
) -> DiagnosticSuggestion {
    DiagnosticSuggestion::machine_edit(kind, DiagnosticSourceEdit::new(span, replacement))
}

#[cfg(test)]
mod tests {
    use bray_source::{TextRange, TextSize};
    use bray_testing::test_source_snapshot;

    use super::{lone_carriage_return, unterminated_block_comment};

    #[test]
    fn lexical_corrections_publish_exact_machine_applicable_edits() {
        let line_break_source = test_source_snapshot("\r");

        let line_break = lone_carriage_return(
            &line_break_source,
            TextRange::new(TextSize::ZERO, TextSize::new(1)),
        );

        let [suggestion] = line_break.suggestions() else {
            panic!("expected one line-break suggestion: {line_break:?}");
        };

        assert_eq!(suggestion.edits()[0].replacement(), "\n");

        let comment_source = test_source_snapshot("/* open");

        let comment = unterminated_block_comment(
            &comment_source,
            TextRange::new(TextSize::ZERO, TextSize::new(7)),
        );

        let [suggestion] = comment.suggestions() else {
            panic!("expected one comment terminator suggestion: {comment:?}");
        };

        assert_eq!(suggestion.edits()[0].replacement(), "*/");
        assert!(suggestion.edits()[0].span().is_empty());
    }
}
