use bray_bound_tree::{BoundBlockId, BoundExpressionId, BoundUnitId, BoundUnitKind};
use bray_symbols::SemanticValueStoreError;

use crate::LoweringPlanFailure;

/// A semantic input required by source-unit lowering.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LoweringInputKind {
    /// Control-flow analysis.
    ControlFlow,
    /// Final expression types.
    ExpressionTypes,
    /// Pattern operations, binding types, and match coverage.
    Patterns,
    /// Source-literal values.
    LiteralValues,
    /// Closed values reached through source constant references.
    ConstantReferences,
    /// Flow-sensitive semantic refinements.
    Refinements,
    /// Verified async, task, cleanup, and runtime lowering plans.
    LoweringPlans,
    /// Effects, capabilities, execution requirements, and lifecycle obligations.
    BodyBehavior,
}

/// A contract violation that prevents a bound unit from entering lowering.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum LoweringInputError {
    /// A required semantic input belongs to another bound unit.
    ForeignInput {
        /// The required input category.
        input: LoweringInputKind,
        /// The bound unit requested for lowering.
        expected: BoundUnitId,
        /// The unit named by the supplied input.
        actual: BoundUnitId,
    },
    /// A required semantic input describes another unit category.
    InputKindMismatch {
        /// The required input category.
        input: LoweringInputKind,
        /// The category carried by the bound-unit key.
        expected: BoundUnitKind,
        /// The category named by the supplied input.
        actual: BoundUnitKind,
    },
    /// A successfully typed expression lacks the semantic choice required by lowering.
    MissingSemanticSelection(BoundExpressionId),
    /// A bound expression has no final checked type.
    MissingExpressionType(BoundExpressionId),
    /// Pattern analysis does not cover the bound patterns, bindings, and matches exactly.
    InvalidPatternInput,
    /// One input contains identities absent from its exact bound unit or dependent input.
    InvalidInputContents(LoweringInputKind),
    /// A semantic value required to validate the lowering input could not be read.
    SemanticValue(SemanticValueStoreError),
    /// A checked storage operation does not match the canonical storage plan.
    InvalidStorageOperation(BoundExpressionId),
    /// Checked storage operations do not cover every canonical access plan exactly once.
    StorageOperationCountMismatch {
        /// The number of canonical access plans.
        expected: usize,
        /// The number of checked operation decisions.
        actual: usize,
    },
    /// A scope-exit storage decision references an unknown scope, identity, access, or borrow.
    InvalidStorageExit(BoundBlockId),
    /// Async, cleanup, task, or runtime analyses cannot form one complete lowering plan.
    InvalidPlan(Box<LoweringPlanFailure>),
    /// Literal adaptation used a machine-sized integer width from another target.
    LiteralTargetWidthMismatch {
        /// The width required by the lowering target.
        expected: std::num::NonZeroU16,
        /// The width used while adapting source literals.
        actual: std::num::NonZeroU16,
    },
    /// A compiler-generated executable host was supplied through a source-unit lowering input.
    ExecutableHostRequiresSyntheticInput,
    /// A compile-time-only unit was supplied through executable MIR lowering.
    CompileTimeUnitRequiresClassification,
}

impl From<SemanticValueStoreError> for LoweringInputError {
    fn from(error: SemanticValueStoreError) -> Self {
        Self::SemanticValue(error)
    }
}

impl From<LoweringPlanFailure> for LoweringInputError {
    fn from(error: LoweringPlanFailure) -> Self {
        Self::InvalidPlan(Box::new(error))
    }
}
