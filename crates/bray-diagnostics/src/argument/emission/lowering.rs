use bray_source::SourceSpan;

/// A lowering-input contract failure and the affected Bray source construct.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticLoweringInputFailure {
    kind: DiagnosticLoweringInputFailureKind,
    source: SourceSpan,
}

impl DiagnosticLoweringInputFailure {
    pub const fn new(kind: DiagnosticLoweringInputFailureKind, source: SourceSpan) -> Self {
        Self { kind, source }
    }

    /// Returns the exact failure category.
    pub const fn kind(self) -> DiagnosticLoweringInputFailureKind {
        self.kind
    }

    /// Returns the closest Bray source construct affected by the failure.
    pub const fn source(self) -> SourceSpan {
        self.source
    }

    /// Returns the stable machine key for this contract failure.
    pub const fn as_str(self) -> &'static str {
        self.kind.as_str()
    }
}

/// Exact lowering-input contract failure retained at a diagnostic boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLoweringInputFailureKind {
    ForeignInput,
    InputKindMismatch,
    MissingSemanticSelection,
    MissingExpressionType,
    InvalidPatternInput,
    InvalidInputContents,
    SemanticValue(crate::DiagnosticSemanticValueFailure),
    InvalidStorageOperation,
    StorageOperationCountMismatch { expected: u64, actual: u64 },
    InvalidStorageExit,
    LiteralTargetWidthMismatch { expected: u16, actual: u16 },
    ExecutableHostRequiresSyntheticInput,
    CompileTimeUnitRequiresClassification,
}

impl DiagnosticLoweringInputFailureKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ForeignInput => "code_production_input_foreign_declaration",
            Self::InputKindMismatch => "code_production_input_declaration_kind_mismatch",
            Self::MissingSemanticSelection => "code_production_input_missing_behavior",
            Self::MissingExpressionType => "code_production_input_missing_expression_type",
            Self::InvalidPatternInput => "code_production_input_invalid_pattern",
            Self::InvalidInputContents => "code_production_input_foreign_source_construct",
            Self::SemanticValue(failure) => failure.as_str(),
            Self::InvalidStorageOperation => "code_production_input_invalid_value_access",
            Self::StorageOperationCountMismatch { .. } => {
                "code_production_input_value_access_count_mismatch"
            }
            Self::InvalidStorageExit => "code_production_input_invalid_scope_cleanup",
            Self::LiteralTargetWidthMismatch { .. } => {
                "code_production_input_integer_target_width_mismatch"
            }
            Self::ExecutableHostRequiresSyntheticInput => {
                "code_production_input_program_startup_source_mismatch"
            }
            Self::CompileTimeUnitRequiresClassification => {
                "code_production_input_declaration_classification_missing"
            }
        }
    }
}

/// A compiler code-production failure and the affected Bray source construct.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticLoweringFailure {
    kind: DiagnosticLoweringFailureKind,
    source: SourceSpan,
}

impl DiagnosticLoweringFailure {
    pub const fn new(kind: DiagnosticLoweringFailureKind, source: SourceSpan) -> Self {
        Self { kind, source }
    }

    /// Returns the exact failure category.
    pub const fn kind(self) -> DiagnosticLoweringFailureKind {
        self.kind
    }

    /// Returns the closest Bray source construct affected by the failure.
    pub const fn source(self) -> SourceSpan {
        self.source
    }

    /// Returns the stable machine key for this lowering failure.
    pub const fn as_str(self) -> &'static str {
        self.kind.as_str()
    }
}

/// Exact compiler code-production failure retained at a diagnostic boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLoweringFailureKind {
    UnsupportedRoot,
    MissingSourceNode(DiagnosticSourceConstructKind),
    RecoveredSourceNode(DiagnosticSourceConstructKind),
    MissingExpressionType,
    AwaitOutsideProtectedFrame,
    MissingSuspensionPoint,
    InvalidTaskOperation,
    MissingCallableResultType,
    MissingLiteralValue,
    MissingSemanticSelection,
    UnsupportedExpression,
    UnsupportedPattern,
    UnsupportedOperator,
    MissingStorageAccess,
    MissingStorageAccessRecord,
    MissingCleanupPlan,
    MissingStorageIdentity,
    MissingStorageIdentityRecord,
    MissingIterationStorage,
    UnsupportedStorageAccess,
    MissingOperationResult,
    MissingRepresentation,
    SemanticValueUnavailable,
    SemanticValue(crate::DiagnosticSemanticValueFailure),
    InvalidFrameDescriptor,
    Mir(DiagnosticMirUnitBuildFailure),
}

impl DiagnosticLoweringFailureKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedRoot => "code_production_declaration_without_executable_body",
            Self::MissingSourceNode(kind) => kind.missing_key(),
            Self::RecoveredSourceNode(kind) => kind.recovered_key(),
            Self::MissingExpressionType => "code_production_expression_type_unavailable",
            Self::AwaitOutsideProtectedFrame => "code_production_await_state_unavailable",
            Self::MissingSuspensionPoint => "code_production_await_resume_path_unavailable",
            Self::InvalidTaskOperation => "code_production_task_call_type_mismatch",
            Self::MissingCallableResultType => "code_production_callable_result_type_unavailable",
            Self::MissingLiteralValue => "code_production_literal_value_unavailable",
            Self::MissingSemanticSelection => "code_production_expression_behavior_unavailable",
            Self::UnsupportedExpression => "code_production_expression_unsupported",
            Self::UnsupportedPattern => "code_production_pattern_unsupported",
            Self::UnsupportedOperator => "code_production_operator_unsupported",
            Self::MissingStorageAccess => "code_production_value_access_unavailable",
            Self::MissingStorageAccessRecord => "code_production_value_access_record_unavailable",
            Self::MissingCleanupPlan => "code_production_scope_cleanup_unavailable",
            Self::MissingStorageIdentity => "code_production_accessed_value_unavailable",
            Self::MissingStorageIdentityRecord => "code_production_value_record_unavailable",
            Self::MissingIterationStorage => "code_production_iteration_state_unavailable",
            Self::UnsupportedStorageAccess => "code_production_value_access_unsupported",
            Self::MissingOperationResult => "code_production_expression_value_unavailable",
            Self::MissingRepresentation => "code_production_target_representation_unavailable",
            Self::SemanticValueUnavailable => "code_production_type_or_constant_unavailable",
            Self::SemanticValue(failure) => failure.as_str(),
            Self::InvalidFrameDescriptor => "code_production_resumable_state_conflict",
            Self::Mir(failure) => failure.as_str(),
        }
    }
}

/// The Bray syntax category associated with a compiler-owned source-node failure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSourceConstructKind {
    Expression,
    Pattern,
    Block,
    CallableBody,
}

impl DiagnosticSourceConstructKind {
    const fn missing_key(self) -> &'static str {
        match self {
            Self::Expression => "code_production_expression_unavailable",
            Self::Pattern => "code_production_pattern_unavailable",
            Self::Block => "code_production_block_unavailable",
            Self::CallableBody => "code_production_callable_body_unavailable",
        }
    }

    const fn recovered_key(self) -> &'static str {
        match self {
            Self::Expression => "code_production_expression_has_prior_error",
            Self::Pattern => "code_production_pattern_has_prior_error",
            Self::Block => "code_production_block_has_prior_error",
            Self::CallableBody => "code_production_callable_body_has_prior_error",
        }
    }
}

/// Exact executable-code construction failure retained through lowering diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticMirUnitBuildFailure {
    SourceOriginMismatch,
    IdentityCapacityExceeded,
    ForeignBlock,
    ForeignOperation,
    ForeignStorage,
    ForeignValue,
    MissingBlock,
    MissingOperation,
    MissingOperationResult,
    UnexpectedOperationResult,
    OperationResultTypeMismatch,
    InvalidAggregateOperation,
    InvalidMemoryOperation,
    InvalidAnonymousCallable,
    InvalidConstructionInput,
    InvalidCall,
    InvalidHostOperation,
    InvalidHostSequence,
    MissingStorage,
    MissingValue,
    DuplicateTerminator,
    MissingTerminator,
    InvalidInlineAssemblyTerminator,
    InvalidSuspensionPayload,
    InvalidCallPanicCheck,
    EdgeArgumentCountMismatch,
    EdgeArgumentTypeMismatch,
    DuplicateSwitchCase,
    CleanupTargetMismatch,
    CleanupPhaseOrderViolation,
    RuntimeRoleMismatch,
    RuntimeAbiVersionMismatch,
    InvalidOperationBlock,
    StorageKindMismatch,
    StorageTypeMismatch,
    ValueDoesNotDominateUse,
    ProtectedFrameMismatch,
    MissingFrameDescriptor,
    DuplicateFrameDescriptor,
    UnexpectedFrameDescriptor,
    InvalidFrameStateEntry,
    MissingFrameState,
}

