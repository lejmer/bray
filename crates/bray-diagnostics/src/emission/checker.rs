/// Exact checker contract failure observed while compiling a product.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCheckerFailure {
    MissingSource {
        source_id: bray_source::SourceId,
    },
    SourceVersionMismatch {
        source_id: bray_source::SourceId,
        expected: bray_source::SourceVersion,
        actual: bray_source::SourceVersion,
    },
    InvalidSourceRange {
        span: bray_source::SourceSpan,
    },
    SemanticQueryUnavailable {
        symbol: DiagnosticCheckerSymbol,
        query: &'static str,
    },
    SemanticValueUnavailable,
    GenericSubstitution(DiagnosticGenericSubstitutionFailure),
    SemanticValue(crate::DiagnosticSemanticValueFailure),
    AtomicRepresentationTypeUnavailable,
    AtomicRepresentationArgumentsUnavailable,
    AtomicInitializerArgumentUnavailable,
    AtomicInitializerResultUnavailable,
    InvalidAtomicOperationInput {
        hook: &'static str,
        argument_count: usize,
    },
    UninitInitializerResultUnavailable,
    ImportedExecutableTemplateMismatch,
    CompilerKnownRepresentationUnavailable(&'static str),
    InvalidExpressionTypeInput {
        expression: DiagnosticCheckerNode,
    },
    IncompatibleInput {
        input: &'static str,
        expected_unit: u32,
        expected_kind: &'static str,
        actual_unit: u32,
        actual_kind: &'static str,
    },
    CheckedConstantTerms {
        owner: DiagnosticCheckerSymbol,
        source: bray_source::SourceSpan,
    },
    LiteralValue(DiagnosticLiteralValueFailure),
    PatternInput(DiagnosticPatternInputFailure),
    ConstantInput(DiagnosticConstantInputFailure),
    ConstantEvaluation(DiagnosticConstantEvaluationFailure),
    ConstantOperation(DiagnosticCheckerConstantOperationFailure),
    SelectionInputCapacityExceeded {
        count: usize,
    },
    SelectionInputOrdinalUnrepresentable {
        ordinal: u32,
    },
    SelectionDiagnosticCapacityExceeded {
        kind: &'static str,
        count: usize,
    },
    CallbackParameterOrdinalUnrepresentable {
        ordinal: usize,
    },
    ConstantArrayLengthCapacityExceeded {
        length: usize,
    },
    InvalidSemanticSelectionInput,
    InvalidMemoryOperationInput {
        hook: &'static str,
    },
    InvalidMemoryGenericArgument {
        ordinal: usize,
        actual: &'static str,
    },
    InvalidCallbackSignatureInput {
        actual: &'static str,
    },
    /// Final semantic-selection table construction rejected one exact relationship.
    SemanticSelection(DiagnosticSemanticSelectionFailure),
    InvalidConstantEvaluationInput,
    InvalidStoragePlan,
    StoragePlan(DiagnosticStoragePlanFailure),
    MemoryOperations(DiagnosticMemoryOperationsFailure),
    InvalidLiveness,
    Liveness(DiagnosticLivenessFailure),
    InvalidRefinementInput,
    RefinementCapacityUnrepresentable,
    RefinementStorageUnavailable,
    StorageFlow(DiagnosticStorageFlowFailure),
    InvalidStorageOperation {
        expression: DiagnosticCheckerNode,
        access: u32,
        status: &'static str,
    },
    InvalidBodySemantics,
    /// Correlated semantic results describe different bound units or unit categories.
    SemanticSnapshot(DiagnosticSemanticSnapshotFailure),
    InvalidBoundNode {
        node: DiagnosticCheckerNode,
    },
    ExpressionTypeCapacityExceeded,
    InvalidUnitView(&'static str),
}

/// Exact generic-substitution shape failure retained across compiler boundaries.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticGenericSubstitutionFailure {
    ArgumentCountMismatch {
        parameter_count: usize,
        argument_count: usize,
    },
    ArgumentKindMismatch {
        ordinal: u32,
        expected: &'static str,
        actual: &'static str,
    },
    OrdinalOverflow,
}

