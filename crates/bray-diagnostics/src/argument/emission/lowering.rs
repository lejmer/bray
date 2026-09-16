use bray_source::SourceSpan;

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
    /// An initialized call or construction input has no checked cleanup shape.
    MissingInputCleanup(DiagnosticLoweringIdentity),
    /// An await expression occurs outside a protected frame.
    AwaitOutsideProtectedFrame(DiagnosticLoweringIdentity),
    /// An await expression has no selected suspension point.
    MissingSuspensionPoint(DiagnosticLoweringIdentity),
    /// A task operation violates its checked operation contract.
    InvalidTaskOperation(DiagnosticLoweringIdentity),
    /// The lowered callable has no result type.
    MissingCallableResultType,
    /// Active lexical scopes do not contain the requested cleanup depth.
    InvalidCleanupScopeDepth {
        /// First active scope position that must be cleaned.
        scope_depth: usize,
        /// Number of lexical scopes active at the exit.
        active_scope_count: usize,
        /// The source occurrence initiating cleanup.
        exit: DiagnosticLoweringIdentity,
    },
    /// Checked async analysis has no decision for one scope and exit.
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
    /// A MIR identity table exceeded its compact representation.
    MirCapacity,
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
            Self::MissingInputCleanup(_) => "code_production_input_cleanup_unavailable",
            Self::MissingCallableResultType => "code_production_callable_result_type_unavailable",
            Self::InvalidCleanupScopeDepth { .. } => "code_production_cleanup_scope_depth_invalid",
            Self::MissingScopeExitPlan { .. } => "code_production_scope_exit_plan_unavailable",
            Self::MissingLiteralValue(_) => "code_production_literal_value_unavailable",
            Self::MissingSemanticSelection(_) => "code_production_expression_behavior_unavailable",
            Self::UnsupportedExpression(_) => "code_production_expression_unsupported",
            Self::UnsupportedPattern(_) => "code_production_pattern_unsupported",
            Self::UnsupportedOperator { .. } => "code_production_operator_unsupported",
            Self::MissingStorageAccess(_) => "code_production_value_access_unavailable",
            Self::MissingStorageAccessRecord(_) => "code_production_value_access_record_unavailable",
            Self::MissingStorageIdentity(_) => "code_production_accessed_value_unavailable",
            Self::MissingStorageIdentityRecord(_) => "code_production_value_record_unavailable",
            Self::MissingIterationStorage(_) => "code_production_iteration_state_unavailable",
            Self::UnsupportedStorageAccess(_) => "code_production_value_access_unsupported",
            Self::MissingOperationResult(_) => "code_production_expression_value_unavailable",
            Self::MissingRepresentation(_) => "code_production_target_representation_unavailable",
            Self::SemanticValueUnavailable => "code_production_type_or_constant_unavailable",
            Self::GenericSubstitution(_) => "code_production_generic_substitution_invalid",
            Self::SemanticValue(failure) => failure.as_str(),
            Self::MirCapacity => "code_production_mir_capacity_exceeded",
            Self::MemoryArgumentOrdinalUnrepresentable { .. } => {
                "code_production_memory_argument_ordinal_unrepresentable"
            }
            Self::MatchArmOrdinalUnrepresentable { .. } => {
                "code_production_match_arm_ordinal_unrepresentable"
            }
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
