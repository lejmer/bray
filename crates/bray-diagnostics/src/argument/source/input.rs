use bray_source::SourceInputKind;
use bray_syntax::SyntaxKind;

use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates a source-count argument.
    pub const fn source_count(source_count: u64) -> Self {
        Self::new(
            DiagnosticArgName::SourceCount,
            DiagnosticArgValue::SourceCount(source_count),
        )
    }

    /// Creates a maximum accepted count argument.
    pub const fn maximum_count(maximum_count: u64) -> Self {
        Self::new(
            DiagnosticArgName::MaximumCount,
            DiagnosticArgValue::Count(maximum_count),
        )
    }

    /// Creates an actual externally supplied count argument.
    pub const fn actual_count(actual_count: u64) -> Self {
        Self::new(
            DiagnosticArgName::ActualCount,
            DiagnosticArgValue::Count(actual_count),
        )
    }

    /// Creates a source-input-kind argument.
    pub const fn source_input_kind(kind: SourceInputKind) -> Self {
        Self::new(
            DiagnosticArgName::SourceInputKind,
            DiagnosticArgValue::SourceInputKind(kind),
        )
    }

    /// Creates an expected syntax-kind argument.
    pub const fn expected_syntax_kind(kind: SyntaxKind) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedSyntaxKind,
            DiagnosticArgValue::SyntaxKind(kind),
        )
    }

    /// Creates an actual syntax-kind argument.
    pub const fn actual_syntax_kind(kind: SyntaxKind) -> Self {
        Self::new(
            DiagnosticArgName::ActualSyntaxKind,
            DiagnosticArgValue::SyntaxKind(kind),
        )
    }

    /// Creates a declaration-modifier syntax-kind argument.
    pub const fn modifier_kind(kind: SyntaxKind) -> Self {
        Self::new(
            DiagnosticArgName::ModifierKind,
            DiagnosticArgValue::SyntaxKind(kind),
        )
    }

    /// Creates a conflicting declaration-modifier syntax-kind argument.
    pub const fn conflicting_modifier_kind(kind: SyntaxKind) -> Self {
        Self::new(
            DiagnosticArgName::ConflictingModifierKind,
            DiagnosticArgValue::SyntaxKind(kind),
        )
    }

    /// Creates a declaration-directive syntax-kind argument.
    pub const fn directive_kind(kind: SyntaxKind) -> Self {
        Self::new(
            DiagnosticArgName::DirectiveKind,
            DiagnosticArgValue::SyntaxKind(kind),
        )
    }

    /// Creates a conflicting declaration-directive syntax-kind argument.
    pub const fn conflicting_directive_kind(kind: SyntaxKind) -> Self {
        Self::new(
            DiagnosticArgName::ConflictingDirectiveKind,
            DiagnosticArgValue::SyntaxKind(kind),
        )
    }
}