/// Exact constant-operation failure retained across compiler boundaries.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCheckerConstantOperationFailure {
    Invalid,
    DivisionByZero,
    NotRepresentable,
    ResourceLimitExceeded { actual: u64, maximum: u64 },
}

/// Exact storage-plan construction failure retained across compiler boundaries.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticStoragePlanFailure {
    ForeignUnit,
    CapacityExceeded,
    MissingIdentity,
    MissingAccess,
    MissingBorrowCapability,
    DuplicateBinding,
    BindingIdentityMismatch,
}

/// Exact checked memory-operation table failure retained across compiler boundaries.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticMemoryOperationsFailure {
    ForeignUnit,
    DuplicateExpression,
}

/// Exact durable liveness failure retained across compiler boundaries.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLivenessFailure {
    ForeignUnit,
    UnsupportedSubject,
}

/// Locale-neutral identity retained for a declaration involved in a checker failure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticCheckerSymbol {
    kind: &'static str,
    ordinal: u32,
}

impl DiagnosticCheckerSymbol {
    pub const fn new(kind: &'static str, ordinal: u32) -> Self {
        Self { kind, ordinal }
    }

    pub const fn kind(self) -> &'static str {
        self.kind
    }

    pub const fn ordinal(self) -> u32 {
        self.ordinal
    }
}

/// Locale-neutral identity retained for a source construct involved in a checker failure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticCheckerNode {
    kind: &'static str,
    unit: u32,
    ordinal: u32,
}

impl DiagnosticCheckerNode {
    pub const fn new(kind: &'static str, unit: u32, ordinal: u32) -> Self {
        Self {
            kind,
            unit,
            ordinal,
        }
    }

    pub const fn kind(self) -> &'static str {
        self.kind
    }

    pub const fn unit(self) -> u32 {
        self.unit
    }

    pub const fn ordinal(self) -> u32 {
        self.ordinal
    }
}

/// Locale-neutral identity retained for a local symbol involved in a checker failure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticCheckerLocal {
    kind: &'static str,
    region: u32,
    ordinal: u32,
}

impl DiagnosticCheckerLocal {
    pub const fn new(kind: &'static str, region: u32, ordinal: u32) -> Self {
        Self {
            kind,
            region,
            ordinal,
        }
    }

    pub const fn kind(self) -> &'static str {
        self.kind
    }

    pub const fn region(self) -> u32 {
        self.region
    }

    pub const fn ordinal(self) -> u32 {
        self.ordinal
    }
}

/// Exact literal-value contract failure retained across compiler boundaries.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLiteralValueFailure {
    ForeignExpressionTypes,
    InvalidLiteral(DiagnosticCheckerNode),
    MissingExpressionType(DiagnosticCheckerNode),
    MissingLiteralValue(DiagnosticCheckerNode),
    ValueTypeMismatch(DiagnosticCheckerNode),
    DuplicateExpression(DiagnosticCheckerNode),
}

/// Exact pattern-input conflict retained across compiler boundaries.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticPatternInputFailure {
    ConflictingDeclaredPattern(DiagnosticCheckerNode),
    ConflictingConstantPattern(DiagnosticCheckerNode),
    ConflictingGuard(DiagnosticCheckerNode),
}

/// Exact constant-input conflict retained across compiler boundaries.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticConstantInputFailure {
    ConflictingReference(DiagnosticCheckerNode),
    ConflictingLocalTerm(DiagnosticCheckerLocal),
}

/// Exact constant-evaluation contract failure retained across compiler boundaries.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticConstantEvaluationFailure {
    InvalidExpressionRoot(DiagnosticCheckerNode),
    InvalidBlockRoot(DiagnosticCheckerNode),
    MissingExpressionType(DiagnosticCheckerNode),
    MissingBlockResultType(DiagnosticCheckerNode),
    MissingExpression(DiagnosticCheckerNode),
    MissingBlock(DiagnosticCheckerNode),
    MissingPatternInput(DiagnosticCheckerNode),
    MissingPattern(DiagnosticCheckerNode),
    MissingPatternBinding(DiagnosticCheckerLocal),
    UnexpectedPropagation { store: u64, slot: u32 },
}

