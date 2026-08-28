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
    /// Label for syntax that exceeds the supported nesting depth.
    NestingLimitExceeded,
    /// Label for an end-of-file position reached before syntax was complete.
    UnexpectedEof,
    /// Label for a name reference rejected during binding.
    NameReference,
    /// Label for a name definition that conflicts with an existing definition.
    NameDefinition,
    /// Label for an alternative pattern with incoherent bindings.
    AlternativePattern,
    /// Label for a pattern occurrence rejected by semantic pattern checking.
    PatternFailure,
    /// Label for a match expression whose cases do not cover the subject domain.
    MatchCoverage,
    /// Label for a callable ABI directive rejected during binding.
    CallableAbiDirective,
    /// Label for a malformed directive argument.
    DirectiveArgument,
    /// Label for a module export rejected during binding.
    ModuleExport,
    /// Label for an expression whose actual type conflicts with its expected type.
    IncompatibleExpressionType,
    /// Label for an expression whose semantic selection has no unique visible candidate.
    SelectionFailure,
    /// Label for a trait member fulfillment rejected by conformance checking.
    TraitFulfillment,
    /// Label for an expression without enough type constraints.
    UnconstrainedExpression,
    /// Label for a half-open range whose bound type is not an integer type.
    InvalidRangeElementType,
    /// Label for an expression rejected during compile-time evaluation.
    InvalidConstantExpression,
    /// Label for a layout, tag, or copy declaration rejected during representation checking.
    InvalidTypeRepresentationContract,
    /// Label for a propagation operator without a compatible enclosing boundary.
    IncompatiblePropagationBoundary,
    /// Label for a fixed-array generator whose output length is not provable.
    UnprovenArrayGeneratorCardinality,
    /// Label for a requirement rejected by the selected compilation target.
    UnsupportedTargetRequirement,
    /// Label for the unit whose refinement analysis exceeds a configured capacity.
    RefinementCapacityExceeded,
    /// Label for the source expression whose memory operation fails.
    MemoryOperationFailure,
    /// Label for the declaration that repeats an existing name.
    DuplicateDeclaration,
    /// Label for a declaration whose form violates its owning context.
    InvalidDeclaration,
    /// Label for a callable that cannot serve as a product entry.
    InvalidProductEntry,
    /// Label for a callable or type that cannot cross a foreign boundary.
    InvalidForeignBoundary,
    /// Label for an implementation that overlaps another implementation.
    OverlappingImplementation,
    /// Label for the implementation at which coherence comparison reached its limit.
    ImplementationCoherenceLimitExceeded,
    /// Label for an implementation that requires an overload family.
    UngroupedImplementationOverload,
    /// Label for an invalid overload arm or family membership.
    InvalidOverload,
    /// Label for a duplicate overload arm.
    DuplicateOverloadArm,
    /// Label for an overload arm or family that conflicts with another.
    ConflictingOverload,
    /// Label for an invalid product-level declaration or directive.
    InvalidProductConfiguration,
    /// Label for a repeated module contribution directive.
    DuplicateModuleContribution,
    /// Label for a use of storage after it was moved.
    MovedStorageUse,
    /// Label for an operation that conflicts with a live borrow.
    ConflictingBorrowOperation,
    /// Label for an operation without mutation authority.
    MissingMutationAuthority,
    /// Label for an operation that requires storage ownership.
    MissingStorageOwnership,
    /// Label for a value whose dependency cannot leave its storage scope.
    EscapingStorageDependency,
    /// Label for the dependency declaration that selected an external artifact.
    DependencySelection,
    /// Label for an await whose required semantic state is unavailable.
    UnavailableAwaitDependency,
    /// Label for an operation that requires an asynchronous callable body.
    InvalidAsyncOperation,
    /// Label for a trusted capability requirement rejected by callable checking.
    InvalidTrustedCapabilityRequirement,
    /// Label for a split module declaration with a conflicting surface.
    ConflictingModuleDeclaration,
}

impl DiagnosticLabelKind {
    /// Returns the stable machine key for this label category.
    pub const fn as_str(self) -> &'static str {
        match self {
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
            Self::NestingLimitExceeded => "nesting_limit_exceeded",
            Self::UnexpectedEof => "unexpected_eof",
            Self::NameReference => "name_reference",
            Self::NameDefinition => "name_definition",
            Self::AlternativePattern => "alternative_pattern",
            Self::PatternFailure => "pattern_failure",
            Self::MatchCoverage => "match_coverage",
            Self::CallableAbiDirective => "callable_abi_directive",
            Self::DirectiveArgument => "directive_argument",
            Self::ModuleExport => "module_export",
            Self::IncompatibleExpressionType => "incompatible_expression_type",
            Self::SelectionFailure => "selection_failure",
            Self::TraitFulfillment => "trait_fulfillment",
            Self::UnconstrainedExpression => "unconstrained_expression",
            Self::InvalidRangeElementType => "invalid_range_element_type",
            Self::InvalidConstantExpression => "invalid_constant_expression",
            Self::InvalidTypeRepresentationContract => "invalid_type_representation_contract",
            Self::IncompatiblePropagationBoundary => "incompatible_propagation_boundary",
            Self::UnprovenArrayGeneratorCardinality => "unproven_array_generator_cardinality",
            Self::UnsupportedTargetRequirement => "unsupported_target_requirement",
            Self::RefinementCapacityExceeded => "refinement_capacity_exceeded",
            Self::MemoryOperationFailure => "memory_operation_failure",
            Self::DuplicateDeclaration => "duplicate_declaration",
            Self::InvalidDeclaration => "invalid_declaration",
            Self::InvalidProductEntry => "invalid_product_entry",
            Self::InvalidForeignBoundary => "invalid_foreign_boundary",
            Self::OverlappingImplementation => "overlapping_implementation",
            Self::ImplementationCoherenceLimitExceeded => "implementation_coherence_limit_exceeded",
            Self::UngroupedImplementationOverload => "ungrouped_implementation_overload",
            Self::InvalidOverload => "invalid_overload",
            Self::DuplicateOverloadArm => "duplicate_overload_arm",
            Self::ConflictingOverload => "conflicting_overload",
            Self::InvalidProductConfiguration => "invalid_product_configuration",
            Self::DuplicateModuleContribution => "duplicate_module_contribution",
            Self::MovedStorageUse => "moved_storage_use",
            Self::ConflictingBorrowOperation => "conflicting_borrow_operation",
            Self::MissingMutationAuthority => "missing_mutation_authority",
            Self::MissingStorageOwnership => "missing_storage_ownership",
            Self::EscapingStorageDependency => "escaping_storage_dependency",
            Self::DependencySelection => "dependency_selection",
            Self::UnavailableAwaitDependency => "unavailable_await_dependency",
            Self::InvalidAsyncOperation => "invalid_async_operation",
            Self::InvalidTrustedCapabilityRequirement => "invalid_trusted_capability_requirement",
            Self::ConflictingModuleDeclaration => "conflicting_module_declaration",
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
