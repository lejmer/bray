use crate::argument::DiagnosticArg;

/// Structured note attached to a diagnostic.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DiagnosticNote {
    kind: DiagnosticNoteKind,
    args: Vec<DiagnosticArg>,
}

impl DiagnosticNote {
    /// Creates a note with no message arguments.
    pub fn new(kind: DiagnosticNoteKind) -> Self {
        Self {
            kind,
            args: Vec::new(),
        }
    }

    /// Adds one typed argument to the note.
    pub fn with_arg(mut self, arg: DiagnosticArg) -> Self {
        self.args.push(arg);

        self
    }

    /// Returns the stable note category.
    pub const fn kind(&self) -> DiagnosticNoteKind {
        self.kind
    }

    /// Returns the typed note arguments.
    pub fn args(&self) -> &[DiagnosticArg] {
        &self.args
    }
}

/// Stable category for a diagnostic note.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticNoteKind {
    /// Location reported by a structured-document parser.
    DocumentFailureLocation,
    /// Recover an expired or cleaned retained product.
    RebuildRetainedProduct,
    /// Source file readability requirement.
    SourceFileMustBeReadable,
    /// Source UTF-8 requirement.
    SourceMustBeUtf8,
    /// Requirement enforced by a source-format check.
    SourceMustMatchFormatterOutput,
    /// Source ID compactness limit.
    SourceIdsAreCompact,
    /// Source text offset compactness limit.
    SourceTextOffsetsAreCompact,
    /// Required source input.
    SourceInputRequired,
    /// Stable source identity requirement.
    SourceInputNeedsStableIdentity,
    /// Positive worker budget requirement.
    WorkerBudgetMustBePositive,
    /// Accepted package identity spelling and authority.
    PackageIdentityMustBeValid,
    /// Accepted package semantic version declaration.
    PackageVersionMustBeValid,
    /// Character rejected by the lexer.
    CharacterNotAccepted,
    /// Byte order mark placement rule.
    BomOnlyAllowedAtStart,
    /// Accepted line break spellings.
    LineBreaksMustBeLfOrCrlf,
    /// Identifier character set rule.
    IdentifiersMustBeAscii,
    /// Identifier spelling rule.
    IdentifierSpellingMustBeValid,
    /// Numeric suffix rule.
    OnlyImaginaryNumericSuffix,
    /// Character literal scalar count rule.
    CharacterLiteralMustContainOneScalar,
    /// Character literal terminator rule.
    CharacterLiteralNeedsTerminator,
    /// String literal terminator rule.
    StringLiteralNeedsTerminator,
    /// Escape sequence spelling rule.
    EscapeMustBeKnown,
    /// Unicode escape scalar value rule.
    UnicodeEscapeMustBeScalar,
    /// Block comment terminator rule.
    BlockCommentNeedsTerminator,
    /// Accepted callable ABI directive forms.
    CallableAbiDirectiveMustNameSupportedAbi,
    /// Accepted directive argument forms.
    DirectiveArgumentMustHaveCompleteForm,
    /// Type-inference recovery guidance.
    TypeInferenceNeedsConstraint,
    /// Compile-time expression recovery guidance.
    ConstantExpressionMustBeEvaluable,
    /// Constant-evaluation resource-limit recovery guidance.
    ConstantEvaluationMustFitLimits,
    /// Accepted type-layout directive forms and constraints.
    TypeLayoutDirectiveForms,
    /// Accepted union-tag directive forms and consistency rules.
    UnionTagDirectiveForms,
    /// Requirements imposed by an explicit copy contract.
    CopyContractRequirements,
    /// Recovery guidance for a type that cannot be stored inline.
    StoredTypeRequiresIndirection,
    /// Recovery guidance for propagation without a compatible result boundary.
    PropagationBoundaryMustMatch,
    /// Recovery guidance for fixed-array generator cardinality.
    ArrayGeneratorMustYieldOncePerElement,
    /// Source contract required for callback-state access.
    CallbackStateRequirements,
    /// Recovery guidance for a refutable pattern used in an irrefutable context.
    RefutablePatternRequiresConditionalContext,
    /// Recovery guidance for a binding inside a structural boolean test.
    PatternTestMustNotBind,
    /// Recovery guidance for an ambiguous semantic selection.
    SelectionMustBeDisambiguated,
    /// Package-selection context for an external interface diagnostic.
    InterfaceDependencyContext,
    /// Exact product, target, and driver context for a native link diagnostic.
    LinkPlanContext,
    /// Recovery guidance after an external native-link tool reports an unsuccessful exit.
    ExternalToolExitRequiresCorrection,
    /// Recovery guidance when native product preparation begins before compilation succeeds.
    NativeProductPreparationRecovery,
    /// Recovery guidance for an unusable selected runtime artifact.
    RuntimeArtifactMustBeUsable,
    /// One exact unavailable state required across an await.
    AwaitDependencyUnavailable,
    /// Recovery guidance for a value whose dependency cannot leave its storage scope.
    EscapingStorageDependencyResolution,
    /// Recovery guidance for operations that require an asynchronous callable body.
    AsynchronousCallableRequired,
    /// Recovery guidance for executable product entrypoint selection.
    ExecutableEntrypointRequired,
    /// Product-kind requirement for the entrypoint directive.
    EntrypointDirectiveRequiresExecutableProduct,
    /// Recovery guidance for a callable selected as a product entry.
    ProductEntryRequirements,
    /// Accepted forms of the test directive.
    TestDirectiveRequirements,
    /// Uniqueness requirement for source-level test identities.
    UniqueTestIdentityRequired,
    /// Visibility requirement for dependencies exposed by public declarations.
    PublicDependencyRequired,
    /// Guidance for reporting a compiler-owned invariant failure.
    ReportCompilerDefect,
    /// Supply the declared result on each normal callable exit.
    ReturnRequiredResult,
}

