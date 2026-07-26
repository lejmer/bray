//! Semantic checking for bound Bray programs.

#![forbid(unsafe_code)]

mod analysis;
mod asynchronous;
mod behavior;
mod constant;
mod context;
mod dependency;
mod diagnostic;
mod expression;
mod limits;
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
pub use diagnostic::diagnostic_type;
pub use expression::NestedCallableEvidence;
pub use limits::SemanticAnalysisLimits;
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
    AsyncChecker, BodyBehaviorCollector, ConstantChecker, ConstantEvaluator, ControlFlowChecker,
    DefaultAsyncChecker, DefaultBodyBehaviorCollector, DefaultConstantChecker,
    DefaultConstantEvaluator, DefaultControlFlowChecker, DefaultDependencyContractChecker,
    DefaultExpressionSemanticChecker, DefaultExpressionTypeChecker, DefaultLivenessAnalyzer,
    DefaultPatternChecker, DefaultRefinementAnalyzer, DefaultSemanticSelector,
    DefaultStorageFlowChecker, DefaultStoragePlanner, DefaultTargetValidityChecker,
    DependencyContractChecker, ExpressionSemanticChecker, ExpressionTypeChecker, LivenessAnalyzer,
    PatternChecker, RefinementAnalyzer, SemanticSelector, StorageFlowChecker, StoragePlanner,
    TargetValidityChecker,
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
