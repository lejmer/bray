use bray_source::SourceSpan;

/// A lowering-input contract failure and the affected Bray source construct.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticLoweringInputFailure {
    kind: DiagnosticLoweringInputFailureKind,
    source: SourceSpan,
}

impl DiagnosticLoweringInputFailure {
    /// Creates a lowering-input failure at its closest source construct.
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
    /// An analysis input belongs to another bound unit.
    ForeignInput {
        /// The analysis input category.
        input: &'static str,
        /// The bound unit required by lowering.
        expected_unit: u32,
        /// The bound unit that owns the supplied input.
        actual_unit: u32,
    },
    /// An analysis input describes the wrong bound-unit category.
    InputKindMismatch {
        /// The analysis input category.
        input: &'static str,
        /// The bound-unit category required by lowering.
        expected_kind: &'static str,
        /// The bound-unit category described by the supplied input.
        actual_kind: &'static str,
    },
    /// An expression has no selected semantic behavior.
    MissingSemanticSelection(DiagnosticLoweringIdentity),
    /// An expression has no checked type.
    MissingExpressionType(DiagnosticLoweringIdentity),
    /// Pattern analysis rejected the lowering input.
    InvalidPatternInput,
    /// An analysis input refers to an unsupported source construct.
    InvalidInputContents(&'static str),
    /// A semantic value could not cross the lowering boundary.
    SemanticValue(crate::DiagnosticSemanticValueFailure),
    /// An expression has an invalid storage operation.
    InvalidStorageOperation(DiagnosticLoweringIdentity),
    /// The storage plan contains the wrong number of operations.
    StorageOperationCountMismatch { expected: u64, actual: u64 },
    /// A block has an invalid storage exit.
    InvalidStorageExit(DiagnosticLoweringIdentity),
    /// Checked semantic plans cannot form one complete lowering-ready plan set.
    InvalidPlan {
        /// The failed semantic plan category.
        plan: &'static str,
        /// The exact verification failure.
        cause: &'static str,
        /// The affected expression, when available.
        expression: Option<DiagnosticLoweringIdentity>,
        /// The affected lexical scope, when available.
        scope: Option<DiagnosticLoweringIdentity>,
        /// The affected exit occurrence, when available.
        exit: Option<DiagnosticLoweringIdentity>,
        /// The affected storage identity, when available.
        storage: Option<DiagnosticLoweringIdentity>,
        /// The affected storage access, when available.
        access: Option<DiagnosticLoweringIdentity>,
    },
    /// An integer literal was checked for a different target width.
    LiteralTargetWidthMismatch { expected: u16, actual: u16 },
    /// Executable-host lowering received a source-owned input.
    ExecutableHostRequiresSyntheticInput,
    /// Compile-time lowering received an unclassified unit.
    CompileTimeUnitRequiresClassification,
}

impl DiagnosticLoweringInputFailureKind {
    /// Returns the stable machine key for this lowering-input failure.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ForeignInput { .. } => "code_production_input_foreign_declaration",
            Self::InputKindMismatch { .. } => "code_production_input_declaration_kind_mismatch",
            Self::MissingSemanticSelection(_) => "code_production_input_missing_behavior",
            Self::MissingExpressionType(_) => "code_production_input_missing_expression_type",
            Self::InvalidPatternInput => "code_production_input_invalid_pattern",
            Self::InvalidInputContents(_) => "code_production_input_foreign_source_construct",
            Self::SemanticValue(failure) => failure.as_str(),
            Self::InvalidStorageOperation(_) => "code_production_input_invalid_value_access",
            Self::StorageOperationCountMismatch { .. } => {
                "code_production_input_value_access_count_mismatch"
            }
            Self::InvalidStorageExit(_) => "code_production_input_invalid_scope_cleanup",
            Self::InvalidPlan { .. } => "code_production_input_invalid_verified_plan",
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
    /// Creates a lowering failure at its closest source construct.
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
    /// The checked root has no supported executable lowering form.
    UnsupportedRoot(DiagnosticLoweringRoot),
    /// A required checked source node is absent.
    MissingSourceNode {
        /// The required source construct category.
        kind: DiagnosticSourceConstructKind,
        /// The missing node identity.
        identity: DiagnosticLoweringIdentity,
    },
    /// A required checked source node contains a recovered earlier error.
    RecoveredSourceNode {
        /// The recovered source construct category.
        kind: DiagnosticSourceConstructKind,
        /// The recovered node identity.
        identity: DiagnosticLoweringIdentity,
    },
    /// An expression has no checked type.
    MissingExpressionType(DiagnosticLoweringIdentity),
    /// An await expression occurs outside a protected frame.
    AwaitOutsideProtectedFrame(DiagnosticLoweringIdentity),
    /// An await expression has no selected suspension point.
    MissingSuspensionPoint(DiagnosticLoweringIdentity),
    /// A task operation violates its checked operation contract.
    InvalidTaskOperation(DiagnosticLoweringIdentity),
    /// The lowered callable has no result type.
    MissingCallableResultType,
    /// A cleanup storage has no checked execution mode.
    MissingCleanupExecution(DiagnosticMirUnitLocalIdentity),
    /// Active lexical scopes do not contain the requested cleanup depth.
    InvalidCleanupScopeDepth {
        /// First active scope position that must be cleaned.
        scope_depth: usize,
        /// Number of lexical scopes active at the exit.
        active_scope_count: usize,
        /// The source occurrence initiating cleanup.
        exit: DiagnosticLoweringIdentity,
    },
    /// A verified lifecycle plan has no decision for one scope and exit.
    MissingScopeExitPlan {
        /// The lexical scope whose decision is absent.
        scope: DiagnosticLoweringIdentity,
        /// The source occurrence initiating cleanup.
        exit: DiagnosticLoweringIdentity,
    },
    /// A literal expression has no checked value.
    MissingLiteralValue(DiagnosticLoweringIdentity),
    /// An expression has no selected semantic behavior.
    MissingSemanticSelection(DiagnosticLoweringIdentity),
    /// An expression has no supported lowering form.
    UnsupportedExpression(DiagnosticLoweringIdentity),
    /// A pattern has no supported lowering form.
    UnsupportedPattern(DiagnosticLoweringIdentity),
    /// An operator has no supported lowering form.
    UnsupportedOperator {
        /// The expression containing the operator.
        expression: DiagnosticLoweringIdentity,
        /// The stable operator spelling.
        operator: &'static str,
    },
    /// An expression has no selected storage access.
    MissingStorageAccess(DiagnosticLoweringIdentity),
    /// A selected storage access has no checked record.
    MissingStorageAccessRecord(DiagnosticLoweringIdentity),
    /// A storage access has no selected storage identity.
    MissingStorageIdentity(DiagnosticLoweringIdentity),
    /// A selected storage identity has no checked record.
    MissingStorageIdentityRecord(DiagnosticLoweringIdentity),
    /// An iteration expression has no selected storage.
    MissingIterationStorage(DiagnosticLoweringIdentity),
    /// A selected storage access has no supported lowering form.
    UnsupportedStorageAccess(DiagnosticLoweringIdentity),
    /// An expression operation did not produce its required result.
    MissingOperationResult(DiagnosticLoweringIdentity),
    /// A compiler-known representation role is unavailable.
    MissingRepresentation(&'static str),
    /// A required type or constant value is unavailable.
    SemanticValueUnavailable,
    /// Generic substitution construction rejected one exact relationship.
    GenericSubstitution(crate::DiagnosticGenericSubstitutionFailure),
    /// A semantic value could not cross the lowering boundary.
    SemanticValue(crate::DiagnosticSemanticValueFailure),
    /// Protected-frame metadata violates the lowering contract.
    InvalidFrameDescriptor(DiagnosticFrameDescriptorFailure),
    /// A selected memory argument ordinal cannot index the host collection.
    MemoryArgumentOrdinalUnrepresentable {
        expression: DiagnosticLoweringIdentity,
        ordinal: u64,
    },
    /// A match-arm ordinal cannot be represented by the MIR protocol.
    MatchArmOrdinalUnrepresentable {
        expression: DiagnosticLoweringIdentity,
        ordinal: usize,
    },
    /// MIR construction rejected the lowered unit.
    Mir(DiagnosticMirUnitBuildFailure),
}

impl DiagnosticLoweringFailureKind {
    /// Returns the stable machine key for this lowering failure.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedRoot(_) => "code_production_declaration_without_executable_body",
            Self::MissingSourceNode { kind, .. } => kind.missing_key(),
            Self::RecoveredSourceNode { kind, .. } => kind.recovered_key(),
            Self::MissingExpressionType(_) => "code_production_expression_type_unavailable",
            Self::AwaitOutsideProtectedFrame(_) => "code_production_await_state_unavailable",
            Self::MissingSuspensionPoint(_) => "code_production_await_resume_path_unavailable",
            Self::InvalidTaskOperation(_) => "code_production_task_call_type_mismatch",
            Self::MissingCallableResultType => "code_production_callable_result_type_unavailable",
            Self::MissingCleanupExecution(_) => "code_production_cleanup_execution_unavailable",
            Self::InvalidCleanupScopeDepth { .. } => "code_production_cleanup_scope_depth_invalid",
            Self::MissingScopeExitPlan { .. } => "code_production_scope_exit_plan_unavailable",
            Self::MissingLiteralValue(_) => "code_production_literal_value_unavailable",
            Self::MissingSemanticSelection(_) => "code_production_expression_behavior_unavailable",
            Self::UnsupportedExpression(_) => "code_production_expression_unsupported",
            Self::UnsupportedPattern(_) => "code_production_pattern_unsupported",
            Self::UnsupportedOperator { .. } => "code_production_operator_unsupported",
            Self::MissingStorageAccess(_) => "code_production_value_access_unavailable",
            Self::MissingStorageAccessRecord(_) => {
                "code_production_value_access_record_unavailable"
            }
            Self::MissingStorageIdentity(_) => "code_production_accessed_value_unavailable",
            Self::MissingStorageIdentityRecord(_) => "code_production_value_record_unavailable",
            Self::MissingIterationStorage(_) => "code_production_iteration_state_unavailable",
            Self::UnsupportedStorageAccess(_) => "code_production_value_access_unsupported",
            Self::MissingOperationResult(_) => "code_production_expression_value_unavailable",
            Self::MissingRepresentation(_) => "code_production_target_representation_unavailable",
            Self::SemanticValueUnavailable => "code_production_type_or_constant_unavailable",
            Self::GenericSubstitution(_) => "code_production_generic_substitution_invalid",
            Self::SemanticValue(failure) => failure.as_str(),
            Self::InvalidFrameDescriptor(_) => "code_production_resumable_state_conflict",
            Self::MemoryArgumentOrdinalUnrepresentable { .. } => {
                "code_production_memory_argument_ordinal_unrepresentable"
            }
            Self::MatchArmOrdinalUnrepresentable { .. } => {
                "code_production_match_arm_ordinal_unrepresentable"
            }
            Self::Mir(failure) => failure.as_str(),
        }
    }
}