/// Exact semantic-selection table contract failure retained across compiler boundaries.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSemanticSelectionFailure {
    /// Expression types describe another bound unit.
    ForeignExpressionTypes {
        /// Requested bound unit identity.
        expected_unit: u32,
        /// Requested bound unit category.
        expected_kind: &'static str,
        /// Supplied bound unit identity.
        actual_unit: u32,
        /// Supplied bound unit category.
        actual_kind: &'static str,
    },
    /// A selection names no expression in the requested unit.
    InvalidExpression(DiagnosticCheckerNode),
    /// More than one selection was supplied for one expression.
    DuplicateExpression(DiagnosticCheckerNode),
    /// A selection category does not match its bound expression.
    SelectionKindMismatch(DiagnosticCheckerNode),
    /// A selected result disagrees with the final expression type.
    ResultTypeMismatch(DiagnosticCheckerNode),
    /// A selected operand disagrees with the final operand type.
    OperandTypeMismatch(DiagnosticCheckerNode),
    /// An implementation subject disagrees with the final subject type.
    SubjectTypeMismatch(DiagnosticCheckerNode),
}

/// Exact semantic-snapshot identity mismatch retained across compiler boundaries.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticSemanticSnapshotFailure {
    input: &'static str,
    expected_unit: u32,
    expected_kind: &'static str,
    actual_unit: u32,
    actual_kind: &'static str,
}

impl DiagnosticSemanticSnapshotFailure {
    /// Creates one locale-neutral semantic-snapshot failure.
    pub const fn new(
        input: &'static str,
        expected_unit: u32,
        expected_kind: &'static str,
        actual_unit: u32,
        actual_kind: &'static str,
    ) -> Self {
        Self {
            input,
            expected_unit,
            expected_kind,
            actual_unit,
            actual_kind,
        }
    }

    /// Returns the semantic result whose identity disagreed.
    pub const fn input(self) -> &'static str {
        self.input
    }

    /// Returns the requested bound unit identity.
    pub const fn expected_unit(self) -> u32 {
        self.expected_unit
    }

    /// Returns the requested bound unit category.
    pub const fn expected_kind(self) -> &'static str {
        self.expected_kind
    }

    /// Returns the supplied bound unit identity.
    pub const fn actual_unit(self) -> u32 {
        self.actual_unit
    }

    /// Returns the supplied bound unit category.
    pub const fn actual_kind(self) -> &'static str {
        self.actual_kind
    }
}

