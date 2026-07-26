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
    /// A compilation request selects one logical source more than once.
    RequestDuplicateSourceInput,
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
    /// A declaration repeats one modifier.
    DeclarationDuplicateModifier,
    /// A declaration combines modifiers that cannot apply together.
    DeclarationIncompatibleModifiers,
    /// A modifier is not valid for the declaration's form or context.
    DeclarationInvalidModifier,
    /// A declaration does not provide its required body.
    DeclarationBodyRequired,
    /// A declaration provides a body despite requiring an opaque or external surface.
    DeclarationBodyNotAllowed,
    /// A declaration repeats one non-repeatable directive.
    DeclarationDuplicateDirective,
    /// A declaration combines directives that cannot apply together.
    DeclarationIncompatibleDirectives,
    /// A directive is attached to a declaration form that does not accept it.
    DeclarationInvalidDirectiveTarget,
    /// A positional parameter appears after a named-only parameter.
    DeclarationInvalidParameterOrder,
    /// A declaration appears in a container that does not permit its form.
    DeclarationInvalidMemberPlacement,
    /// One declaration container repeats a lifecycle slot.
    DeclarationDuplicateLifecycleSlot,
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
    /// An ordinary predicate declaration does not provide its required body.
    BindingPredicateBodyRequired,
    /// A trusted predicate declaration provides a body despite being opaque.
    BindingTrustedPredicateBodyNotAllowed,
    /// An export declaration depends on itself through one or more module surfaces.
    BindingCyclicModuleExport,
    /// A re-exported name conflicts with another declaration in the exporting module.
    BindingConflictingModuleExport,
    /// An export path resolves to an entity that cannot enter a module export surface.
    BindingInvalidModuleExportTarget,
    /// A directive argument cannot be bound because its syntax is malformed.
    BindingMalformedDirectiveArgument,
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
    /// Constant evaluation exceeded its deterministic exact-integer size limit.
    CheckingConstantIntegerSizeLimitExceeded,
    /// Constant definitions form a direct or transitive dependency cycle.
    CheckingCyclicConstantDefinition,
    /// A constant operation divides or takes a remainder by zero.
    CheckingConstantDivisionByZero,
    /// A constant operation result cannot be represented by its selected type.
    CheckingConstantValueNotRepresentable,
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
    /// A pattern form cannot match values of its established input type.
    CheckingIncompatiblePattern,
    /// A context requiring an irrefutable pattern received a refutable pattern.
    CheckingRefutablePattern,
    /// A match expression does not cover every value of its subject type.
    CheckingNonExhaustiveMatch,
    /// A match arm cannot be selected because earlier arms already cover it.
    CheckingUnreachableMatchArm,
    /// A pattern alternative cannot match values not covered by earlier alternatives.
    CheckingUnreachablePatternAlternative,
    /// A module declaration repeats one contribution-controlling directive.
    CheckingDuplicateModuleContributionDirective,
    /// A fixed-size array generator's required element count cannot be proven.
    CheckingArrayGeneratorCardinalityNotProvable,
    /// A stored field or payload does not have a valid finite outer representation.
    CheckingInvalidStoredType,
    /// A declared type contains an inline representation cycle.
    CheckingRecursiveTypeRepresentation,
    /// A layout directive does not describe a valid source-level layout contract.
    CheckingInvalidLayoutDirective,
    /// A declared implicit-copy contract is not satisfied by the represented type.
    CheckingInvalidCopyContract,
    /// A union tag contract is incomplete, duplicated, or otherwise invalid.
    CheckingInvalidUnionTag,
    /// Flow-sensitive fact analysis exceeded its deterministic capacity.
    CheckingRefinementCapacityExceeded,
    /// Storage is used before it is initialized on every incoming path.
    CheckingUseOfUninitializedStorage,
    /// Storage or substorage is used after ownership was moved from it.
    CheckingUseOfMovedStorage,
    /// An operation conflicts with an active overlapping borrow.
    CheckingConflictingBorrow,
    /// An operation requires mutation authority that is not available.
    CheckingMissingMutationAuthority,
    /// A copy operation was requested for a non-copyable type.
    CheckingTypeIsNotCopyable,
    /// An operation requires ownership of storage reached only through a borrow.
    CheckingMissingStorageOwnership,
    /// A nullable or union projection is inactive on the current control-flow path.
    CheckingInactiveStorageProjection,
    /// A trait implementation omits a required member fulfillment.
    CheckingMissingTraitFulfillment,
    /// A trait implementation declares a member that does not fulfill its trait.
    CheckingExtraTraitFulfillment,
    /// A trait implementation member is incompatible with its requirement.
    CheckingIncompatibleTraitFulfillment,
    /// A trait implementation declares one fulfillment slot more than once.
    CheckingDuplicateTraitFulfillment,
    /// Two participating implementation declarations can produce the same coherence key.
    CheckingOverlappingImplementation,
    /// Several trait applications share a subject and trait without one overload family.
    CheckingUngroupedImplementationOverloads,
    /// An implementation overload header does not name a supported subject and trait.
    CheckingInvalidImplementationOverloadHeader,
    /// An implementation overload arm is duplicated or incompatible with its family.
    CheckingInvalidImplementationOverloadArm,
    /// A callable overload arm does not name an accessible callable valid for its family.
    CheckingInvalidCallableOverloadArm,
    /// A callable declaration occurs more than once in one overload family.
    CheckingDuplicateCallableOverloadArm,
    /// A callable declaration belongs to more than one overload family.
    CheckingConflictingCallableOverloadFamily,
    /// Two callable overload arms have indistinguishable selection signatures.
    CheckingConflictingCallableOverloadSignature,
    /// A foreign callable omits one directive required by its boundary contract.
    CheckingMissingForeignCallableDirective,
    /// A foreign callable declaration does not establish a trusted boundary.
    CheckingForeignCallableRequiresTrusted,
    /// A foreign callable declaration omits a required trusted capability.
    CheckingForeignCallableRequiresCapability,
    /// A callable execution mode cannot cross the selected foreign ABI.
    CheckingForeignCallableExecutionUnsupported,
    /// A declared value type cannot cross the selected foreign ABI.
    CheckingForeignAbiTypeUnsupported,
    /// A native link directive does not provide a valid dependency requirement.
    CheckingInvalidNativeLinkDirective,
    /// A native symbol directive does not provide a valid external symbol name.
    CheckingInvalidNativeSymbolDirective,
    /// More than one source callable declares the same native symbol.
    CheckingDuplicateNativeSymbol,
    /// A native link requirement has no input supplied for the selected target.
    CheckingUnavailableNativeLinkInput,
    /// A callable body uses a trusted capability omitted from its declaration.
    CheckingUndeclaredTrustedCapability,
    /// A callable declaration names a trusted capability its body does not use.
    CheckingUnusedTrustedCapability,
    /// Trusted implementation capability use occurs outside a trusted callable.
    CheckingTrustedCapabilityRequiresTrustedCallable,
    /// A direct await occurs outside an asynchronous callable body.
    CheckingAwaitOutsideAsyncCallable,
    /// A future is started outside an active asynchronous callable body.
    CheckingTaskStartOutsideAsyncCallable,
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
            Self::RequestDuplicateSourceInput => 1104,
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
            Self::DeclarationDuplicateModifier => 4004,
            Self::DeclarationIncompatibleModifiers => 4005,
            Self::DeclarationBodyRequired => 4006,
            Self::DeclarationBodyNotAllowed => 4007,
            Self::DeclarationDuplicateDirective => 4008,
            Self::DeclarationInvalidMemberPlacement => 4009,
            Self::DeclarationDuplicateLifecycleSlot => 4010,
            Self::DeclarationInvalidModifier => 4011,
            Self::DeclarationInvalidDirectiveTarget => 4012,
            Self::DeclarationInvalidParameterOrder => 4013,
            Self::DeclarationIncompatibleDirectives => 4014,
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
            Self::BindingUnresolvedName => 6001,
            Self::BindingAmbiguousName => 6002,
            Self::BindingInaccessibleName => 6003,
            Self::BindingWrongNameKind => 6004,
            Self::BindingMalformedName => 6005,
            Self::BindingNameAlreadyDefined => 6006,
            Self::BindingIncoherentAlternativePattern => 6007,
            Self::BindingInvalidCallableAbi => 6008,
            Self::BindingDuplicateCallableAbi => 6009,
            Self::BindingPredicateBodyRequired => 6010,
            Self::BindingTrustedPredicateBodyNotAllowed => 6011,
            Self::BindingCyclicModuleExport => 6012,
            Self::BindingConflictingModuleExport => 6013,
            Self::BindingInvalidModuleExportTarget => 6014,
            Self::BindingMalformedDirectiveArgument => 6015,
            Self::CheckingIncompatibleExpressionType => 7001,
            Self::CheckingCannotInferExpressionType => 7002,
            Self::CheckingInvalidConstantExpression => 7003,
            Self::CheckingArrayLengthNotPositive => 7013,
            Self::CheckingConstantLiteralNotRepresentable => 7004,
            Self::CheckingConstantEvaluationStepLimitExceeded => 7005,
            Self::CheckingConstantAggregateLimitExceeded => 7006,
            Self::CheckingConstantLiteralSizeLimitExceeded => 7007,
            Self::CheckingConstantIntegerSizeLimitExceeded => 7020,
            Self::CheckingCyclicConstantDefinition => 7008,
            Self::CheckingConstantDivisionByZero => 7018,
            Self::CheckingConstantValueNotRepresentable => 7019,
            Self::CheckingNoApplicableCandidate => 7009,
            Self::CheckingAmbiguousCandidate => 7010,
            Self::CheckingInaccessibleCandidate => 7011,
            Self::CheckingIncompatibleCandidate => 7012,
            Self::CheckingTargetRepresentationUnavailable => 7014,
            Self::CheckingTargetCallableAbiUnavailable => 7015,
            Self::CheckingTargetAlignmentUnsupported => 7016,
            Self::CheckingTargetAbiRepresentationUnsupported => 7017,
            Self::CheckingIncompatiblePattern => 7021,
            Self::CheckingRefutablePattern => 7022,
            Self::CheckingNonExhaustiveMatch => 7023,
            Self::CheckingUnreachableMatchArm => 7024,
            Self::CheckingUnreachablePatternAlternative => 7027,
            Self::CheckingDuplicateModuleContributionDirective => 7025,
            Self::CheckingArrayGeneratorCardinalityNotProvable => 7026,
            Self::CheckingInvalidStoredType => 7028,
            Self::CheckingRecursiveTypeRepresentation => 7029,
            Self::CheckingInvalidLayoutDirective => 7030,
            Self::CheckingInvalidCopyContract => 7031,
            Self::CheckingInvalidUnionTag => 7032,
            Self::CheckingRefinementCapacityExceeded => 7033,
            Self::CheckingUseOfUninitializedStorage => 7034,
            Self::CheckingUseOfMovedStorage => 7035,
            Self::CheckingConflictingBorrow => 7036,
            Self::CheckingMissingMutationAuthority => 7037,
            Self::CheckingTypeIsNotCopyable => 7038,
            Self::CheckingMissingStorageOwnership => 7039,
            Self::CheckingInactiveStorageProjection => 7044,
            Self::CheckingMissingTraitFulfillment => 7040,
            Self::CheckingExtraTraitFulfillment => 7041,
            Self::CheckingIncompatibleTraitFulfillment => 7042,
            Self::CheckingDuplicateTraitFulfillment => 7043,
            Self::CheckingOverlappingImplementation => 7045,
            Self::CheckingUngroupedImplementationOverloads => 7046,
            Self::CheckingInvalidImplementationOverloadHeader => 7047,
            Self::CheckingInvalidImplementationOverloadArm => 7048,
            Self::CheckingInvalidCallableOverloadArm => 7049,
            Self::CheckingDuplicateCallableOverloadArm => 7050,
            Self::CheckingConflictingCallableOverloadFamily => 7051,
            Self::CheckingConflictingCallableOverloadSignature => 7052,
            Self::CheckingMissingForeignCallableDirective => 7053,
            Self::CheckingForeignCallableRequiresTrusted => 7054,
            Self::CheckingForeignCallableRequiresCapability => 7055,
            Self::CheckingForeignCallableExecutionUnsupported => 7056,
            Self::CheckingForeignAbiTypeUnsupported => 7057,
            Self::CheckingInvalidNativeLinkDirective => 7058,
            Self::CheckingInvalidNativeSymbolDirective => 7059,
            Self::CheckingDuplicateNativeSymbol => 7060,
            Self::CheckingUnavailableNativeLinkInput => 7061,
            Self::CheckingUndeclaredTrustedCapability => 7062,
            Self::CheckingUnusedTrustedCapability => 7063,
            Self::CheckingTrustedCapabilityRequiresTrustedCallable => 7064,
            Self::CheckingAwaitOutsideAsyncCallable => 7065,
            Self::CheckingTaskStartOutsideAsyncCallable => 7066,
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
            Self::RequestDuplicateSourceInput => "request_duplicate_source_input",
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
            Self::DeclarationDuplicateModifier => "declaration_duplicate_modifier",
            Self::DeclarationIncompatibleModifiers => "declaration_incompatible_modifiers",
            Self::DeclarationBodyRequired => "declaration_body_required",
            Self::DeclarationBodyNotAllowed => "declaration_body_not_allowed",
            Self::DeclarationDuplicateDirective => "declaration_duplicate_directive",
            Self::DeclarationInvalidMemberPlacement => "declaration_invalid_member_placement",
            Self::DeclarationDuplicateLifecycleSlot => "declaration_duplicate_lifecycle_slot",
            Self::DeclarationInvalidModifier => "declaration_invalid_modifier",
            Self::DeclarationInvalidDirectiveTarget => "declaration_invalid_directive_target",
            Self::DeclarationInvalidParameterOrder => "declaration_invalid_parameter_order",
            Self::DeclarationIncompatibleDirectives => "declaration_incompatible_directives",
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
            Self::BindingUnresolvedName => "binding_unresolved_name",
            Self::BindingAmbiguousName => "binding_ambiguous_name",
            Self::BindingInaccessibleName => "binding_inaccessible_name",
            Self::BindingWrongNameKind => "binding_wrong_name_kind",
            Self::BindingMalformedName => "binding_malformed_name",
            Self::BindingNameAlreadyDefined => "binding_name_already_defined",
            Self::BindingIncoherentAlternativePattern => "binding_incoherent_alternative_pattern",
            Self::BindingInvalidCallableAbi => "binding_invalid_callable_abi",
            Self::BindingDuplicateCallableAbi => "binding_duplicate_callable_abi",
            Self::BindingPredicateBodyRequired => "binding_predicate_body_required",
            Self::BindingTrustedPredicateBodyNotAllowed => {
                "binding_trusted_predicate_body_not_allowed"
            }
            Self::BindingCyclicModuleExport => "binding_cyclic_module_export",
            Self::BindingConflictingModuleExport => "binding_conflicting_module_export",
            Self::BindingInvalidModuleExportTarget => "binding_invalid_module_export_target",
            Self::BindingMalformedDirectiveArgument => "binding_malformed_directive_argument",
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
            Self::CheckingConstantIntegerSizeLimitExceeded => {
                "checking_constant_integer_size_limit_exceeded"
            }
            Self::CheckingCyclicConstantDefinition => "checking_cyclic_constant_definition",
            Self::CheckingConstantDivisionByZero => "checking_constant_division_by_zero",
            Self::CheckingConstantValueNotRepresentable => {
                "checking_constant_value_not_representable"
            }
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
            Self::CheckingIncompatiblePattern => "checking_incompatible_pattern",
            Self::CheckingRefutablePattern => "checking_refutable_pattern",
            Self::CheckingNonExhaustiveMatch => "checking_non_exhaustive_match",
            Self::CheckingUnreachableMatchArm => "checking_unreachable_match_arm",
            Self::CheckingUnreachablePatternAlternative => {
                "checking_unreachable_pattern_alternative"
            }
            Self::CheckingDuplicateModuleContributionDirective => {
                "checking_duplicate_module_contribution_directive"
            }
            Self::CheckingArrayGeneratorCardinalityNotProvable => {
                "checking_array_generator_cardinality_not_provable"
            }
            Self::CheckingInvalidStoredType => "checking_invalid_stored_type",
            Self::CheckingRecursiveTypeRepresentation => "checking_recursive_type_representation",
            Self::CheckingInvalidLayoutDirective => "checking_invalid_layout_directive",
            Self::CheckingInvalidCopyContract => "checking_invalid_copy_contract",
            Self::CheckingInvalidUnionTag => "checking_invalid_union_tag",
            Self::CheckingRefinementCapacityExceeded => "checking_refinement_capacity_exceeded",
            Self::CheckingUseOfUninitializedStorage => "checking_use_of_uninitialized_storage",
            Self::CheckingUseOfMovedStorage => "checking_use_of_moved_storage",
            Self::CheckingConflictingBorrow => "checking_conflicting_borrow",
            Self::CheckingMissingMutationAuthority => "checking_missing_mutation_authority",
            Self::CheckingTypeIsNotCopyable => "checking_type_is_not_copyable",
            Self::CheckingMissingStorageOwnership => "checking_missing_storage_ownership",
            Self::CheckingInactiveStorageProjection => "checking_inactive_storage_projection",
            Self::CheckingMissingTraitFulfillment => "checking_missing_trait_fulfillment",
            Self::CheckingExtraTraitFulfillment => "checking_extra_trait_fulfillment",
            Self::CheckingIncompatibleTraitFulfillment => "checking_incompatible_trait_fulfillment",
            Self::CheckingDuplicateTraitFulfillment => "checking_duplicate_trait_fulfillment",
            Self::CheckingOverlappingImplementation => "checking_overlapping_implementation",
            Self::CheckingUngroupedImplementationOverloads => {
                "checking_ungrouped_implementation_overloads"
            }
            Self::CheckingInvalidImplementationOverloadHeader => {
                "checking_invalid_implementation_overload_header"
            }
            Self::CheckingInvalidImplementationOverloadArm => {
                "checking_invalid_implementation_overload_arm"
            }
            Self::CheckingInvalidCallableOverloadArm => "checking_invalid_callable_overload_arm",
            Self::CheckingDuplicateCallableOverloadArm => {
                "checking_duplicate_callable_overload_arm"
            }
            Self::CheckingConflictingCallableOverloadFamily => {
                "checking_conflicting_callable_overload_family"
            }
            Self::CheckingConflictingCallableOverloadSignature => {
                "checking_conflicting_callable_overload_signature"
            }
            Self::CheckingMissingForeignCallableDirective => {
                "checking_missing_foreign_callable_directive"
            }
            Self::CheckingForeignCallableRequiresTrusted => {
                "checking_foreign_callable_requires_trusted"
            }
            Self::CheckingForeignCallableRequiresCapability => {
                "checking_foreign_callable_requires_capability"
            }
            Self::CheckingForeignCallableExecutionUnsupported => {
                "checking_foreign_callable_execution_unsupported"
            }
            Self::CheckingForeignAbiTypeUnsupported => "checking_foreign_abi_type_unsupported",
            Self::CheckingInvalidNativeLinkDirective => "checking_invalid_native_link_directive",
            Self::CheckingInvalidNativeSymbolDirective => {
                "checking_invalid_native_symbol_directive"
            }
            Self::CheckingDuplicateNativeSymbol => "checking_duplicate_native_symbol",
            Self::CheckingUnavailableNativeLinkInput => "checking_unavailable_native_link_input",
            Self::CheckingUndeclaredTrustedCapability => "checking_undeclared_trusted_capability",
            Self::CheckingUnusedTrustedCapability => "checking_unused_trusted_capability",
            Self::CheckingTrustedCapabilityRequiresTrustedCallable => {
                "checking_trusted_capability_requires_trusted_callable"
            }
            Self::CheckingAwaitOutsideAsyncCallable => "checking_await_outside_async_callable",
            Self::CheckingTaskStartOutsideAsyncCallable => {
                "checking_task_start_outside_async_callable"
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

    fn all_diagnostic_kinds() -> [DiagnosticKind; 142] {
        [
            DiagnosticKind::SourceFileReadFailed,
            DiagnosticKind::SourceInvalidUtf8,
            DiagnosticKind::SourceTooManyInputs,
            DiagnosticKind::SourceTextTooLarge,
            DiagnosticKind::RequestMissingSourceInput,
            DiagnosticKind::RequestInvalidSourceInput,
            DiagnosticKind::RequestDuplicateSourceInput,
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
            DiagnosticKind::DeclarationDuplicateModifier,
            DiagnosticKind::DeclarationIncompatibleModifiers,
            DiagnosticKind::DeclarationInvalidModifier,
            DiagnosticKind::DeclarationBodyRequired,
            DiagnosticKind::DeclarationBodyNotAllowed,
            DiagnosticKind::DeclarationDuplicateDirective,
            DiagnosticKind::DeclarationIncompatibleDirectives,
            DiagnosticKind::DeclarationInvalidDirectiveTarget,
            DiagnosticKind::DeclarationInvalidParameterOrder,
            DiagnosticKind::DeclarationInvalidMemberPlacement,
            DiagnosticKind::DeclarationDuplicateLifecycleSlot,
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
            DiagnosticKind::BindingUnresolvedName,
            DiagnosticKind::BindingAmbiguousName,
            DiagnosticKind::BindingInaccessibleName,
            DiagnosticKind::BindingWrongNameKind,
            DiagnosticKind::BindingMalformedName,
            DiagnosticKind::BindingNameAlreadyDefined,
            DiagnosticKind::BindingIncoherentAlternativePattern,
            DiagnosticKind::BindingInvalidCallableAbi,
            DiagnosticKind::BindingDuplicateCallableAbi,
            DiagnosticKind::BindingPredicateBodyRequired,
            DiagnosticKind::BindingTrustedPredicateBodyNotAllowed,
            DiagnosticKind::BindingCyclicModuleExport,
            DiagnosticKind::BindingConflictingModuleExport,
            DiagnosticKind::BindingInvalidModuleExportTarget,
            DiagnosticKind::BindingMalformedDirectiveArgument,
            DiagnosticKind::CheckingIncompatibleExpressionType,
            DiagnosticKind::CheckingCannotInferExpressionType,
            DiagnosticKind::CheckingInvalidConstantExpression,
            DiagnosticKind::CheckingArrayLengthNotPositive,
            DiagnosticKind::CheckingConstantLiteralNotRepresentable,
            DiagnosticKind::CheckingConstantEvaluationStepLimitExceeded,
            DiagnosticKind::CheckingConstantAggregateLimitExceeded,
            DiagnosticKind::CheckingConstantLiteralSizeLimitExceeded,
            DiagnosticKind::CheckingConstantIntegerSizeLimitExceeded,
            DiagnosticKind::CheckingCyclicConstantDefinition,
            DiagnosticKind::CheckingConstantDivisionByZero,
            DiagnosticKind::CheckingConstantValueNotRepresentable,
            DiagnosticKind::CheckingNoApplicableCandidate,
            DiagnosticKind::CheckingAmbiguousCandidate,
            DiagnosticKind::CheckingInaccessibleCandidate,
            DiagnosticKind::CheckingIncompatibleCandidate,
            DiagnosticKind::CheckingTargetRepresentationUnavailable,
            DiagnosticKind::CheckingTargetCallableAbiUnavailable,
            DiagnosticKind::CheckingTargetAlignmentUnsupported,
            DiagnosticKind::CheckingTargetAbiRepresentationUnsupported,
            DiagnosticKind::CheckingIncompatiblePattern,
            DiagnosticKind::CheckingRefutablePattern,
            DiagnosticKind::CheckingNonExhaustiveMatch,
            DiagnosticKind::CheckingUnreachableMatchArm,
            DiagnosticKind::CheckingUnreachablePatternAlternative,
            DiagnosticKind::CheckingDuplicateModuleContributionDirective,
            DiagnosticKind::CheckingArrayGeneratorCardinalityNotProvable,
            DiagnosticKind::CheckingInvalidStoredType,
            DiagnosticKind::CheckingRecursiveTypeRepresentation,
            DiagnosticKind::CheckingInvalidLayoutDirective,
            DiagnosticKind::CheckingInvalidCopyContract,
            DiagnosticKind::CheckingInvalidUnionTag,
            DiagnosticKind::CheckingRefinementCapacityExceeded,
            DiagnosticKind::CheckingUseOfUninitializedStorage,
            DiagnosticKind::CheckingUseOfMovedStorage,
            DiagnosticKind::CheckingConflictingBorrow,
            DiagnosticKind::CheckingMissingMutationAuthority,
            DiagnosticKind::CheckingTypeIsNotCopyable,
            DiagnosticKind::CheckingMissingStorageOwnership,
            DiagnosticKind::CheckingInactiveStorageProjection,
            DiagnosticKind::CheckingMissingTraitFulfillment,
            DiagnosticKind::CheckingExtraTraitFulfillment,
            DiagnosticKind::CheckingIncompatibleTraitFulfillment,
            DiagnosticKind::CheckingDuplicateTraitFulfillment,
            DiagnosticKind::CheckingOverlappingImplementation,
            DiagnosticKind::CheckingUngroupedImplementationOverloads,
            DiagnosticKind::CheckingInvalidImplementationOverloadHeader,
            DiagnosticKind::CheckingInvalidImplementationOverloadArm,
            DiagnosticKind::CheckingInvalidCallableOverloadArm,
            DiagnosticKind::CheckingDuplicateCallableOverloadArm,
            DiagnosticKind::CheckingConflictingCallableOverloadFamily,
            DiagnosticKind::CheckingConflictingCallableOverloadSignature,
            DiagnosticKind::CheckingMissingForeignCallableDirective,
            DiagnosticKind::CheckingForeignCallableRequiresTrusted,
            DiagnosticKind::CheckingForeignCallableRequiresCapability,
            DiagnosticKind::CheckingForeignCallableExecutionUnsupported,
            DiagnosticKind::CheckingForeignAbiTypeUnsupported,
            DiagnosticKind::CheckingInvalidNativeLinkDirective,
            DiagnosticKind::CheckingInvalidNativeSymbolDirective,
            DiagnosticKind::CheckingDuplicateNativeSymbol,
            DiagnosticKind::CheckingUnavailableNativeLinkInput,
            DiagnosticKind::CheckingUndeclaredTrustedCapability,
            DiagnosticKind::CheckingUnusedTrustedCapability,
            DiagnosticKind::CheckingTrustedCapabilityRequiresTrustedCallable,
            DiagnosticKind::CheckingAwaitOutsideAsyncCallable,
            DiagnosticKind::CheckingTaskStartOutsideAsyncCallable,
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