/// Exact protected-frame descriptor failure retained across compiler boundaries.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticFrameDescriptorFailure {
    MissingState,
    NonContiguousState,
    DuplicateStateOrEntry,
    IdentityCapacityExceeded,
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

/// A compilation-unit-local identity retained by a lowering failure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticLoweringIdentity {
    unit: u32,
    ordinal: u32,
}

impl DiagnosticLoweringIdentity {
    /// Creates an identity from its compilation-local unit and ordinal.
    pub const fn new(unit: u32, ordinal: u32) -> Self {
        Self { unit, ordinal }
    }

    /// Returns the compilation-local unit identity.
    pub const fn unit(self) -> u32 {
        self.unit
    }

    /// Returns the unit-local ordinal.
    pub const fn ordinal(self) -> u32 {
        self.ordinal
    }
}

/// The exact checked-HIR root rejected by synchronous lowering.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLoweringRoot {
    /// A declared callable body and its execution mode.
    CallableBody {
        /// The callable execution mode.
        execution: &'static str,
        /// The bound callable-body identity.
        body: DiagnosticLoweringIdentity,
    },
    /// A local anonymous callable and its body.
    AnonymousCallable {
        /// The local-symbol region containing the anonymous callable.
        callable_region: u32,
        /// The local-symbol ordinal of the anonymous callable.
        callable_ordinal: u32,
        /// The callable execution mode.
        execution: &'static str,
        /// The bound callable-body identity.
        body: DiagnosticLoweringIdentity,
    },
    /// A declaration-owned expression root.
    Expression(DiagnosticLoweringIdentity),
    /// A declaration-owned expression-sequence root.
    ExpressionSequence(DiagnosticLoweringIdentity),
}

