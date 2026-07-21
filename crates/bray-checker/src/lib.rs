//! Semantic checking for bound Bray programs.

#![forbid(unsafe_code)]

mod analysis;
mod constant;
mod context;
mod diagnostic;
mod outcome;
mod representation;
mod selection;
mod semantic_context;
mod service;
mod type_check;
mod unit;

#[cfg(test)]
mod test_support;

pub use constant::{
    ArrayLengthError, ConstantEvaluationInput, ConstantEvaluationLimits, ConstantLiteralError,
    ConstantReferenceResolution, check_array_length, check_constant_literal,
    normalize_integer_literal,
};
pub use context::{
    CheckerFactError, CheckerFactResult, CheckerInfrastructureError, CheckerRequestContext,
    CheckerSemanticFactProvider, CheckerSource,
};
pub use outcome::{CheckerOutcome, ControlFlowCheckResult};
pub use selection::{
    CallableCandidate, CallableCandidateState, CallableCandidateTemplate,
    CallableCandidateTemplateState, CallableCandidateTemplates,
    CallableDeclarationCandidateTemplate, CallableParameterDefaultTemplate,
    CallableSelectionRequest, CallableValueCandidateTemplate, CandidateAbsence, CandidateSelection,
    CompilerKnownOperationEvidence, ConstructionInputSurface, ExpressionCandidateSet,
    ImplementationSelectionEvidence, OperationCandidate, OperationCandidateState,
    OperationSelectionRequest, ReceiverCapability, ReceiverSelection, SelectionCandidateKey,
    SelectionFailure,
};
pub use semantic_context::{
    AnonymousCallableContext, ContractClauseContext, DeclaredUnitContext, SemanticUnitContext,
};
pub use service::{
    ConstantEvaluator, ControlFlowChecker, DefaultConstantEvaluator, DefaultControlFlowChecker,
    DefaultExpressionTypeChecker, DefaultSemanticSelector, ExpressionTypeChecker, SemanticSelector,
};
pub use type_check::{ExpressionTypeEvidence, ExpressionTypeExpectation, ExpressionTypeInput};
pub use unit::{CheckerUnitRoot, CheckerUnitView, CheckerUnitViewError};
