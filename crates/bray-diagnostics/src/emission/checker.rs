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
    SemanticValue(crate::DiagnosticSemanticValueFailure),
    AtomicRepresentationTypeUnavailable,
    AtomicRepresentationArgumentsUnavailable,
    AtomicInitializerArgumentUnavailable,
    AtomicInitializerResultUnavailable,
    UninitInitializerResultUnavailable,
    ImportedExecutableTemplateMismatch,
    CompilerKnownRepresentationUnavailable(&'static str),
    InvalidExpressionTypeInput {
        expression: DiagnosticCheckerNode,
    },
    InvalidSemanticSelectionInput,
    InvalidLiteralValueInput,
    InvalidConstantEvaluationInput,
    InvalidPatternCheckInput,
    InvalidStoragePlan,
    InvalidLiveness,
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
    InvalidBoundNode {
        node: DiagnosticCheckerNode,
    },
    ExpressionTypeCapacityExceeded,
    InvalidUnitView(&'static str),
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

impl DiagnosticCheckerFailure {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingSource { .. } => "checker_missing_source",
            Self::SourceVersionMismatch { .. } => "checker_source_version_mismatch",
            Self::InvalidSourceRange { .. } => "checker_invalid_source_range",
            Self::SemanticQueryUnavailable { .. } => "checker_semantic_query_unavailable",
            Self::SemanticValueUnavailable => "checker_semantic_value_unavailable",
            Self::SemanticValue(failure) => failure.as_str(),
            Self::AtomicRepresentationTypeUnavailable => "atomic_representation_type_unavailable",
            Self::AtomicRepresentationArgumentsUnavailable => {
                "atomic_representation_arguments_unavailable"
            }
            Self::AtomicInitializerArgumentUnavailable => "atomic_initializer_argument_unavailable",
            Self::AtomicInitializerResultUnavailable => "atomic_initializer_result_unavailable",
            Self::UninitInitializerResultUnavailable => "uninit_initializer_result_unavailable",
            Self::ImportedExecutableTemplateMismatch => "imported_executable_template_mismatch",
            Self::CompilerKnownRepresentationUnavailable(_) => {
                "checker_compiler_known_representation_unavailable"
            }
            Self::InvalidExpressionTypeInput { .. } => "checker_invalid_expression_type_input",
            Self::InvalidSemanticSelectionInput => "checker_invalid_semantic_selection_input",
            Self::InvalidLiteralValueInput => "checker_invalid_literal_value_input",
            Self::InvalidConstantEvaluationInput => "checker_invalid_constant_evaluation_input",
            Self::InvalidPatternCheckInput => "checker_invalid_pattern_check_input",
            Self::InvalidStoragePlan => "checker_invalid_storage_plan",
            Self::InvalidLiveness => "checker_invalid_liveness",
            Self::InvalidRefinementInput => "checker_invalid_refinement_input",
            Self::RefinementCapacityUnrepresentable => {
                "checker_refinement_capacity_unrepresentable"
            }
            Self::RefinementStorageUnavailable => "checker_refinement_storage_unavailable",
            Self::StorageFlow(_) => "checker_storage_flow_failure",
            Self::InvalidStorageOperation { .. } => "checker_invalid_storage_operation",
            Self::InvalidBodySemantics => "checker_invalid_body_semantics",
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
