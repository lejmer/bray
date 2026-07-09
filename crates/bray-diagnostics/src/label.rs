use bray_source::SourceSpan;

use crate::argument::DiagnosticArg;

/// Source label attached to a diagnostic.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DiagnosticLabel {
    kind: DiagnosticLabelKind,
    style: DiagnosticLabelStyle,
    span: SourceSpan,
    args: Vec<DiagnosticArg>,
}

impl DiagnosticLabel {
    /// Creates a source label with no message arguments.
    pub fn new(kind: DiagnosticLabelKind, style: DiagnosticLabelStyle, span: SourceSpan) -> Self {
        Self {
            kind,
            style,
            span,
            args: Vec::new(),
        }
    }

    /// Creates a primary source label.
    pub fn primary(kind: DiagnosticLabelKind, span: SourceSpan) -> Self {
        Self::new(kind, DiagnosticLabelStyle::Primary, span)
    }

    /// Creates a secondary source label.
    pub fn secondary(kind: DiagnosticLabelKind, span: SourceSpan) -> Self {
        Self::new(kind, DiagnosticLabelStyle::Secondary, span)
    }

    /// Adds one typed argument to the label.
    pub fn with_arg(mut self, arg: DiagnosticArg) -> Self {
        self.args.push(arg);

        self
    }

    /// Returns the stable label category.
    pub const fn kind(&self) -> DiagnosticLabelKind {
        self.kind
    }

    /// Returns whether this is a primary or secondary label.
    pub const fn style(&self) -> DiagnosticLabelStyle {
        self.style
    }

    /// Returns the labeled source span.
    pub const fn span(&self) -> SourceSpan {
        self.span
    }

    /// Returns the typed label arguments.
    pub fn args(&self) -> &[DiagnosticArg] {
        &self.args
    }
}

/// Stable category for a diagnostic source label.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLabelKind {
    /// Label for bytes that are not valid UTF-8.
    InvalidUtf8Bytes,
    /// Label for a character that the lexer cannot accept.
    InvalidCharacter,
    /// Label for a byte order mark after the start of the source.
    MisplacedBom,
    /// Label for a lone carriage return outside a block comment.
    LoneCarriageReturn,
    /// Label for non-ASCII identifier text.
    NonAsciiIdentifier,
    /// Label for invalid identifier text.
    InvalidIdentifier,
    /// Label for invalid operator or punctuation text.
    InvalidOperatorOrPunctuation,
    /// Label for a malformed literal spelling.
    MalformedLiteral,
    /// Label for a numeric suffix that is not accepted.
    InvalidNumericSuffix,
    /// Label for the start of an unterminated character literal.
    UnterminatedCharacterLiteralStart,
    /// Label for the start of an unterminated string literal.
    UnterminatedStringLiteralStart,
    /// Label for an unknown escape sequence.
    UnknownEscape,
    /// Label for an invalid Unicode escape sequence.
    InvalidUnicodeEscape,
    /// Label for the start of a block comment that was not terminated.
    UnterminatedBlockCommentStart,
    /// Label for a missing token insertion point.
    ExpectedTokenInsertionPoint,
    /// Label for source text where an expression was expected.
    ExpectedExpression,
    /// Label for an end-of-file position reached before syntax was complete.
    UnexpectedEof,
}

impl DiagnosticLabelKind {
    /// Returns the stable machine key for this label category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidUtf8Bytes => "invalid_utf8_bytes",
            Self::InvalidCharacter => "invalid_character",
            Self::MisplacedBom => "misplaced_bom",
            Self::LoneCarriageReturn => "lone_carriage_return",
            Self::NonAsciiIdentifier => "non_ascii_identifier",
            Self::InvalidIdentifier => "invalid_identifier",
            Self::InvalidOperatorOrPunctuation => "invalid_operator_or_punctuation",
            Self::MalformedLiteral => "malformed_literal",
            Self::InvalidNumericSuffix => "invalid_numeric_suffix",
            Self::UnterminatedCharacterLiteralStart => "unterminated_character_literal_start",
            Self::UnterminatedStringLiteralStart => "unterminated_string_literal_start",
            Self::UnknownEscape => "unknown_escape",
            Self::InvalidUnicodeEscape => "invalid_unicode_escape",
            Self::UnterminatedBlockCommentStart => "unterminated_block_comment_start",
            Self::ExpectedTokenInsertionPoint => "expected_token_insertion_point",
            Self::ExpectedExpression => "expected_expression",
            Self::UnexpectedEof => "unexpected_eof",
        }
    }
}

/// Relationship between a label and its diagnostic.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLabelStyle {
    /// Main label for the source range directly responsible.
    Primary,
    /// Supporting label for another relevant source range.
    Secondary,
}

impl DiagnosticLabelStyle {
    /// Returns the stable machine key for this label style.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};

    use super::{DiagnosticLabel, DiagnosticLabelKind, DiagnosticLabelStyle};
    use crate::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

    #[test]
    fn labels_carry_span_style_kind_and_typed_args() {
        let span = SourceSpan::new(
            SourceId::new(2),
            TextRange::new(TextSize::new(8), TextSize::new(9)),
        );

        let label = DiagnosticLabel::primary(DiagnosticLabelKind::InvalidCharacter, span).with_arg(
            DiagnosticArg::new(
                DiagnosticArgName::Character,
                DiagnosticArgValue::Character('\u{7f}'),
            ),
        );

        assert_eq!(label.kind(), DiagnosticLabelKind::InvalidCharacter);
        assert_eq!(label.kind().as_str(), "invalid_character");
        assert_eq!(label.style(), DiagnosticLabelStyle::Primary);
        assert_eq!(label.style().as_str(), "primary");
        assert_eq!(label.span(), span);

        assert_eq!(
            label.args(),
            &[DiagnosticArg::new(
                DiagnosticArgName::Character,
                DiagnosticArgValue::Character('\u{7f}')
            )]
        );
    }
}
