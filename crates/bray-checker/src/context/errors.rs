use crate::{CheckedConstantTermsBuildError, CheckerUnitViewError};
use bray_bound_tree::{
    AnyBoundNodeId, AsyncAnalysisBuildError, BorrowCapabilityId, BoundBlockId,
    BoundDependencyContractId, BoundExpressionId, BoundPatternId, BoundUnitId, BoundUnitKind,
    CheckedMemoryOperationsBuildError, DependencyContractsBuildError, LivenessBuildError,
    SemanticSelectionTableBuildError, SemanticSnapshotBuildError, StorageAccessId,
    StorageFlowBuildError, StorageIdentityId, StorageOperationStatus, StoragePlanBuildError,
};
use bray_compiler_known::{ImplementationHook, RepresentationRole};
use bray_source::{SourceId, SourceSpan, SourceVersion};
use bray_symbols::{
    AnyLocalSymbolId, AnySymbolId, ConstantTermId, GenericArgumentKind,
    GenericSubstitutionShapeError, LocalBindingSymbolId, SymbolQueryKind,
};

/// A checker infrastructure failure that is neither a source diagnostic nor cancellation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerInfrastructureError {
    /// The compilation does not contain the source named by a bound anchor.
    MissingSource {
        /// The unavailable source identity.
        source_id: SourceId,
    },
    /// A bound anchor names a different source revision than the compilation.
    SourceVersionMismatch {
        /// The source whose revision did not match.
        source_id: SourceId,
        /// The revision retained by the bound anchor.
        expected: SourceVersion,
        /// The revision available in the compilation.
        actual: SourceVersion,
    },
    /// A bound anchor does not cover a valid UTF-8 range in its source revision.
    InvalidSourceRange {
        /// The invalid source span.
        span: SourceSpan,
    },
    /// A required semantic query could not be supplied.
    SemanticQueryUnavailable {
        /// The exact symbol that owns the query.
        symbol: AnySymbolId,
        /// The unavailable query category.
        kind: SymbolQueryKind,
    },
    /// Canonical semantic value construction or lookup failed without an available store cause.
    SemanticValueUnavailable,
    /// Generic substitution construction rejected an exact parameter-to-argument relationship.
    GenericSubstitution(GenericSubstitutionShapeError),
    /// The canonical semantic value store rejected a construction or lookup operation.
    SemanticValueStore(bray_symbols::SemanticValueStoreError),
    /// The representation type for an atomic value is unavailable.
    AtomicRepresentationTypeUnavailable,
    /// The representation arguments for an atomic value are unavailable.
    AtomicRepresentationArgumentsUnavailable,
    /// The atomic initializer argument has no available compile-time value.
    AtomicInitializerArgumentUnavailable,
    /// The atomic initializer result cannot be retained as a compile-time value.
    AtomicInitializerResultUnavailable,
    /// Atomic classification received a hook outside the atomic operation catalog.
    InvalidAtomicOperationInput {
        /// Unexpected compiler-known hook.
        hook: ImplementationHook,
        /// Number of parsed atomic generic arguments.
        argument_count: usize,
    },
    /// The uninitialized-storage initializer result cannot be retained as a compile-time value.
    UninitInitializerResultUnavailable,
    /// An imported native operation does not match its compiled definition.
    ImportedExecutableTemplateMismatch,
    /// A required compiler-known representation is unavailable for the selected target.
    CompilerKnownRepresentationUnavailable {
        /// The unavailable representation role.
        role: RepresentationRole,
    },
    /// Type-checking input names an expression outside the requested unit.
    InvalidExpressionTypeInput {
        /// The invalid expression identity.
        expression: BoundExpressionId,
    },
    /// One correlated checker input belongs to another bound unit.
    IncompatibleInput {
        /// The input table whose identity disagreed with the requested unit.
        input: CheckerInputKind,
        /// Requested bound unit identity.
        expected_unit: BoundUnitId,
        /// Requested bound unit category.
        expected_kind: BoundUnitKind,
        /// Input table's bound unit identity.
        actual_unit: BoundUnitId,
        /// Input table's bound unit category.
        actual_kind: BoundUnitKind,
    },
    /// Checked constant occurrences could not form one unambiguous term table.
    CheckedConstantTerms(CheckedConstantTermsBuildError),
    /// Literal-value table construction rejected one exact input relationship.
    LiteralValue(CheckerLiteralValueFailure),
    /// Pattern-checking input construction retained conflicting evidence.
    PatternInput(CheckerPatternInputFailure),
    /// Constant-evaluation input construction retained conflicting evidence.
    ConstantInput(CheckerConstantInputFailure),
    /// Constant evaluation encountered an invalid source-correlated input.
    ConstantEvaluation(CheckerConstantEvaluationFailure),
    /// Constant comparison rejected one exact scalar operation.
    ConstantOperation(CheckerConstantOperationFailure),
    /// A semantic-selection input ordinal cannot be represented by the public protocol.
    SelectionInputCapacityExceeded {
        /// Exact zero-based input count that exceeded the protocol.
        count: usize,
    },
    /// A callable parameter ordinal cannot address the selected host representation.
    SelectionInputOrdinalUnrepresentable {
        /// Exact callable parameter ordinal.
        ordinal: u32,
    },
    /// A diagnostic selection summary cannot represent the complete candidate count.
    SelectionDiagnosticCapacityExceeded {
        /// Diagnostic selection summary category.
        kind: &'static str,
        /// Exact candidate or rejection count.
        count: usize,
    },
    /// A callback parameter position cannot be represented by the diagnostic protocol.
    CallbackParameterOrdinalUnrepresentable {
        /// Exact zero-based parameter position.
        ordinal: usize,
    },
    /// A constant array length cannot be represented by the semantic-value protocol.
    ConstantArrayLengthCapacityExceeded {
        /// Exact array length that exceeded the protocol.
        length: usize,
    },
    /// Semantic-selection inputs do not describe the requested bound unit or operation category.
    InvalidSemanticSelectionInput,
    /// Memory-operation classification received a hook outside its operation catalog.
    InvalidMemoryOperationInput {
        /// Unexpected compiler-known hook.
        hook: ImplementationHook,
    },
    /// A memory operation received a generic argument with the wrong category.
    InvalidMemoryGenericArgument {
        /// Stable zero-based generic argument position.
        ordinal: usize,
        /// Actual argument category.
        actual: GenericArgumentKind,
    },
    /// Callback validation received a non-callable type-expression template.
    InvalidCallbackSignatureInput {
        /// Actual type-expression template category.
        actual: &'static str,
    },
    /// Construction of the final semantic-selection table rejected one exact relationship.
    SemanticSelection(SemanticSelectionTableBuildError),
    /// Constant-evaluation inputs do not describe the requested bound unit.
    InvalidConstantEvaluationInput,
    /// Storage-planning inputs or constructed records violate the requested unit contract.
    InvalidStoragePlan,
    /// The storage-plan builder rejected one exact relationship.
    StoragePlan(StoragePlanBuildError),
    /// Final checked memory-operation table construction rejected one exact relationship.
    MemoryOperations(CheckedMemoryOperationsBuildError),
    /// Liveness inputs or durable decisions violate the requested unit contract.
    InvalidLiveness,
    /// Durable liveness construction rejected one exact relationship.
    Liveness(LivenessBuildError),
    /// Refinement inputs do not describe the requested bound unit.
    InvalidRefinementInput,
    /// Refinement resource counts cannot be represented by the diagnostic protocol.
    RefinementCapacityUnrepresentable,
    /// Host allocation failed while constructing refinement analysis storage.
    RefinementStorageUnavailable,
    /// One exact storage-flow contract was violated.
    StorageFlow(CheckerStorageFlowFailure),
    /// Storage-flow analysis produced a state forbidden for one exact planned source operation.
    InvalidStorageOperation {
        /// Source expression whose planned access produced the forbidden state.
        expression: BoundExpressionId,
        /// Planned storage access whose state was rejected.
        access: StorageAccessId,
        /// Exact rejected storage state.
        status: StorageOperationStatus,
    },
    /// Correlated body-semantic inputs or durable results violate the requested unit contract.
    InvalidBodySemantics,
    /// Correlated semantic results describe different bound units or unit categories.
    SemanticSnapshot(SemanticSnapshotBuildError),
    /// A committed bound relationship names a node absent from the requested unit.
    InvalidBoundNode {
        /// The missing bound node identity.
        node: AnyBoundNodeId,
    },
    /// One unit contains more expression variables than the checker can identify compactly.
    ExpressionTypeCapacityExceeded,
    /// A checker unit view did not match its canonical bound unit.
    InvalidUnitView(CheckerUnitViewError),
}

