use crate::code::DiagnosticCode;

/// Locale-neutral category for a compiler diagnostic.
///
/// Variants are grouped by owning phase.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticKind {
    /// A requested source file could not be read.
    SourceFileReadFailed,
    /// Source input contains bytes that are not valid UTF-8.
    SourceInvalidUtf8,
    /// Source input count cannot fit in compact source IDs.
    SourceTooManyInputs,
    /// Source text is too large for compact byte offsets.
    SourceTextTooLarge,
    /// The compilation request does not contain any source inputs.
    RequestMissingSourceInput,
    /// A compilation request source input is not valid.
    RequestInvalidSourceInput,
    /// The requested worker budget is not valid.
    RequestInvalidWorkerBudget,
    /// Source input contains a character that the lexer cannot accept.
    LexicalInvalidCharacter,
    /// Source input contains a byte order mark after the start of the source.
    LexicalMisplacedBom,
    /// Source input contains a lone carriage return outside a block comment.
    LexicalLoneCarriageReturn,
    /// Source input contains non-ASCII identifier text.
    LexicalNonAsciiIdentifier,
    /// Source input contains an identifier spelling that is not valid.
    LexicalInvalidIdentifier,
    /// Source input contains an operator or punctuation spelling that is not valid.
    LexicalInvalidOperatorOrPunctuation,
    /// Source input contains a malformed numeric literal spelling.
    LexicalMalformedNumericLiteral,
    /// Source input contains a numeric suffix other than imaginary `i`.
    LexicalInvalidNumericSuffix,
    /// Source input contains a malformed character literal spelling.
    LexicalMalformedCharacterLiteral,
    /// A character literal reaches the end of input or a line break before its terminator.
    LexicalUnterminatedCharacterLiteral,
    /// A string literal reaches the end of input or a line break before its terminator.
    LexicalUnterminatedStringLiteral,
    /// Source input contains an escape sequence that is not accepted.
    LexicalUnknownEscape,
    /// Source input contains a Unicode escape that is not valid.
    LexicalInvalidUnicodeEscape,
    /// A block comment reaches the end of input before its terminator.
    LexicalUnterminatedBlockComment,
    /// The parser expected a token that is missing at this source position.
    SyntaxExpectedToken,
    /// The parser expected an expression at this source position.
    SyntaxExpectedExpression,
    /// The parser reached the end of input before the current syntax construct was complete.
    SyntaxUnexpectedEof,
    /// A declaration name is repeated in the same declaration domain.
    DeclarationDuplicateName,
    /// Split declarations of one module disagree on effective visibility.
    DeclarationConflictingModuleVisibility,
    /// Split declarations of one module disagree on trusted-module state.
    DeclarationConflictingModuleTrust,
    /// A package interface does not start with the required magic bytes.
    InterfaceInvalidMagic,
    /// A package interface uses a wire-format revision this compiler does not implement.
    InterfaceUnsupportedFormatRevision,
    /// A package interface requires a different language semantic revision.
    InterfaceUnsupportedLanguageRevision,
    /// A package interface declares an invalid byte order or unsupported required flags.
    InterfaceUnsupportedEncoding,
    /// A package interface ends before a required structural value is complete.
    InterfaceTruncated,
    /// A package-interface header or section directory is malformed.
    InterfaceMalformed,
    /// A package interface does not match its declared artifact or content hash.
    InterfaceHashMismatch,
    /// One package-interface section does not match its declared checksum.
    InterfaceSectionChecksumMismatch,
    /// Untrusted package-interface input exceeds a configured resource ceiling.
    InterfaceResourceLimitExceeded,
    /// A package interface does not describe the package selected by package resolution.
    InterfacePackageIdentityMismatch,
    /// A package interface does not describe the product selected by package resolution.
    InterfaceProductIdentityMismatch,
    /// Selected dependency interfaces do not form the exact required dependency graph.
    InterfaceDependencyGraphInvalid,
    /// Decoded package-interface semantic facts cannot be used in this compilation.
    InterfaceSemanticFactsInvalid,
    /// A package-interface command was invoked without the required command shape.
    InterfaceCommandUsage,
    /// A package-interface command action is not recognized.
    InterfaceCommandUnexpectedAction,
    /// A package-interface command received an argument it does not accept.
    InterfaceCommandUnexpectedArgument,
    /// A package-interface section option is missing its section name.
    InterfaceCommandMissingSectionName,
    /// A package-interface section name is not recognized.
    InterfaceCommandUnknownSectionName,
    /// A package-interface artifact could not be read from the host.
    InterfaceArtifactReadFailed,
    /// Structured package-interface command output could not be rendered.
    InterfaceCommandOutputFailed,
    /// Name binding could not find a declaration or local with the requested spelling.
    BindingUnresolvedName,
    /// Name binding found more than one candidate for one ordinary name.
    BindingAmbiguousName,
    /// Name binding found candidates that are not visible in the current context.
    BindingInaccessibleName,
    /// Name binding found an ordinary name in a different semantic category.
    BindingWrongNameKind,
    /// Name binding found a candidate whose declaration surface is malformed.
    BindingMalformedName,
    /// A local declaration attempts to shadow an existing ordinary name.
    BindingNameAlreadyDefined,
    /// Alternatives do not introduce one coherent set of pattern bindings.
    BindingIncoherentAlternativePattern,
    /// A callable ABI directive does not name a supported ABI.
    BindingInvalidCallableAbi,
    /// A callable surface contains more than one ABI directive.
    BindingDuplicateCallableAbi,
    /// An expression's established type is incompatible with its expected type.
    CheckingIncompatibleExpressionType,
    /// Available constraints cannot establish an expression's canonical type.
    CheckingCannotInferExpressionType,
    /// An expression is not permitted in compile-time constant context.
    CheckingInvalidConstantExpression,
    /// A fixed array length is not greater than zero.
    CheckingArrayLengthNotPositive,
    /// A literal value cannot be represented by its selected type.
    CheckingConstantLiteralNotRepresentable,
    /// Constant evaluation exhausted its deterministic operation budget.
    CheckingConstantEvaluationStepLimitExceeded,
    /// Constant evaluation exhausted its deterministic aggregate-element budget.
    CheckingConstantAggregateLimitExceeded,
    /// Constant evaluation exhausted its deterministic literal-byte budget.
    CheckingConstantLiteralSizeLimitExceeded,
    /// Constant definitions form a direct or transitive dependency cycle.
    CheckingCyclicConstantDefinition,
    /// No available candidate can perform the requested semantic operation.
    CheckingNoApplicableCandidate,
    /// More than one candidate can perform the requested semantic operation.
    CheckingAmbiguousCandidate,
    /// Matching candidates exist but cannot be accessed from the current context.
    CheckingInaccessibleCandidate,
    /// Candidate parameter or operand types do not accept the supplied expressions.
    CheckingIncompatibleCandidate,
    /// The selected target does not provide the required scalar representation.
    CheckingTargetRepresentationUnavailable,
    /// The selected target does not provide the required callable ABI.
    CheckingTargetCallableAbiUnavailable,
    /// The selected callable ABI does not accept one by-value representation.
    CheckingTargetAbiRepresentationUnsupported,
    /// The selected target cannot represent the required alignment.
    CheckingTargetAlignmentUnsupported,
    /// A required planned artifact contribution was not supplied.
    EmissionMissingContribution,
    /// An artifact contribution does not satisfy the immutable emission plan.
    EmissionInvalidContribution,
    /// Completed artifact content could not be read for validation or publication.
    EmissionArtifactReadFailed,
    /// A planned external artifact destination could not be opened.
    EmissionArtifactOpenFailed,
    /// Complete artifact bytes could not be written to their destination.
    EmissionArtifactWriteFailed,
    /// A completed artifact destination could not be flushed.
    EmissionArtifactFlushFailed,
    /// Completed artifact bytes do not match the producer-supplied digest.
    EmissionArtifactDigestMismatch,
    /// Completed artifact bytes do not match the producer-supplied length.
    EmissionArtifactLengthMismatch,
    /// A complete indirect artifact write could not be committed.
    EmissionArtifactCommitFailed,
}

