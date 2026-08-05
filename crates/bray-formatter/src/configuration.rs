use std::num::NonZeroU16;

const DEFAULT_MAXIMUM_LINE_WIDTH: NonZeroU16 = match NonZeroU16::new(120) {
    Some(width) => width,
    None => panic!("the default formatter width must be positive"),
};

/// One independently configurable formatter behavior.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum FormatterRule {
    /// Indentation inside nested source constructs.
    Indentation,
    /// Placement of braces around block-shaped syntax.
    BlockBraces,
    /// Blank lines between module items.
    ModuleItemSpacing,
    /// Blank lines between callable member declarations.
    CallableMemberSpacing,
    /// Line breaks following directives.
    DirectiveLineBreaks,
    /// Paragraph spacing inside executable blocks.
    BlockParagraphSpacing,
    /// Spacing between adjacent match cases.
    MatchCaseSpacing,
    /// Inline layout for simple match-arm bodies.
    MatchArmBodyLayout,
    /// Field layout inside struct construction expressions.
    StructConstructionLayout,
    /// Entry layout inside overload declarations.
    OverloadArmLayout,
    /// Inline and multiline parenthesized-list layout.
    ParenthesizedListLayout,
    /// Inline and multiline bracketed-list layout.
    BracketedListLayout,
    /// Inline and multiline generic-list layout.
    GenericListLayout,
    /// Trailing commas in multiline lists.
    TrailingCommaLayout,
    /// Spacing around commas.
    CommaSpacing,
    /// Spacing around colons.
    ColonSpacing,
    /// Spacing around infix operators.
    OperatorSpacing,
    /// Spacing around generic delimiters.
    GenericDelimiterSpacing,
    /// Spacing around member access.
    MemberAccessSpacing,
    /// Spacing around range operators.
    RangeSpacing,
    /// Spacing after prefix operators.
    PrefixOperatorSpacing,
    /// Spacing between directive markers and names.
    DirectiveMarkerSpacing,
    /// Spacing between adjacent word-like tokens.
    WordSpacing,
    /// Placement and spacing of semicolons.
    SemicolonLayout,
    /// Placement and preservation of comments.
    CommentPlacement,
    /// Width-aware line wrapping.
    LineWrapping,
    /// Preservation of the selected source line ending.
    LineEndingStyle,
    /// Presence of one final source line ending.
    FinalNewline,
    /// Syntax-local simplification of nested conditional expressions.
    SimplifyNestedIf,
}

impl FormatterRule {
    /// Every formatter rule in stable declaration order.
    pub const ALL: &'static [Self] = &[
        Self::Indentation,
        Self::BlockBraces,
        Self::ModuleItemSpacing,
        Self::CallableMemberSpacing,
        Self::DirectiveLineBreaks,
        Self::BlockParagraphSpacing,
        Self::MatchCaseSpacing,
        Self::MatchArmBodyLayout,
        Self::StructConstructionLayout,
        Self::OverloadArmLayout,
        Self::ParenthesizedListLayout,
        Self::BracketedListLayout,
        Self::GenericListLayout,
        Self::TrailingCommaLayout,
        Self::CommaSpacing,
        Self::ColonSpacing,
        Self::OperatorSpacing,
        Self::GenericDelimiterSpacing,
        Self::MemberAccessSpacing,
        Self::RangeSpacing,
        Self::PrefixOperatorSpacing,
        Self::DirectiveMarkerSpacing,
        Self::WordSpacing,
        Self::SemicolonLayout,
        Self::CommentPlacement,
        Self::LineWrapping,
        Self::LineEndingStyle,
        Self::FinalNewline,
        Self::SimplifyNestedIf,
    ];

    /// Returns the stable configuration-file name for this rule.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Indentation => "indentation",
            Self::BlockBraces => "block-braces",
            Self::ModuleItemSpacing => "module-item-spacing",
            Self::CallableMemberSpacing => "callable-member-spacing",
            Self::DirectiveLineBreaks => "directive-line-breaks",
            Self::BlockParagraphSpacing => "block-paragraph-spacing",
            Self::MatchCaseSpacing => "match-case-spacing",
            Self::MatchArmBodyLayout => "match-arm-body-layout",
            Self::StructConstructionLayout => "struct-construction-layout",
            Self::OverloadArmLayout => "overload-arm-layout",
            Self::ParenthesizedListLayout => "parenthesized-list-layout",
            Self::BracketedListLayout => "bracketed-list-layout",
            Self::GenericListLayout => "generic-list-layout",
            Self::TrailingCommaLayout => "trailing-comma-layout",
            Self::CommaSpacing => "comma-spacing",
            Self::ColonSpacing => "colon-spacing",
            Self::OperatorSpacing => "operator-spacing",
            Self::GenericDelimiterSpacing => "generic-delimiter-spacing",
            Self::MemberAccessSpacing => "member-access-spacing",
            Self::RangeSpacing => "range-spacing",
            Self::PrefixOperatorSpacing => "prefix-operator-spacing",
            Self::DirectiveMarkerSpacing => "directive-marker-spacing",
            Self::WordSpacing => "word-spacing",
            Self::SemicolonLayout => "semicolon-layout",
            Self::CommentPlacement => "comment-placement",
            Self::LineWrapping => "line-wrapping",
            Self::LineEndingStyle => "line-ending-style",
            Self::FinalNewline => "final-newline",
            Self::SimplifyNestedIf => "simplify-nested-if",
        }
    }

    /// Resolves a stable configuration-file rule name.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|rule| rule.as_str() == name)
    }

    const fn index(self) -> usize {
        self as usize
    }

    const fn enabled_by_default(self) -> bool {
        !matches!(self, Self::SimplifyNestedIf)
    }
}