/// Exact scalar-operation failure retained when checking constant equality.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerConstantOperationFailure {
    /// The operand or operation category is invalid for constant folding.
    Invalid,
    /// Constant evaluation attempted division by zero.
    DivisionByZero,
    /// The exact constant result cannot be represented by the requested type.
    NotRepresentable,
    /// The operation exceeded its configured resource limit.
    ResourceLimitExceeded {
        /// Observed resource demand.
        actual: u64,
        /// Maximum permitted demand.
        maximum: u64,
    },
}

/// Identifies one correlated semantic input supplied to a checker service.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerInputKind {
    /// Checked asynchronous behavior.
    AsyncAnalysis,
    /// Checked control-flow structure.
    ControlFlow,
    /// Declaration-provided value type templates.
    DeclaredValueTypes,
    /// Complete checked expression semantics.
    ExpressionSemantics,
    /// Checked expression types.
    ExpressionTypes,
    /// Checked literal values.
    LiteralValues,
    /// Checked memory operations.
    MemoryOperations,
    /// Checked pattern semantics.
    Patterns,
    /// Checked semantic selections.
    SemanticSelections,
    /// Planned storage operations.
    StoragePlan,
}

impl CheckerInputKind {
    /// Returns this input category's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AsyncAnalysis => "async_analysis",
            Self::ControlFlow => "control_flow",
            Self::DeclaredValueTypes => "declared_value_types",
            Self::ExpressionSemantics => "expression_semantics",
            Self::ExpressionTypes => "expression_types",
            Self::LiteralValues => "literal_values",
            Self::MemoryOperations => "memory_operations",
            Self::Patterns => "patterns",
            Self::SemanticSelections => "semantic_selections",
            Self::StoragePlan => "storage_plan",
        }
    }
}