impl DiagnosticCheckerFailure {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingSource { .. } => "checker_missing_source",
            Self::SourceVersionMismatch { .. } => "checker_source_version_mismatch",
            Self::InvalidSourceRange { .. } => "checker_invalid_source_range",
            Self::SemanticQueryUnavailable { .. } => "checker_semantic_query_unavailable",
            Self::SemanticValueUnavailable => "checker_semantic_value_unavailable",
            Self::GenericSubstitution(_) => "checker_generic_substitution_failure",
            Self::SemanticValue(failure) => failure.as_str(),
            Self::AtomicRepresentationTypeUnavailable => "atomic_representation_type_unavailable",
            Self::AtomicRepresentationArgumentsUnavailable => {
                "atomic_representation_arguments_unavailable"
            }
            Self::AtomicInitializerArgumentUnavailable => "atomic_initializer_argument_unavailable",
            Self::AtomicInitializerResultUnavailable => "atomic_initializer_result_unavailable",
            Self::InvalidAtomicOperationInput { .. } => "checker_invalid_atomic_operation_input",
            Self::UninitInitializerResultUnavailable => "uninit_initializer_result_unavailable",
            Self::ImportedExecutableTemplateMismatch => "imported_executable_template_mismatch",
            Self::CompilerKnownRepresentationUnavailable(_) => {
                "checker_compiler_known_representation_unavailable"
            }
            Self::InvalidExpressionTypeInput { .. } => "checker_invalid_expression_type_input",
            Self::IncompatibleInput { .. } => "checker_incompatible_input",
            Self::CheckedConstantTerms { .. } => "checker_checked_constant_terms",
            Self::LiteralValue(_) => "checker_literal_value_failure",
            Self::PatternInput(_) => "checker_pattern_input_failure",
            Self::ConstantInput(_) => "checker_constant_input_failure",
            Self::ConstantEvaluation(_) => "checker_constant_evaluation_failure",
            Self::ConstantOperation(_) => "checker_constant_operation_failure",
            Self::SelectionInputCapacityExceeded { .. } => {
                "checker_selection_input_capacity_exceeded"
            }
            Self::SelectionInputOrdinalUnrepresentable { .. } => {
                "checker_selection_input_ordinal_unrepresentable"
            }
            Self::SelectionDiagnosticCapacityExceeded { .. } => {
                "checker_selection_diagnostic_capacity_exceeded"
            }
            Self::CallbackParameterOrdinalUnrepresentable { .. } => {
                "checker_callback_parameter_ordinal_unrepresentable"
            }
            Self::ConstantArrayLengthCapacityExceeded { .. } => {
                "checker_constant_array_length_capacity_exceeded"
            }
            Self::InvalidSemanticSelectionInput => "checker_invalid_semantic_selection_input",
            Self::InvalidMemoryOperationInput { .. } => "checker_invalid_memory_operation_input",
            Self::InvalidMemoryGenericArgument { .. } => "checker_invalid_memory_generic_argument",
            Self::InvalidCallbackSignatureInput { .. } => {
                "checker_invalid_callback_signature_input"
            }
            Self::SemanticSelection(_) => "checker_semantic_selection_failure",
            Self::InvalidConstantEvaluationInput => "checker_invalid_constant_evaluation_input",
            Self::InvalidStoragePlan => "checker_invalid_storage_plan",
            Self::StoragePlan(_) => "checker_storage_plan_failure",
            Self::MemoryOperations(_) => "checker_memory_operations_failure",
            Self::InvalidLiveness => "checker_invalid_liveness",
            Self::Liveness(_) => "checker_liveness_failure",
            Self::InvalidRefinementInput => "checker_invalid_refinement_input",
            Self::RefinementCapacityUnrepresentable => {
                "checker_refinement_capacity_unrepresentable"
            }
            Self::RefinementStorageUnavailable => "checker_refinement_storage_unavailable",
            Self::StorageFlow(_) => "checker_storage_flow_failure",
            Self::InvalidStorageOperation { .. } => "checker_invalid_storage_operation",
            Self::InvalidBodySemantics => "checker_invalid_body_semantics",
            Self::SemanticSnapshot(_) => "checker_semantic_snapshot_failure",
            Self::InvalidBoundNode { .. } => "checker_invalid_bound_node",
            Self::ExpressionTypeCapacityExceeded => "checker_expression_type_capacity_exceeded",
            Self::InvalidUnitView(_) => "checker_invalid_unit_view",
        }
    }
}

/// Exact storage-flow contract failure retained across compiler boundaries.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticStorageFlowFailure {
    IncompatibleInput {
        input: &'static str,
        expected_unit: u32,
        expected_kind: &'static str,
        actual_unit: u32,
        actual_kind: &'static str,
    },
    FlowConstruction(&'static str),
    ForeignDependencyContract,
    DependencyContractsConstruction(&'static str),
    AsyncConstruction(&'static str),
    MissingAwaitDependencyContract {
        expression: DiagnosticCheckerNode,
    },
    MissingDependencyContract {
        expression: DiagnosticCheckerNode,
        contract_unit: u32,
        contract: u32,
    },
    CallableParameterCountMismatch {
        callable: DiagnosticCheckerSymbol,
        signature_parameters: usize,
        type_parameters: usize,
    },
    CallableTypeNotCallable {
        callable: DiagnosticCheckerSymbol,
    },
    MissingBorrowCapability {
        unit: u32,
        borrow: u32,
    },
    MissingExitOrigin {
        exit: DiagnosticCheckerNode,
    },
    MissingBlock {
        block: DiagnosticCheckerNode,
    },
    MissingStorageAccess {
        unit: u32,
        access: u32,
    },
    MissingStorageIdentity {
        unit: u32,
        identity: u32,
    },
    MissingStorageSymbolName {
        symbol: DiagnosticCheckerSymbol,
    },
    UnbalancedScopes {
        open_scope: Option<DiagnosticCheckerNode>,
    },
    MissingPattern {
        pattern: DiagnosticCheckerNode,
    },
}
