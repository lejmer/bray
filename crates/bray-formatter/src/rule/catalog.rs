use super::descriptor::{
    FormatterDecision, FormatterDependency, FormatterParameter, FormatterRuleDescriptor,
};

macro_rules! rules {
    ($($rule:ident, $name:literal, $description:literal, $default:literal, $order:literal,
        [$($decision:ident),*], [$($parameter:ident),*],
        [$($dependent_decision:ident => $dependency:ident),*], [$($conflict:ident),*];)*) => {
        /// One independently configurable formatter behavior.
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        #[repr(u8)]
        pub enum FormatterRule {
            $(#[doc = $description] $rule,)*
        }

        impl FormatterRule {
            /// Every rule in stable identity order, generated with its descriptor.
            pub const ALL: &'static [Self] = &[$(Self::$rule,)*];

            /// Returns this rule's exhaustive composition and configuration contract.
            pub const fn descriptor(self) -> &'static FormatterRuleDescriptor {
                match self {
                    $(Self::$rule => &FormatterRuleDescriptor {
                        rule: Self::$rule,
                        name: $name,
                        description: $description,
                        enabled_by_default: $default,
                        order: $order,
                        decisions: &[$(FormatterDecision::$decision),*],
                        parameters: &[$(FormatterParameter::$parameter),*],
                        dependencies: &[$(FormatterDependency {
                            decision: FormatterDecision::$dependent_decision,
                            rule: Self::$dependency,
                        }),*],
                        conflicts: &[$(Self::$conflict),*],
                    },)*
                }
            }

            /// Returns the stable configuration-file name for this rule.
            pub const fn as_str(self) -> &'static str {
                self.descriptor().name
            }

            /// Resolves a stable configuration-file rule name.
            pub fn from_name(name: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|rule| rule.as_str() == name)
            }

            pub(crate) const fn index(self) -> usize {
                self as usize
            }
        }
    };
}