impl DiagnosticMirUnitBuildFailure {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceOriginMismatch => "executable_code_source_declaration_mismatch",
            Self::IdentityCapacityExceeded => "executable_code_capacity_exceeded",
            Self::ForeignBlock => "executable_code_foreign_path",
            Self::ForeignOperation => "executable_code_foreign_instruction",
            Self::ForeignStorage => "executable_code_foreign_memory",
            Self::ForeignValue => "executable_code_foreign_value",
            Self::MissingBlock => "executable_code_missing_path",
            Self::MissingOperation => "executable_code_missing_instruction",
            Self::MissingOperationResult => "executable_code_missing_instruction_value",
            Self::UnexpectedOperationResult => "executable_code_unexpected_value",
            Self::OperationResultTypeMismatch => "executable_code_value_type_mismatch",
            Self::InvalidAggregateOperation => "executable_code_aggregate_value_mismatch",
            Self::InvalidMemoryOperation => "executable_code_value_access_mismatch",
            Self::InvalidAnonymousCallable => "executable_code_invalid_anonymous_callable",
            Self::InvalidConstructionInput => "executable_code_construction_value_mismatch",
            Self::InvalidCall => "executable_code_invalid_call",
            Self::InvalidHostOperation => "executable_code_program_lifecycle_mismatch",
            Self::InvalidHostSequence => "executable_code_program_shutdown_order",
            Self::MissingStorage => "executable_code_missing_memory",
            Self::MissingValue => "executable_code_missing_value",
            Self::DuplicateTerminator => "executable_code_multiple_path_outcomes",
            Self::MissingTerminator => "executable_code_missing_path_outcome",
            Self::InvalidInlineAssemblyTerminator => {
                "executable_code_inline_assembly_branch_mismatch"
            }
            Self::InvalidSuspensionPayload => "executable_code_suspension_value_mismatch",
            Self::InvalidCallPanicCheck => "executable_code_call_panic_handling_mismatch",
            Self::EdgeArgumentCountMismatch => "executable_code_path_value_count_mismatch",
            Self::EdgeArgumentTypeMismatch => "executable_code_path_value_type_mismatch",
            Self::DuplicateSwitchCase => "executable_code_duplicate_branch_case",
            Self::CleanupTargetMismatch => "executable_code_cleanup_stage_mismatch",
            Self::CleanupPhaseOrderViolation => "executable_code_cleanup_order",
            Self::RuntimeRoleMismatch => "executable_code_runtime_service_mismatch",
            Self::RuntimeAbiVersionMismatch => "executable_code_runtime_interface_mismatch",
            Self::InvalidOperationBlock => "executable_code_instruction_path_mismatch",
            Self::StorageKindMismatch => "executable_code_ownership_role_mismatch",
            Self::StorageTypeMismatch => "executable_code_value_access_type_mismatch",
            Self::ValueDoesNotDominateUse => "executable_code_value_used_before_production",
            Self::ProtectedFrameMismatch => "executable_code_async_callable_state_mismatch",
            Self::MissingFrameDescriptor => "executable_code_missing_resumable_state_layout",
            Self::DuplicateFrameDescriptor => "executable_code_conflicting_resumable_state_layouts",
            Self::UnexpectedFrameDescriptor => "executable_code_unexpected_resumable_state_layout",
            Self::InvalidFrameStateEntry => "executable_code_invalid_resume_point",
            Self::MissingFrameState => "executable_code_missing_resume_point",
        }
    }
}