/// Exact executable-code construction failure retained through lowering diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticMirUnitBuildFailure {
    kind: DiagnosticMirUnitBuildFailureKind,
    context: DiagnosticMirUnitBuildFailureContext,
}

impl DiagnosticMirUnitBuildFailure {
    /// Creates one MIR construction failure from its leaf category and exact typed context.
    pub const fn new(
        kind: DiagnosticMirUnitBuildFailureKind,
        context: DiagnosticMirUnitBuildFailureContext,
    ) -> Self {
        Self { kind, context }
    }

    /// Returns the exact failure category.
    pub const fn kind(self) -> DiagnosticMirUnitBuildFailureKind {
        self.kind
    }

    /// Returns the exact identities or contract values retained by the failure.
    pub const fn context(self) -> DiagnosticMirUnitBuildFailureContext {
        self.context
    }

    /// Returns the stable machine key for this MIR construction failure.
    pub const fn as_str(self) -> &'static str {
        self.kind.as_str()
    }
}

/// Stable MIR identity local to one compilation unit.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticMirUnitLocalIdentity {
    unit: u32,
    slot: u32,
}

impl DiagnosticMirUnitLocalIdentity {
    /// Creates a unit-local MIR identity.
    pub const fn new(unit: u32, slot: u32) -> Self {
        Self { unit, slot }
    }