// Conflicting pairs are recorded once. Lower explicit order wins, independently
// of catalog declaration order or configuration-file field order.
rules! {
    Indentation, "indentation",
        "Indentation inside nested source constructs.", true, 13,
        [Indentation], [],
        [], [];

    BlockBraces, "block-braces",
        "Placement of braces around block-shaped syntax.", true, 14,
        [BracePlacement], [MaximumLineWidth],
        [], [];

    ModuleItemSpacing, "module-item-spacing",
        "Blank lines between module items.", true, 15,
        [ModuleItemSeparation], [],
        [], [];

    CallableMemberSpacing, "callable-member-spacing",
        "Blank lines between callable member declarations.", true, 16,
        [CallableMemberSeparation], [],
        [], [];

    DirectiveLineBreaks, "directive-line-breaks",
        "Line breaks following directives.", true, 17,
        [DirectiveSeparation], [],
        [], [];

    BlockParagraphSpacing, "block-paragraph-spacing",
        "Paragraph spacing inside executable blocks.", true, 19,
        [BlockParagraphSeparation], [],
        [], [];

    MatchCaseSpacing, "match-case-spacing",
        "Spacing between adjacent match cases.", true, 18,
        [MatchCaseSeparation, BlockParagraphSeparation], [],
        [], [BlockParagraphSpacing];

    MatchArmBodyLayout, "match-arm-body-layout",
        "Inline layout for simple match-arm bodies.", true, 0,
        [BracePlacement, StatementSeparation], [MaximumLineWidth],
        [BracePlacement => BlockBraces, StatementSeparation => BlockBraces], [BlockBraces, SemicolonLayout];

    StructConstructionLayout, "struct-construction-layout",
        "Field layout inside struct construction expressions.", true, 20,
        [ConstructionFieldSeparation], [],
        [], [];

    OverloadArmLayout, "overload-arm-layout",
        "Entry layout inside overload declarations.", true, 21,
        [OverloadArmSeparation], [],
        [], [];

    ParenthesizedListLayout, "parenthesized-list-layout",
        "Inline and multiline parenthesized-list layout.", true, 9,
        [ParenthesizedListBreaks, TokenSpacing], [MaximumLineWidth],
        [ParenthesizedListBreaks => LineWrapping], [BracketedListLayout, WordSpacing];

    BracketedListLayout, "bracketed-list-layout",
        "Inline and multiline bracketed-list layout.", true, 10,
        [BracketedListBreaks, TokenSpacing], [MaximumLineWidth],
        [BracketedListBreaks => LineWrapping], [WordSpacing];

    GenericListLayout, "generic-list-layout",
        "Inline and multiline generic-list layout.", true, 22,
        [GenericListBreaks], [MaximumLineWidth],
        [GenericListBreaks => LineWrapping], [];

    TrailingCommaLayout, "trailing-comma-layout",
        "Trailing commas in multiline lists.", true, 23,
        [TrailingComma], [],
        [TrailingComma => LineWrapping], [];

    CommaSpacing, "comma-spacing",
        "Spacing around commas.", true, 4,
        [TokenSpacing], [],
        [], [ColonSpacing, SemicolonLayout, PrefixOperatorSpacing, OperatorSpacing, ParenthesizedListLayout, BracketedListLayout, WordSpacing];

    ColonSpacing, "colon-spacing",
        "Spacing around colons.", true, 5,
        [TokenSpacing], [],
        [], [SemicolonLayout, PrefixOperatorSpacing, OperatorSpacing, ParenthesizedListLayout, BracketedListLayout, WordSpacing];

    OperatorSpacing, "operator-spacing",
        "Spacing around infix operators.", true, 8,
        [TokenSpacing], [],
        [], [ParenthesizedListLayout, BracketedListLayout, WordSpacing];

    GenericDelimiterSpacing, "generic-delimiter-spacing",
        "Spacing around generic delimiters.", true, 0,
        [TokenSpacing], [],
        [], [RangeSpacing, MemberAccessSpacing, DirectiveMarkerSpacing, CommaSpacing, ColonSpacing, SemicolonLayout, PrefixOperatorSpacing, OperatorSpacing, ParenthesizedListLayout, BracketedListLayout, WordSpacing];

    MemberAccessSpacing, "member-access-spacing",
        "Spacing around member access.", true, 2,
        [TokenSpacing], [],
        [], [DirectiveMarkerSpacing, CommaSpacing, ColonSpacing, SemicolonLayout, PrefixOperatorSpacing, OperatorSpacing, ParenthesizedListLayout, BracketedListLayout, WordSpacing];

    RangeSpacing, "range-spacing",
        "Spacing around range operators.", true, 1,
        [TokenSpacing], [],
        [], [MemberAccessSpacing, DirectiveMarkerSpacing, CommaSpacing, ColonSpacing, SemicolonLayout, PrefixOperatorSpacing, OperatorSpacing, ParenthesizedListLayout, BracketedListLayout, WordSpacing];

    PrefixOperatorSpacing, "prefix-operator-spacing",
        "Spacing after prefix operators.", true, 7,
        [TokenSpacing], [],
        [], [OperatorSpacing, ParenthesizedListLayout, BracketedListLayout, WordSpacing];

    DirectiveMarkerSpacing, "directive-marker-spacing",
        "Spacing between directive markers and names.", true, 3,
        [TokenSpacing], [],
        [], [CommaSpacing, ColonSpacing, SemicolonLayout, PrefixOperatorSpacing, OperatorSpacing, ParenthesizedListLayout, BracketedListLayout, WordSpacing];

    WordSpacing, "word-spacing",
        "Spacing between adjacent word-like tokens.", true, 11,
        [TokenSpacing], [],
        [], [];

    SemicolonLayout, "semicolon-layout",
        "Placement and spacing of semicolons.", true, 6,
        [StatementSeparation, TokenSpacing], [],
        [], [PrefixOperatorSpacing, OperatorSpacing, ParenthesizedListLayout, BracketedListLayout, WordSpacing];

    CommentPlacement, "comment-placement",
        "Placement and preservation of comments.", true, 24,
        [CommentAttachment], [],
        [], [];

    LineWrapping, "line-wrapping",
        "Width-aware line wrapping.", true, 25,
        [LineBreakSelection], [MaximumLineWidth],
        [], [];

    LineEndingStyle, "line-ending-style",
        "Preservation of the selected source line ending.", true, 26,
        [LineEnding], [],
        [], [];

    FinalNewline, "final-newline",
        "Presence of one final source line ending.", true, 27,
        [FinalNewline], [],
        [], [];

    SimplifyNestedIf, "simplify-nested-if",
        "Syntax-local simplification of nested conditional expressions.", false, 28,
        [ConditionalRewrite], [],
        [], [];
}
