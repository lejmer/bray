use std::num::NonZeroU16;

use super::FormatterRule;

/// Formatter precedence, from preservation constraints to output serialization.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FormatterStage {
    /// Preserve recovered source before applying any configurable rule.
    RecoveryPreservation,
    /// Apply explicitly enabled, syntax-local rewrites.
    SyntaxRewrites,
    /// Preserve comment ownership before choosing structural layout.
    CommentOwnership,
    /// Establish structural groups and required breaks.
    StructuralLayout,
    /// Choose spacing within structural constraints.
    TokenSpacing,
    /// Choose among legal line breaks using the configured width.
    Wrapping,
    /// Serialize line endings and the final newline.
    Serialization,
}

impl FormatterStage {
    /// Every precedence stage in application order.
    pub const ALL: &'static [Self] = &[
        Self::RecoveryPreservation,
        Self::SyntaxRewrites,
        Self::CommentOwnership,
        Self::StructuralLayout,
        Self::TokenSpacing,
        Self::Wrapping,
        Self::Serialization,
    ];
}

/// A formatting decision that a rule may own.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormatterDecision {
    /// Indentation levels.
    Indentation,
    /// Block and match-arm brace placement.
    BracePlacement,
    /// Separation of module items.
    ModuleItemSeparation,
    /// Separation of callable members.
    CallableMemberSeparation,
    /// Breaks following directives.
    DirectiveSeparation,
    /// Paragraph boundaries inside executable blocks.
    BlockParagraphSeparation,
    /// Separation of match cases.
    MatchCaseSeparation,
    /// Separation of construction fields.
    ConstructionFieldSeparation,
    /// Separation of overload arms.
    OverloadArmSeparation,
    /// Legal breaks in parenthesized lists.
    ParenthesizedListBreaks,
    /// Legal breaks in bracketed lists.
    BracketedListBreaks,
    /// Legal breaks in generic lists.
    GenericListBreaks,
    /// Conditional rendering of existing trailing commas.
    TrailingComma,
    /// Spacing at a boundary between two tokens.
    TokenSpacing,
    /// Separation of sequenced expressions.
    StatementSeparation,
    /// Attachment and separation of comments.
    CommentAttachment,
    /// Width-dependent selection among legal breaks.
    LineBreakSelection,
    /// Output line-ending spelling.
    LineEnding,
    /// Presence of a final newline.
    FinalNewline,
    /// Syntax-local nested conditional simplification.
    ConditionalRewrite,
}

impl FormatterDecision {
    /// Returns the stage that owns this decision.
    pub const fn stage(self) -> FormatterStage {
        match self {
            Self::ConditionalRewrite => FormatterStage::SyntaxRewrites,
            Self::CommentAttachment => FormatterStage::CommentOwnership,
            Self::TokenSpacing => FormatterStage::TokenSpacing,
            Self::LineBreakSelection => FormatterStage::Wrapping,
            Self::LineEnding | Self::FinalNewline => FormatterStage::Serialization,
            Self::Indentation
            | Self::BracePlacement
            | Self::ModuleItemSeparation
            | Self::CallableMemberSeparation
            | Self::DirectiveSeparation
            | Self::BlockParagraphSeparation
            | Self::MatchCaseSeparation
            | Self::ConstructionFieldSeparation
            | Self::OverloadArmSeparation
            | Self::ParenthesizedListBreaks
            | Self::BracketedListBreaks
            | Self::GenericListBreaks
            | Self::TrailingComma
            | Self::StatementSeparation => FormatterStage::StructuralLayout,
        }
    }
}

/// A prerequisite for one owned decision, leaving other decisions independent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FormatterDependency {
    /// The decision requiring another enabled rule.
    pub decision: FormatterDecision,
    /// The rule that must also be enabled.
    pub rule: FormatterRule,
}

/// A typed formatter parameter accepted by configuration decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormatterParameter {
    /// Positive maximum display width, shared by all width-aware layout.
    MaximumLineWidth,
}

/// Schema for a positive 16-bit integer parameter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FormatterParameterSchema {
    /// Stable configuration-field name.
    pub name: &'static str,
    /// Default value within the inclusive range 1 through 65535.
    pub default: NonZeroU16,
}

impl FormatterParameter {
    /// Returns the parameter's authoritative name, type constraint, and default.
    pub const fn schema(self) -> FormatterParameterSchema {
        match self {
            Self::MaximumLineWidth => FormatterParameterSchema {
                name: "maximum_line_width",
                default: NonZeroU16::new(120).expect("the default formatter width is positive"),
            },
        }
    }
}

impl FormatterParameterSchema {
    /// Validates an external integer against the positive 16-bit schema.
    pub fn validate(self, value: u64) -> Option<NonZeroU16> {
        u16::try_from(value).ok().and_then(NonZeroU16::new)
    }
}

/// The authoritative contract for one independently configurable formatter rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FormatterRuleDescriptor {
    /// Stable typed identity.
    pub rule: FormatterRule,
    /// Stable configuration name.
    pub name: &'static str,
    /// Rule purpose, also used by the generated rule reference.
    pub description: &'static str,
    /// Whether the rule is enabled without an explicit override.
    pub enabled_by_default: bool,
    /// Typed schemas for parameters used by this rule.
    pub parameters: &'static [FormatterParameter],
    /// Decisions owned by the rule.
    pub decisions: &'static [FormatterDecision],
    /// Decision-specific enabled-rule prerequisites.
    pub dependencies: &'static [FormatterDependency],
    /// Other owners explicitly allowed to compete for a shared decision.
    ///
    /// Each pair is recorded once. The earlier precedence key wins.
    pub conflicts: &'static [FormatterRule],
    /// Explicit precedence within the stage, with smaller values winning.
    pub order: u8,
}

impl FormatterRuleDescriptor {
    /// Returns the stages in which this rule owns decisions, in precedence order.
    pub fn stages(&self) -> impl Iterator<Item = FormatterStage> + '_ {
        FormatterStage::ALL.iter().copied().filter(|stage| {
            self.decisions
                .iter()
                .any(|decision| decision.stage() == *stage)
        })
    }
}