/// Immutable resolved formatter policy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FormatterConfiguration {
    enabled_rules: [bool; FormatterRule::ALL.len()],
    maximum_line_width: NonZeroU16,
}

impl FormatterConfiguration {
    /// Returns whether one formatter rule is enabled.
    pub const fn is_enabled(&self, rule: FormatterRule) -> bool {
        self.enabled_rules[rule.index()]
    }

    /// Returns the selected maximum display width.
    pub const fn maximum_line_width(&self) -> u16 {
        self.maximum_line_width.get()
    }

    /// Returns a configuration with one rule explicitly enabled or disabled.
    pub fn with_rule(mut self, rule: FormatterRule, enabled: bool) -> Self {
        self.enabled_rules[rule.index()] = enabled;

        self
    }

    /// Returns a configuration with the selected positive maximum display width.
    pub fn with_maximum_line_width(mut self, maximum_line_width: NonZeroU16) -> Self {
        self.maximum_line_width = maximum_line_width;

        self
    }
}

impl Default for FormatterConfiguration {
    fn default() -> Self {
        let mut enabled_rules = [false; FormatterRule::ALL.len()];

        for rule in FormatterRule::ALL {
            enabled_rules[rule.index()] = rule.enabled_by_default();
        }

        Self {
            enabled_rules,
            maximum_line_width: DEFAULT_MAXIMUM_LINE_WIDTH,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU16;

    use super::{FormatterConfiguration, FormatterRule};

    #[test]
    fn rule_names_are_unique_and_round_trip() {
        let mut names = FormatterRule::ALL
            .iter()
            .map(|rule| rule.as_str())
            .collect::<Vec<_>>();

        let rule_count = names.len();

        names.sort_unstable();
        names.dedup();

        assert_eq!(names.len(), rule_count);

        for rule in FormatterRule::ALL {
            assert_eq!(FormatterRule::from_name(rule.as_str()), Some(*rule));
        }
    }

    #[test]
    fn defaults_and_immutable_overrides_are_explicit() {
        let defaults = FormatterConfiguration::default();

        assert_eq!(defaults.maximum_line_width(), 120);

        assert!(defaults.is_enabled(FormatterRule::Indentation));
        assert!(!defaults.is_enabled(FormatterRule::SimplifyNestedIf));

        let configured = defaults
            .clone()
            .with_rule(FormatterRule::Indentation, false)
            .with_rule(FormatterRule::SimplifyNestedIf, true)
            .with_maximum_line_width(NonZeroU16::new(96).unwrap_or_else(|| unreachable!()));

        assert!(defaults.is_enabled(FormatterRule::Indentation));
        assert!(!defaults.is_enabled(FormatterRule::SimplifyNestedIf));
        assert!(!configured.is_enabled(FormatterRule::Indentation));
        assert!(configured.is_enabled(FormatterRule::SimplifyNestedIf));

        assert_eq!(configured.maximum_line_width(), 96);
    }
}