    /// Returns the compilation-local MIR unit identity.
    pub const fn unit(self) -> u32 {
        self.unit
    }

    /// Returns the unit-local MIR arena slot.
    pub const fn slot(self) -> u32 {
        self.slot
    }
}

/// Exact payload retained by one MIR construction failure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticMirUnitBuildFailureContext {
    /// The failure carries no additional payload.
    None,
    /// The failure crossed MIR unit boundaries.
    UnitMismatch {
        /// The expected MIR unit identity.
        expected: u32,
        /// The actual MIR unit identity.
        actual: u32,
    },
    /// The affected MIR block.
    Block(DiagnosticMirUnitLocalIdentity),
    /// The affected MIR operation.
    Operation(DiagnosticMirUnitLocalIdentity),
    /// The affected MIR storage identity.
    Storage(DiagnosticMirUnitLocalIdentity),
    /// The affected MIR value.
    Value(DiagnosticMirUnitLocalIdentity),
    /// A cleanup phase and the block selected as its target.
    CleanupTarget {
        /// The cleanup phase.
        phase: &'static str,
        /// The selected cleanup target block.
        target: DiagnosticMirUnitLocalIdentity,
    },
    /// The expected and actual runtime ABI roles.
    RuntimeRoleMismatch {
        /// The required runtime ABI role.
        expected: &'static str,
        /// The supplied runtime ABI role.
        actual: &'static str,
    },
}

