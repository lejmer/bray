/// Source newline normalization policy.
///
/// Bray preserves exact source text. LF and CRLF are accepted source line
/// breaks. A lone CR is not normalized and is not an accepted source line
/// break. Consumers that present source locations may still recover it as a
/// line break after reporting it as invalid.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SourceNewlinePolicy {
    /// Preserve exact source text without newline normalization.
    #[default]
    PreserveExactText,
}

impl SourceNewlinePolicy {
    /// The default source newline policy.
    pub const DEFAULT: Self = Self::PreserveExactText;

    /// Returns whether this policy preserves source text exactly.
    pub const fn preserves_exact_text(self) -> bool {
        match self {
            Self::PreserveExactText => true,
        }
    }

    /// Returns the stable machine key for this newline policy.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PreserveExactText => "preserve_exact_text",
        }
    }

    /// Applies newline normalization according to this policy.
    pub fn normalize_text(self, text: String) -> String {
        match self {
            Self::PreserveExactText => text,
        }
    }

    /// Returns whether a lone CR is a recognized line break.
    pub const fn treats_lone_cr_as_line_break(self) -> bool {
        match self {
            Self::PreserveExactText => false,
        }
    }

    /// Returns the source line break spelling beginning at `character`.
    pub fn line_break_kind(
        self,
        character: char,
        next_character: Option<char>,
    ) -> Option<SourceLineBreakKind> {
        match self {
            Self::PreserveExactText => match (character, next_character) {
                ('\r', Some('\n')) => Some(SourceLineBreakKind::CarriageReturnLineFeed),
                ('\n', _) => Some(SourceLineBreakKind::LineFeed),
                _ => None,
            },
        }
    }
}

/// Recognized source line break spelling.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SourceLineBreakKind {
    /// LF line break.
    LineFeed,
    /// CRLF line break.
    CarriageReturnLineFeed,
}

impl SourceLineBreakKind {
    /// Returns the byte length of this line break spelling.
    pub const fn byte_len(self) -> usize {
        match self {
            Self::LineFeed => 1,
            Self::CarriageReturnLineFeed => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SourceLineBreakKind, SourceNewlinePolicy};

    #[test]
    fn newline_policy_preserves_exact_text() {
        let text = String::from("a\r\nb\rc\n");

        assert_eq!(
            SourceNewlinePolicy::DEFAULT.normalize_text(text),
            "a\r\nb\rc\n"
        );

        assert!(SourceNewlinePolicy::DEFAULT.preserves_exact_text());
        assert_eq!(SourceNewlinePolicy::DEFAULT.as_str(), "preserve_exact_text");
    }

    #[test]
    fn newline_policy_recognizes_lf_and_crlf_only() {
        let policy = SourceNewlinePolicy::DEFAULT;

        assert_eq!(
            policy.line_break_kind('\n', None),
            Some(SourceLineBreakKind::LineFeed)
        );

        assert_eq!(
            policy.line_break_kind('\r', Some('\n')),
            Some(SourceLineBreakKind::CarriageReturnLineFeed)
        );

        assert_eq!(policy.line_break_kind('\r', None), None);
        assert_eq!(policy.line_break_kind('\r', Some('x')), None);

        assert!(!policy.treats_lone_cr_as_line_break());
    }
}