impl DiagnosticKind {
    /// Returns the numeric code for this diagnostic category.
    pub const fn code(self) -> DiagnosticCode {
        let raw = match self {
            Self::SourceFileReadFailed => 1001,
            Self::SourceInvalidUtf8 => 1002,
            Self::SourceTooManyInputs => 1003,
            Self::SourceTextTooLarge => 1004,
            Self::RequestMissingSourceInput => 1101,
            Self::RequestInvalidSourceInput => 1102,
            Self::RequestInvalidWorkerBudget => 1103,
            Self::LexicalInvalidCharacter => 2001,
            Self::LexicalMisplacedBom => 2002,
            Self::LexicalLoneCarriageReturn => 2003,
            Self::LexicalNonAsciiIdentifier => 2004,
            Self::LexicalInvalidIdentifier => 2005,
            Self::LexicalInvalidOperatorOrPunctuation => 2006,
            Self::LexicalMalformedNumericLiteral => 2007,
            Self::LexicalInvalidNumericSuffix => 2008,
            Self::LexicalMalformedCharacterLiteral => 2009,
            Self::LexicalUnterminatedCharacterLiteral => 2010,
            Self::LexicalUnterminatedStringLiteral => 2011,
            Self::LexicalUnknownEscape => 2012,
            Self::LexicalInvalidUnicodeEscape => 2013,
            Self::LexicalUnterminatedBlockComment => 2014,
            Self::SyntaxExpectedToken => 3001,
            Self::SyntaxExpectedExpression => 3005,
            Self::SyntaxUnexpectedEof => 3003,
            Self::DeclarationDuplicateName => 4001,
            Self::DeclarationConflictingModuleVisibility => 4002,
            Self::DeclarationConflictingModuleTrust => 4003,
            Self::InterfaceInvalidMagic => 5001,
            Self::InterfaceUnsupportedFormatRevision => 5002,
            Self::InterfaceUnsupportedLanguageRevision => 5003,
            Self::InterfaceUnsupportedEncoding => 5004,
            Self::InterfaceTruncated => 5005,
            Self::InterfaceMalformed => 5006,
            Self::InterfaceHashMismatch => 5007,
            Self::InterfaceSectionChecksumMismatch => 5008,
            Self::InterfaceResourceLimitExceeded => 5009,
            Self::InterfacePackageIdentityMismatch => 5010,
            Self::InterfaceProductIdentityMismatch => 5011,
            Self::InterfaceDependencyGraphInvalid => 5012,
            Self::InterfaceSemanticFactsInvalid => 5013,
            Self::InterfaceCommandUsage => 5014,
            Self::InterfaceCommandUnexpectedAction => 5015,
            Self::InterfaceCommandUnexpectedArgument => 5016,
            Self::InterfaceCommandMissingSectionName => 5017,
            Self::InterfaceCommandUnknownSectionName => 5018,
            Self::InterfaceArtifactReadFailed => 5019,
            Self::InterfaceCommandOutputFailed => 5020,
            Self::BindingUnresolvedName => 6001,
            Self::BindingAmbiguousName => 6002,
            Self::BindingInaccessibleName => 6003,
            Self::BindingWrongNameKind => 6004,
            Self::BindingMalformedName => 6005,
            Self::BindingNameAlreadyDefined => 6006,
            Self::BindingIncoherentAlternativePattern => 6007,
            Self::BindingInvalidCallableAbi => 6008,
            Self::BindingDuplicateCallableAbi => 6009,
            Self::CheckingIncompatibleExpressionType => 7001,
            Self::CheckingCannotInferExpressionType => 7002,
            Self::CheckingInvalidConstantExpression => 7003,
            Self::CheckingArrayLengthNotPositive => 7013,
            Self::CheckingConstantLiteralNotRepresentable => 7004,
            Self::CheckingConstantEvaluationStepLimitExceeded => 7005,
            Self::CheckingConstantAggregateLimitExceeded => 7006,
            Self::CheckingConstantLiteralSizeLimitExceeded => 7007,
            Self::CheckingCyclicConstantDefinition => 7008,
            Self::CheckingNoApplicableCandidate => 7009,
            Self::CheckingAmbiguousCandidate => 7010,
            Self::CheckingInaccessibleCandidate => 7011,
            Self::CheckingIncompatibleCandidate => 7012,
            Self::CheckingTargetRepresentationUnavailable => 7014,
            Self::CheckingTargetCallableAbiUnavailable => 7015,
            Self::CheckingTargetAlignmentUnsupported => 7016,
            Self::CheckingTargetAbiRepresentationUnsupported => 7017,
            Self::EmissionMissingContribution => 9001,
            Self::EmissionInvalidContribution => 9002,
            Self::EmissionArtifactReadFailed => 9003,
            Self::EmissionArtifactOpenFailed => 9004,
            Self::EmissionArtifactWriteFailed => 9005,
            Self::EmissionArtifactFlushFailed => 9006,
            Self::EmissionArtifactDigestMismatch => 9007,
            Self::EmissionArtifactLengthMismatch => 9008,
            Self::EmissionArtifactCommitFailed => 9009,
        };

        DiagnosticCode::new(raw)
    }