/// One exact literal-value table contract violation retained by the checker boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerLiteralValueFailure {
    /// The expression-type table belongs to another unit.
    ForeignExpressionTypes,
    /// An entry does not name a literal expression.
    InvalidLiteral {
        /// The invalid literal expression identity.
        expression: BoundExpressionId,
    },
    /// A literal expression has no checked type.
    MissingExpressionType {
        /// The literal expression without a checked type.
        expression: BoundExpressionId,
    },
    /// A literal expression has no checked value.
    MissingLiteralValue {
        /// The literal expression without a checked value.
        expression: BoundExpressionId,
    },
    /// A literal value has a type different from its expression.
    ValueTypeMismatch {
        /// The literal expression whose value has another type.
        expression: BoundExpressionId,
    },
    /// More than one value was supplied for one literal expression.
    DuplicateExpression {
        /// The repeated literal expression identity.
        expression: BoundExpressionId,
    },
}

/// Conflicting evidence retained while constructing one pattern-checking request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerPatternInputFailure {
    /// One pattern has conflicting declared type templates.
    ConflictingDeclaredPattern {
        /// The pattern with conflicting declared types.
        pattern: BoundPatternId,
    },
    /// One pattern has conflicting constant evidence.
    ConflictingConstantPattern {
        /// The pattern with conflicting constants.
        pattern: BoundPatternId,
    },
    /// One guard expression has conflicting constant values.
    ConflictingGuard {
        /// The guard expression with conflicting constants.
        expression: BoundExpressionId,
    },
}

/// Conflicting evidence retained while constructing one constant-evaluation request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerConstantInputFailure {
    /// One reference expression has conflicting resolutions.
    ConflictingReference {
        /// The reference expression with conflicting resolutions.
        expression: BoundExpressionId,
    },
    /// One local constant has conflicting symbolic terms.
    ConflictingLocalTerm {
        /// The local constant with conflicting terms.
        local: AnyLocalSymbolId,
    },
}

/// One invalid source-correlated input encountered during constant evaluation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerConstantEvaluationFailure {
    /// The requested expression root is absent from the unit.
    InvalidExpressionRoot {
        /// The unavailable expression root.
        expression: BoundExpressionId,
    },
    /// The requested block root is absent from the unit.
    InvalidBlockRoot {
        /// The unavailable block root.
        block: BoundBlockId,
    },
    /// The selected expression has no checked type.
    MissingExpressionType {
        /// The expression without a checked type.
        expression: BoundExpressionId,
    },
    /// A block-rooted request has no declared result type.
    MissingBlockResultType {
        /// The block without a result type.
        block: BoundBlockId,
    },
    /// Evaluation reached an expression absent from the unit.
    MissingExpression {
        /// The unavailable expression identity.
        expression: BoundExpressionId,
    },
    /// Evaluation reached a block absent from the unit.
    MissingBlock {
        /// The unavailable block identity.
        block: BoundBlockId,
    },
    /// Pattern evaluation was requested without checked pattern input.
    MissingPatternInput {
        /// The pattern that required checked input.
        pattern: BoundPatternId,
    },
    /// A requested pattern is absent from the unit or checked pattern input.
    MissingPattern {
        /// The unavailable pattern identity.
        pattern: BoundPatternId,
    },
    /// A requested pattern binding has no checked projection.
    MissingPatternBinding {
        /// The binding without a checked pattern projection.
        binding: LocalBindingSymbolId,
    },
    /// Closed evaluation unexpectedly retained a propagating term.
    UnexpectedPropagation {
        /// The symbolic term that unexpectedly propagated.
        term: ConstantTermId,
    },
}

