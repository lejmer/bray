//! Semantic checking for bound Bray programs.

#![forbid(unsafe_code)]

mod analysis;
mod constant;
mod context;
mod diagnostic;
mod expression;
mod outcome;
mod pattern;
mod representation;
mod selection;
mod semantic_context;
mod service;
mod storage;
mod target;
mod type_check;
mod type_representation;
mod unit;

#[cfg(test)]
mod test_support;

pub use constant::{
    ArrayLengthError, CheckedConstantTerms, CheckedConstantTermsBuildError, ConstantCallRequest,
    ConstantCallResolution, ConstantCallResolver, ConstantEvaluationInput,
    ConstantEvaluationLimits, ConstantLiteralError, ConstantReferenceResolution, EvaluatedConstant,
    check_array_length, check_constant_literal, normalize_integer_literal,
    resolve_callable_signature_template, resolve_type_expression_template,
};
pub use context::{
    CheckerFactError, CheckerFactResult, CheckerInfrastructureError, CheckerRequestContext,
    CheckerSemanticFactProvider, CheckerSource,
};
pub use expression::NestedCallableEvidence;
pub use outcome::{CheckerOutcome, ControlFlowCheckResult};
pub use pattern::{
    GuardConstantEvidence, IterationPatternType, PatternCheckInput, PatternConstantEvidence,
};
pub use selection::{
    CallableCandidate, CallableCandidateState, CallableCandidateTemplate,
    CallableCandidateTemplateState, CallableCandidateTemplates,
    CallableDeclarationCandidateTemplate, CallableParameterDefaultTemplate,
    CallableSelectionRequest, CallableValueCandidateTemplate, CandidateAbsence, CandidateSelection,
    CompilerKnownOperationEvidence, ConstructionInputSurface, ExpressionCandidateSet,
    ImplementationSelectionEvidence, IterationSourceCandidate, IterationSourceSelectionRequest,
    OperationCandidate, OperationCandidateSource, OperationCandidateState,
    OperationSelectionRequest, ReceiverCapability, ReceiverSelection, SelectionCandidateKey,
    SelectionFailure,
};
pub use semantic_context::{
    AnonymousCallableContext, ContractClauseContext, DeclaredUnitContext, SemanticUnitContext,
};
pub use service::{
    ConstantChecker, ConstantEvaluator, ControlFlowChecker, DefaultConstantChecker,
    DefaultConstantEvaluator, DefaultControlFlowChecker, DefaultExpressionSemanticChecker,
    DefaultExpressionTypeChecker, DefaultLivenessAnalyzer, DefaultPatternChecker,
    DefaultSemanticSelector, DefaultStoragePlanner, DefaultTargetValidityChecker,
    ExpressionSemanticChecker, ExpressionTypeChecker, LivenessAnalyzer, PatternChecker,
    SemanticSelector, StoragePlanner, TargetValidityChecker,
};
pub use target::{
    TargetAbiValue, TargetCallableAbiRequirement, TargetLayoutRequirement, TargetLayoutUse,
    TargetValidity, TargetValidityContext, TargetValidityRequest, TargetValidityRequirement,
};
pub use type_check::{ExpressionTypeEvidence, ExpressionTypeExpectation, ExpressionTypeInput};
pub use type_representation::{
    DeclaredStorageMember, DeclaredTypeDefinition, DeclaredUnionVariant, RepresentationIntegerType,
    TypeRepresentationContext, check_declared_type_representation,
};
pub use unit::{CheckerUnitRoot, CheckerUnitView, CheckerUnitViewError};
