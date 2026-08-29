/// Exact lowering-input contract failure retained at a diagnostic boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLoweringInputFailure {
    ForeignInput,
    InputKindMismatch,
    MissingSemanticSelection,
    MissingExpressionType,
    InvalidPatternInput,
    InvalidInputContents,
    InvalidStorageOperation,
    StorageOperationCountMismatch,
    InvalidStorageExit,
    LiteralTargetWidthMismatch,
    ExecutableHostRequiresSyntheticInput,
    CompileTimeUnitRequiresClassification,
}

impl DiagnosticLoweringInputFailure {
    /// Returns the stable machine key for this contract failure.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ForeignInput => "lowering_input_foreign_input",
            Self::InputKindMismatch => "lowering_input_kind_mismatch",
            Self::MissingSemanticSelection => "lowering_input_missing_semantic_selection",
            Self::MissingExpressionType => "lowering_input_missing_expression_type",
            Self::InvalidPatternInput => "lowering_input_invalid_pattern_input",
            Self::InvalidInputContents => "lowering_input_invalid_contents",
            Self::InvalidStorageOperation => "lowering_input_invalid_storage_operation",
            Self::StorageOperationCountMismatch => {
                "lowering_input_storage_operation_count_mismatch"
            }
            Self::InvalidStorageExit => "lowering_input_invalid_storage_exit",
            Self::LiteralTargetWidthMismatch => "lowering_input_literal_target_width_mismatch",
            Self::ExecutableHostRequiresSyntheticInput => {
                "lowering_input_executable_host_requires_synthetic_input"
            }
            Self::CompileTimeUnitRequiresClassification => {
                "lowering_input_compile_time_unit_requires_classification"
            }
        }
    }
}

/// Exact checked-HIR or MIR construction failure retained at a diagnostic boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLoweringFailure {
    UnsupportedRoot,
    MissingBoundNode,
    RecoveredBoundNode,
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
    InvalidFrameDescriptor,
    Mir(DiagnosticMirUnitBuildFailure),
}

impl DiagnosticLoweringFailure {
    /// Returns the stable machine key for this lowering failure.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedRoot => "lowering_unsupported_root",
            Self::MissingBoundNode => "lowering_missing_bound_node",
            Self::RecoveredBoundNode => "lowering_recovered_bound_node",
            Self::MissingExpressionType => "lowering_missing_expression_type",
            Self::AwaitOutsideProtectedFrame => "lowering_await_outside_protected_frame",
            Self::MissingSuspensionPoint => "lowering_missing_suspension_point",
            Self::InvalidTaskOperation => "lowering_invalid_task_operation",
            Self::MissingCallableResultType => "lowering_missing_callable_result_type",
            Self::MissingLiteralValue => "lowering_missing_literal_value",
            Self::MissingSemanticSelection => "lowering_missing_semantic_selection",
            Self::UnsupportedExpression => "lowering_unsupported_expression",
            Self::UnsupportedPattern => "lowering_unsupported_pattern",
            Self::UnsupportedOperator => "lowering_unsupported_operator",
            Self::MissingStorageAccess => "lowering_missing_storage_access",
            Self::MissingStorageAccessRecord => "lowering_missing_storage_access_record",
            Self::MissingCleanupPlan => "lowering_missing_cleanup_plan",
            Self::MissingStorageIdentity => "lowering_missing_storage_identity",
            Self::MissingStorageIdentityRecord => "lowering_missing_storage_identity_record",
            Self::MissingIterationStorage => "lowering_missing_iteration_storage",
            Self::UnsupportedStorageAccess => "lowering_unsupported_storage_access",
            Self::MissingOperationResult => "lowering_missing_operation_result",
            Self::MissingRepresentation => "lowering_missing_representation",
            Self::SemanticValueUnavailable => "lowering_semantic_value_unavailable",
            Self::InvalidFrameDescriptor => "lowering_invalid_frame_descriptor",
            Self::Mir(failure) => failure.as_str(),
        }
    }
}

/// Exact MIR unit construction failure retained through lowering diagnostics.
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
    /// Returns the stable machine key for this MIR construction failure.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceOriginMismatch => "mir_source_origin_mismatch",
            Self::IdentityCapacityExceeded => "mir_identity_capacity_exceeded",
            Self::ForeignBlock => "mir_foreign_block",
            Self::ForeignOperation => "mir_foreign_operation",
            Self::ForeignStorage => "mir_foreign_storage",
            Self::ForeignValue => "mir_foreign_value",
            Self::MissingBlock => "mir_missing_block",
            Self::MissingOperation => "mir_missing_operation",
            Self::MissingOperationResult => "mir_missing_operation_result",
            Self::UnexpectedOperationResult => "mir_unexpected_operation_result",
            Self::OperationResultTypeMismatch => "mir_operation_result_type_mismatch",
            Self::InvalidAggregateOperation => "mir_invalid_aggregate_operation",
            Self::InvalidMemoryOperation => "mir_invalid_memory_operation",
            Self::InvalidAnonymousCallable => "mir_invalid_anonymous_callable",
            Self::InvalidConstructionInput => "mir_invalid_construction_input",
            Self::InvalidCall => "mir_invalid_call",
            Self::InvalidHostOperation => "mir_invalid_host_operation",
            Self::InvalidHostSequence => "mir_invalid_host_sequence",
            Self::MissingStorage => "mir_missing_storage",
            Self::MissingValue => "mir_missing_value",
            Self::DuplicateTerminator => "mir_duplicate_terminator",
            Self::MissingTerminator => "mir_missing_terminator",
            Self::InvalidInlineAssemblyTerminator => "mir_invalid_inline_assembly_terminator",
            Self::InvalidSuspensionPayload => "mir_invalid_suspension_payload",
            Self::InvalidCallPanicCheck => "mir_invalid_call_panic_check",
            Self::EdgeArgumentCountMismatch => "mir_edge_argument_count_mismatch",
            Self::EdgeArgumentTypeMismatch => "mir_edge_argument_type_mismatch",
            Self::DuplicateSwitchCase => "mir_duplicate_switch_case",
            Self::CleanupTargetMismatch => "mir_cleanup_target_mismatch",
            Self::CleanupPhaseOrderViolation => "mir_cleanup_phase_order_violation",
            Self::RuntimeRoleMismatch => "mir_runtime_role_mismatch",
            Self::RuntimeAbiVersionMismatch => "mir_runtime_abi_version_mismatch",
            Self::InvalidOperationBlock => "mir_invalid_operation_block",
            Self::StorageKindMismatch => "mir_storage_kind_mismatch",
            Self::StorageTypeMismatch => "mir_storage_type_mismatch",
            Self::ValueDoesNotDominateUse => "mir_value_does_not_dominate_use",
            Self::ProtectedFrameMismatch => "mir_protected_frame_mismatch",
            Self::MissingFrameDescriptor => "mir_missing_frame_descriptor",
            Self::DuplicateFrameDescriptor => "mir_duplicate_frame_descriptor",
            Self::UnexpectedFrameDescriptor => "mir_unexpected_frame_descriptor",
            Self::InvalidFrameStateEntry => "mir_invalid_frame_state_entry",
            Self::MissingFrameState => "mir_missing_frame_state",
        }
    }
}