    /// Returns the machine key for this diagnostic category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceFileReadFailed => "source_file_read_failed",
            Self::SourceInvalidUtf8 => "source_invalid_utf8",
            Self::SourceTooManyInputs => "source_too_many_inputs",
            Self::SourceTextTooLarge => "source_text_too_large",
            Self::RequestMissingSourceInput => "request_missing_source_input",
            Self::RequestInvalidSourceInput => "request_invalid_source_input",
            Self::RequestInvalidWorkerBudget => "request_invalid_worker_budget",
            Self::LexicalInvalidCharacter => "lexical_invalid_character",
            Self::LexicalMisplacedBom => "lexical_misplaced_bom",
            Self::LexicalLoneCarriageReturn => "lexical_lone_carriage_return",
            Self::LexicalNonAsciiIdentifier => "lexical_non_ascii_identifier",
            Self::LexicalInvalidIdentifier => "lexical_invalid_identifier",
            Self::LexicalInvalidOperatorOrPunctuation => "lexical_invalid_operator_or_punctuation",
            Self::LexicalMalformedNumericLiteral => "lexical_malformed_numeric_literal",
            Self::LexicalInvalidNumericSuffix => "lexical_invalid_numeric_suffix",
            Self::LexicalMalformedCharacterLiteral => "lexical_malformed_character_literal",
            Self::LexicalUnterminatedCharacterLiteral => "lexical_unterminated_character_literal",
            Self::LexicalUnterminatedStringLiteral => "lexical_unterminated_string_literal",
            Self::LexicalUnknownEscape => "lexical_unknown_escape",
            Self::LexicalInvalidUnicodeEscape => "lexical_invalid_unicode_escape",
            Self::LexicalUnterminatedBlockComment => "lexical_unterminated_block_comment",
            Self::SyntaxExpectedToken => "syntax_expected_token",
            Self::SyntaxExpectedExpression => "syntax_expected_expression",
            Self::SyntaxUnexpectedEof => "syntax_unexpected_eof",
            Self::DeclarationDuplicateName => "declaration_duplicate_name",
            Self::DeclarationConflictingModuleVisibility => {
                "declaration_conflicting_module_visibility"
            }
            Self::DeclarationConflictingModuleTrust => "declaration_conflicting_module_trust",
            Self::InterfaceInvalidMagic => "interface_invalid_magic",
            Self::InterfaceUnsupportedFormatRevision => "interface_unsupported_format_revision",
            Self::InterfaceUnsupportedLanguageRevision => "interface_unsupported_language_revision",
            Self::InterfaceUnsupportedEncoding => "interface_unsupported_encoding",
            Self::InterfaceTruncated => "interface_truncated",
            Self::InterfaceMalformed => "interface_malformed",
            Self::InterfaceHashMismatch => "interface_hash_mismatch",
            Self::InterfaceSectionChecksumMismatch => "interface_section_checksum_mismatch",
            Self::InterfaceResourceLimitExceeded => "interface_resource_limit_exceeded",
            Self::InterfacePackageIdentityMismatch => "interface_package_identity_mismatch",
            Self::InterfaceProductIdentityMismatch => "interface_product_identity_mismatch",
            Self::InterfaceDependencyGraphInvalid => "interface_dependency_graph_invalid",
            Self::InterfaceSemanticFactsInvalid => "interface_semantic_facts_invalid",
            Self::InterfaceCommandUsage => "interface_command_usage",
            Self::InterfaceCommandUnexpectedAction => "interface_command_unexpected_action",
            Self::InterfaceCommandUnexpectedArgument => "interface_command_unexpected_argument",
            Self::InterfaceCommandMissingSectionName => "interface_command_missing_section_name",
            Self::InterfaceCommandUnknownSectionName => "interface_command_unknown_section_name",
            Self::InterfaceArtifactReadFailed => "interface_artifact_read_failed",
            Self::InterfaceCommandOutputFailed => "interface_command_output_failed",
            Self::BindingUnresolvedName => "binding_unresolved_name",
            Self::BindingAmbiguousName => "binding_ambiguous_name",
            Self::BindingInaccessibleName => "binding_inaccessible_name",
            Self::BindingWrongNameKind => "binding_wrong_name_kind",
            Self::BindingMalformedName => "binding_malformed_name",
            Self::BindingNameAlreadyDefined => "binding_name_already_defined",
            Self::BindingIncoherentAlternativePattern => "binding_incoherent_alternative_pattern",
            Self::BindingInvalidCallableAbi => "binding_invalid_callable_abi",
            Self::BindingDuplicateCallableAbi => "binding_duplicate_callable_abi",
            Self::CheckingIncompatibleExpressionType => "checking_incompatible_expression_type",
            Self::CheckingCannotInferExpressionType => "checking_cannot_infer_expression_type",
            Self::CheckingInvalidConstantExpression => "checking_invalid_constant_expression",
            Self::CheckingArrayLengthNotPositive => "checking_array_length_not_positive",
            Self::CheckingConstantLiteralNotRepresentable => {
                "checking_constant_literal_not_representable"
            }
            Self::CheckingConstantEvaluationStepLimitExceeded => {
                "checking_constant_evaluation_step_limit_exceeded"
            }
            Self::CheckingConstantAggregateLimitExceeded => {
                "checking_constant_aggregate_limit_exceeded"
            }
            Self::CheckingConstantLiteralSizeLimitExceeded => {
                "checking_constant_literal_size_limit_exceeded"
            }
            Self::CheckingCyclicConstantDefinition => "checking_cyclic_constant_definition",
            Self::CheckingNoApplicableCandidate => "checking_no_applicable_candidate",
            Self::CheckingAmbiguousCandidate => "checking_ambiguous_candidate",
            Self::CheckingInaccessibleCandidate => "checking_inaccessible_candidate",
            Self::CheckingIncompatibleCandidate => "checking_incompatible_candidate",
            Self::CheckingTargetRepresentationUnavailable => {
                "checking_target_representation_unavailable"
            }
            Self::CheckingTargetCallableAbiUnavailable => {
                "checking_target_callable_abi_unavailable"
            }
            Self::CheckingTargetAlignmentUnsupported => "checking_target_alignment_unsupported",
            Self::CheckingTargetAbiRepresentationUnsupported => {
                "checking_target_abi_representation_unsupported"
            }
            Self::EmissionMissingContribution => "emission_missing_contribution",
            Self::EmissionInvalidContribution => "emission_invalid_contribution",
            Self::EmissionArtifactReadFailed => "emission_artifact_read_failed",
            Self::EmissionArtifactOpenFailed => "emission_artifact_open_failed",
            Self::EmissionArtifactWriteFailed => "emission_artifact_write_failed",
            Self::EmissionArtifactFlushFailed => "emission_artifact_flush_failed",
            Self::EmissionArtifactDigestMismatch => "emission_artifact_digest_mismatch",
            Self::EmissionArtifactLengthMismatch => "emission_artifact_length_mismatch",
            Self::EmissionArtifactCommitFailed => "emission_artifact_commit_failed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DiagnosticKind;