impl DiagnosticNoteKind {
    /// Returns the stable machine key for this note category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DocumentFailureLocation => "document_failure_location",
            Self::RebuildRetainedProduct => "rebuild_retained_product",
            Self::SourceFileMustBeReadable => "source_file_must_be_readable",
            Self::SourceMustBeUtf8 => "source_must_be_utf8",
            Self::SourceMustMatchFormatterOutput => "source_must_match_formatter_output",
            Self::SourceIdsAreCompact => "source_ids_are_compact",
            Self::SourceTextOffsetsAreCompact => "source_text_offsets_are_compact",
            Self::SourceInputRequired => "source_input_required",
            Self::SourceInputNeedsStableIdentity => "source_input_needs_stable_identity",
            Self::WorkerBudgetMustBePositive => "worker_budget_must_be_positive",
            Self::PackageIdentityMustBeValid => "package_identity_must_be_valid",
            Self::PackageVersionMustBeValid => "package_version_must_be_valid",
            Self::CharacterNotAccepted => "character_not_accepted",
            Self::BomOnlyAllowedAtStart => "bom_only_allowed_at_start",
            Self::LineBreaksMustBeLfOrCrlf => "line_breaks_must_be_lf_or_crlf",
            Self::IdentifiersMustBeAscii => "identifiers_must_be_ascii",
            Self::IdentifierSpellingMustBeValid => "identifier_spelling_must_be_valid",
            Self::OnlyImaginaryNumericSuffix => "only_imaginary_numeric_suffix",
            Self::CharacterLiteralMustContainOneScalar => {
                "character_literal_must_contain_one_scalar"
            }
            Self::CharacterLiteralNeedsTerminator => "character_literal_needs_terminator",
            Self::StringLiteralNeedsTerminator => "string_literal_needs_terminator",
            Self::EscapeMustBeKnown => "escape_must_be_known",
            Self::UnicodeEscapeMustBeScalar => "unicode_escape_must_be_scalar",
            Self::BlockCommentNeedsTerminator => "block_comment_needs_terminator",
            Self::CallableAbiDirectiveMustNameSupportedAbi => {
                "callable_abi_directive_must_name_supported_abi"
            }
            Self::DirectiveArgumentMustHaveCompleteForm => {
                "directive_argument_must_have_complete_form"
            }
            Self::TypeInferenceNeedsConstraint => "type_inference_needs_constraint",
            Self::ConstantExpressionMustBeEvaluable => "constant_expression_must_be_evaluable",
            Self::ConstantEvaluationMustFitLimits => "constant_evaluation_must_fit_limits",
            Self::TypeLayoutDirectiveForms => "type_layout_directive_forms",
            Self::UnionTagDirectiveForms => "union_tag_directive_forms",
            Self::CopyContractRequirements => "copy_contract_requirements",
            Self::StoredTypeRequiresIndirection => "stored_type_requires_indirection",
            Self::PropagationBoundaryMustMatch => "propagation_boundary_must_match",
            Self::ArrayGeneratorMustYieldOncePerElement => {
                "array_generator_must_yield_once_per_element"
            }
            Self::CallbackStateRequirements => "callback_state_requirements",
            Self::RefutablePatternRequiresConditionalContext => {
                "refutable_pattern_requires_conditional_context"
            }
            Self::PatternTestMustNotBind => "pattern_test_must_not_bind",
            Self::SelectionMustBeDisambiguated => "selection_must_be_disambiguated",
            Self::InterfaceDependencyContext => "interface_dependency_context",
            Self::LinkPlanContext => "link_plan_context",
            Self::ExternalToolExitRequiresCorrection => "external_tool_exit_requires_correction",
            Self::NativeProductPreparationRecovery => "native_product_preparation_recovery",
            Self::RuntimeArtifactMustBeUsable => "runtime_artifact_must_be_usable",
            Self::AwaitDependencyUnavailable => "await_dependency_unavailable",
            Self::EscapingStorageDependencyResolution => "escaping_storage_dependency_resolution",
            Self::AsynchronousCallableRequired => "asynchronous_callable_required",
            Self::ExecutableEntrypointRequired => "executable_entrypoint_required",
            Self::EntrypointDirectiveRequiresExecutableProduct => {
                "entrypoint_directive_requires_executable_product"
            }
            Self::ProductEntryRequirements => "product_entry_requirements",
            Self::TestDirectiveRequirements => "test_directive_requirements",
            Self::UniqueTestIdentityRequired => "unique_test_identity_required",
            Self::PublicDependencyRequired => "public_dependency_required",
            Self::ReportCompilerDefect => "report_compiler_defect",
            Self::ReturnRequiredResult => "return_required_result",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DiagnosticNote, DiagnosticNoteKind};
    use crate::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

    #[test]
    fn notes_carry_kind_and_typed_args() {
        let note =
            DiagnosticNote::new(DiagnosticNoteKind::SourceMustBeUtf8).with_arg(DiagnosticArg::new(
                DiagnosticArgName::ByteCount,
                DiagnosticArgValue::ByteCount(2),
            ));

        assert_eq!(note.kind(), DiagnosticNoteKind::SourceMustBeUtf8);
        assert_eq!(note.kind().as_str(), "source_must_be_utf8");

        assert_eq!(
            note.args(),
            &[DiagnosticArg::new(
                DiagnosticArgName::ByteCount,
                DiagnosticArgValue::ByteCount(2)
            )]
        );
    }
}