/// The exact storage-flow contract violated by checker inputs or constructed analysis.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerStorageFlowFailure {
    /// One analysis input belongs to a different checked source body.
    IncompatibleInput {
        /// Input table whose identity disagreed with the requested source body.
        input: StorageFlowInputKind,
        /// Requested source body identity.
        expected_unit: BoundUnitId,
        /// Requested source body category.
        expected_kind: BoundUnitKind,
        /// Input table's source body identity.
        actual_unit: BoundUnitId,
        /// Input table's source body category.
        actual_kind: BoundUnitKind,
    },
    /// Durable storage-flow construction rejected an exact invariant.
    FlowConstruction(StorageFlowBuildError),
    /// A selected call or iteration produced a dependency contract for another source body.
    ForeignDependencyContract,
    /// A call reached concrete dependency checking without its selected witness.
    UnresolvedDependencyWitness { expression: BoundExpressionId },
    /// Durable dependency-contract construction rejected an exact invariant.
    DependencyContractsConstruction(DependencyContractsBuildError),
    /// Durable async-analysis construction rejected an exact invariant.
    AsyncConstruction(AsyncAnalysisBuildError),
    /// A non-recovered await expression has no selected dependency contract.
    MissingAwaitDependencyContract { expression: BoundExpressionId },
    /// An await expression names a dependency contract absent from its source body.
    MissingDependencyContract {
        expression: BoundExpressionId,
        contract: BoundDependencyContractId,
    },
    /// The callable signature and callable type disagree about their parameter count.
    CallableParameterCountMismatch {
        callable: AnySymbolId,
        signature_parameters: usize,
        type_parameters: usize,
    },
    /// A callable declaration's resolved type is not callable.
    CallableTypeNotCallable { callable: AnySymbolId },
    /// A storage borrow identity has no retained capability.
    MissingBorrowCapability { borrow: BorrowCapabilityId },
    /// A control-flow exit has no retained source origin.
    MissingExitOrigin { exit: AnyBoundNodeId },
    /// A control-flow transfer names a block absent from its source body.
    MissingBlock { block: BoundBlockId },
    /// A planned storage access is absent from its source body's storage plan.
    MissingStorageAccess { access: StorageAccessId },
    /// A planned storage identity is absent from its source body's storage plan.
    MissingStorageIdentity { identity: StorageIdentityId },
    /// A storage identity refers to a declaration whose name is unavailable.
    MissingStorageSymbolName { symbol: AnySymbolId },
    /// Walking a source body ended with unbalanced lexical scopes.
    UnbalancedScopes { open_scope: Option<BoundBlockId> },
    /// A pattern referenced while assigning lexical ownership is absent from its source body.
    MissingPattern { pattern: BoundPatternId },
}

/// Identifies one correlated input to storage-flow-related analysis.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StorageFlowInputKind {
    ExpressionTypes,
    SemanticSelections,
    StoragePlan,
    Liveness,
    Refinements,
    MemoryOperations,
    StorageFlow,
    DependencyContracts,
}

/// A failure while requesting a checker dependency.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerQueryError<Upstream = std::convert::Infallible> {
    /// Cancellation was observed while obtaining the dependency.
    Cancelled,
    /// Compiler infrastructure could not supply the dependency.
    Infrastructure(CheckerInfrastructureError),
    /// The coordinating query layer returned one of its own exact failures.
    Upstream(Upstream),
}

impl CheckerQueryError {
    /// Widens a checker-local failure to a boundary with an upstream error type.
    pub fn with_upstream<Upstream>(self) -> CheckerQueryError<Upstream> {
        match self {
            Self::Cancelled => CheckerQueryError::Cancelled,
            Self::Infrastructure(error) => CheckerQueryError::Infrastructure(error),
            Self::Upstream(error) => match error {},
        }
    }
}

impl<Upstream> From<CheckerInfrastructureError> for CheckerQueryError<Upstream> {
    fn from(error: CheckerInfrastructureError) -> Self {
        Self::Infrastructure(error)
    }
}

/// The result of requesting one checker dependency.
pub type CheckerQueryResult<T, Upstream = std::convert::Infallible> =
    Result<T, CheckerQueryError<Upstream>>;