    #[test]
    fn diagnostic_kinds_expose_stable_machine_keys() {
        assert_eq!(
            DiagnosticKind::LexicalUnterminatedBlockComment.as_str(),
            "lexical_unterminated_block_comment"
        );

        assert_eq!(
            DiagnosticKind::SyntaxExpectedToken.as_str(),
            "syntax_expected_token"
        );

        assert_eq!(
            DiagnosticKind::DeclarationConflictingModuleVisibility.as_str(),
            "declaration_conflicting_module_visibility"
        );
    }

    #[test]
    fn diagnostic_kinds_expose_stable_numeric_codes() {
        assert_eq!(DiagnosticKind::SourceInvalidUtf8.code().raw(), 1002);

        assert_eq!(
            DiagnosticKind::LexicalUnterminatedBlockComment.code().raw(),
            2014
        );

        assert_eq!(DiagnosticKind::SyntaxExpectedExpression.code().raw(), 3005);
        assert_eq!(DiagnosticKind::DeclarationDuplicateName.code().raw(), 4001);
        assert_eq!(DiagnosticKind::BindingUnresolvedName.code().raw(), 6001);
    }

    #[test]
    fn diagnostic_kind_codes_are_unique() {
        let mut codes = Vec::new();

        for kind in all_diagnostic_kinds() {
            assert!(
                !codes.contains(&kind.code()),
                "duplicate diagnostic code for {kind:?}"
            );

            codes.push(kind.code());
        }
    }

