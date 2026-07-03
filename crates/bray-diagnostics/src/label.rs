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
    /// Label for the start of a block comment that was not terminated.
    UnterminatedBlockCommentStart,
}

impl DiagnosticLabelKind {
    /// Returns the stable machine key for this label category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidUtf8Bytes => "invalid_utf8_bytes",
            Self::InvalidCharacter => "invalid_character",
            Self::UnterminatedBlockCommentStart => "unterminated_block_comment_start",
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