/// Stable leaf category for one MIR construction failure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticMirUnitBuildFailureKind {
    /// The MIR source origin disagrees with the unit identity.
    SourceOriginMismatch,
    /// A unit-local identity could not fit the compact representation.
    IdentityCapacityExceeded,
    /// A block belongs to another MIR unit.
    ForeignBlock,
    /// An operation belongs to another MIR unit.
    ForeignOperation,
    /// A storage identity belongs to another MIR unit.
    ForeignStorage,
    /// A value belongs to another MIR unit.
    ForeignValue,
    /// A referenced block is absent.
    MissingBlock,
    /// A referenced operation is absent.
    MissingOperation,
    /// An operation did not publish its required result.
    MissingOperationResult,
    /// An operation published a result when none is permitted.
    UnexpectedOperationResult,
    /// An operation result has the wrong type.
    OperationResultTypeMismatch,
    /// An aggregate operation violates its type contract.
    InvalidAggregateOperation,
    /// A memory operation violates its storage contract.
    InvalidMemoryOperation,
    /// An anonymous-callable operation violates its callable contract.
    InvalidAnonymousCallable,
    /// An implicit destructor remainder selects an incompatible lifecycle role.
    InvalidDestructorRemainder,
    /// A construction operation has incompatible inputs.
    InvalidConstructionInput,
    /// A call operation violates its callable contract.
    InvalidCall,
    /// A host operation violates its lifecycle contract.
    InvalidHostOperation,
    /// Host operations are not in the required lifecycle order.
    InvalidHostSequence,
    /// A referenced storage identity is absent.
    MissingStorage,
    /// A referenced value is absent.
    MissingValue,
    /// A block has more than one terminator.
    DuplicateTerminator,
    /// A block has no terminator.
    MissingTerminator,
    /// Inline assembly uses an invalid terminator shape.
    InvalidInlineAssemblyTerminator,
    /// A suspension terminator has an invalid payload.
    InvalidSuspensionPayload,
    /// A call terminator has an invalid panic check.
    InvalidCallPanicCheck,
    /// A control-flow edge has the wrong argument count.
    EdgeArgumentCountMismatch,
    /// A control-flow edge has incompatible argument types.
    EdgeArgumentTypeMismatch,
    /// A switch contains a duplicate case.
    DuplicateSwitchCase,
    /// A cleanup edge targets a block for another phase.
    CleanupTargetMismatch,
    /// Cleanup phases occur in an invalid order.
    CleanupPhaseOrderViolation,
    /// A runtime reference uses the wrong ABI role.
    RuntimeRoleMismatch,
    /// A runtime reference uses an incompatible ABI version.
    RuntimeAbiVersionMismatch,
    /// An operation was inserted into the wrong block.
    InvalidOperationBlock,
    /// A storage identity has an incompatible storage kind.
    StorageKindMismatch,
    /// A storage identity has an incompatible type.
    StorageTypeMismatch,
    /// A value does not dominate one of its uses.
    ValueDoesNotDominateUse,
    /// Protected-frame metadata disagrees with the unit contract.
    ProtectedFrameMismatch,
    /// A protected unit has no frame descriptor.
    MissingFrameDescriptor,
    /// A protected unit has more than one frame descriptor.
    DuplicateFrameDescriptor,
    /// An ordinary unit unexpectedly has a frame descriptor.
    UnexpectedFrameDescriptor,
    /// A frame-state entry references an invalid block.
    InvalidFrameStateEntry,
    /// A required protected-frame state is absent.
    MissingFrameState,
}

impl DiagnosticMirUnitBuildFailureKind {
    /// Returns the stable machine key for this MIR construction category.
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
            Self::InvalidDestructorRemainder => "executable_code_invalid_destructor_remainder",
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