    fn all_diagnostic_kinds() -> [DiagnosticKind; 82] {
        [
            DiagnosticKind::SourceFileReadFailed,
            DiagnosticKind::SourceInvalidUtf8,
            DiagnosticKind::SourceTooManyInputs,
            DiagnosticKind::SourceTextTooLarge,
            DiagnosticKind::RequestMissingSourceInput,
            DiagnosticKind::RequestInvalidSourceInput,
            DiagnosticKind::RequestInvalidWorkerBudget,
            DiagnosticKind::LexicalInvalidCharacter,
            DiagnosticKind::LexicalMisplacedBom,
            DiagnosticKind::LexicalLoneCarriageReturn,
            DiagnosticKind::LexicalNonAsciiIdentifier,
            DiagnosticKind::LexicalInvalidIdentifier,
            DiagnosticKind::LexicalInvalidOperatorOrPunctuation,
            DiagnosticKind::LexicalMalformedNumericLiteral,
            DiagnosticKind::LexicalInvalidNumericSuffix,
            DiagnosticKind::LexicalMalformedCharacterLiteral,
            DiagnosticKind::LexicalUnterminatedCharacterLiteral,
            DiagnosticKind::LexicalUnterminatedStringLiteral,
            DiagnosticKind::LexicalUnknownEscape,
            DiagnosticKind::LexicalInvalidUnicodeEscape,
            DiagnosticKind::LexicalUnterminatedBlockComment,
            DiagnosticKind::SyntaxExpectedToken,
            DiagnosticKind::SyntaxExpectedExpression,
            DiagnosticKind::SyntaxUnexpectedEof,
            DiagnosticKind::DeclarationDuplicateName,
            DiagnosticKind::DeclarationConflictingModuleVisibility,
            DiagnosticKind::DeclarationConflictingModuleTrust,
            DiagnosticKind::InterfaceInvalidMagic,
            DiagnosticKind::InterfaceUnsupportedFormatRevision,
            DiagnosticKind::InterfaceUnsupportedLanguageRevision,
            DiagnosticKind::InterfaceUnsupportedEncoding,
            DiagnosticKind::InterfaceTruncated,
            DiagnosticKind::InterfaceMalformed,
            DiagnosticKind::InterfaceHashMismatch,
            DiagnosticKind::InterfaceSectionChecksumMismatch,
            DiagnosticKind::InterfaceResourceLimitExceeded,
            DiagnosticKind::InterfacePackageIdentityMismatch,
            DiagnosticKind::InterfaceProductIdentityMismatch,
            DiagnosticKind::InterfaceDependencyGraphInvalid,
            DiagnosticKind::InterfaceSemanticFactsInvalid,
            DiagnosticKind::InterfaceCommandUsage,
            DiagnosticKind::InterfaceCommandUnexpectedAction,
            DiagnosticKind::InterfaceCommandUnexpectedArgument,
            DiagnosticKind::InterfaceCommandMissingSectionName,
            DiagnosticKind::InterfaceCommandUnknownSectionName,
            DiagnosticKind::InterfaceArtifactReadFailed,
            DiagnosticKind::InterfaceCommandOutputFailed,
            DiagnosticKind::BindingUnresolvedName,
            DiagnosticKind::BindingAmbiguousName,
            DiagnosticKind::BindingInaccessibleName,
            DiagnosticKind::BindingWrongNameKind,
            DiagnosticKind::BindingMalformedName,
            DiagnosticKind::BindingNameAlreadyDefined,
            DiagnosticKind::BindingIncoherentAlternativePattern,
            DiagnosticKind::BindingInvalidCallableAbi,
            DiagnosticKind::BindingDuplicateCallableAbi,
            DiagnosticKind::CheckingIncompatibleExpressionType,
            DiagnosticKind::CheckingCannotInferExpressionType,
            DiagnosticKind::CheckingInvalidConstantExpression,
            DiagnosticKind::CheckingArrayLengthNotPositive,
            DiagnosticKind::CheckingConstantLiteralNotRepresentable,
            DiagnosticKind::CheckingConstantEvaluationStepLimitExceeded,
            DiagnosticKind::CheckingConstantAggregateLimitExceeded,
            DiagnosticKind::CheckingConstantLiteralSizeLimitExceeded,
            DiagnosticKind::CheckingCyclicConstantDefinition,
            DiagnosticKind::CheckingNoApplicableCandidate,
            DiagnosticKind::CheckingAmbiguousCandidate,
            DiagnosticKind::CheckingInaccessibleCandidate,
            DiagnosticKind::CheckingIncompatibleCandidate,
            DiagnosticKind::CheckingTargetRepresentationUnavailable,
            DiagnosticKind::CheckingTargetCallableAbiUnavailable,
            DiagnosticKind::CheckingTargetAlignmentUnsupported,
            DiagnosticKind::CheckingTargetAbiRepresentationUnsupported,
            DiagnosticKind::EmissionMissingContribution,
            DiagnosticKind::EmissionInvalidContribution,
            DiagnosticKind::EmissionArtifactReadFailed,
            DiagnosticKind::EmissionArtifactOpenFailed,
            DiagnosticKind::EmissionArtifactWriteFailed,
            DiagnosticKind::EmissionArtifactFlushFailed,
            DiagnosticKind::EmissionArtifactDigestMismatch,
            DiagnosticKind::EmissionArtifactLengthMismatch,
            DiagnosticKind::EmissionArtifactCommitFailed,
        ]
    }
}
