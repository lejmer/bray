//! Semantic checking for bound Bray programs.

#![forbid(unsafe_code)]

mod analysis;
mod asynchronous;
mod atomic;
mod behavior;
mod body_semantics;
mod constant;
mod context;
mod contract;
mod contract_implication;
mod dependency;
mod diagnostic;
mod execution;
mod expression;
mod finalization;
mod guarantee;
mod memory;
mod memory_diagnostics;
mod outcome;
mod pattern;
mod predicate;
mod proof_dependency;
mod representation;
mod selection;
mod semantic_context;
mod service;
mod storage;
mod target;
mod target_control;
mod target_control_contract;
mod target_control_symbol;
mod type_check;
mod type_normalization;
mod type_representation;
mod unit;

#[cfg(test)]
mod test_support;

pub use analysis::{closed_type_is_copyable, type_is_copyable, type_is_copyable_in_context};
pub use asynchronous::{
    CleanupExecutionStep, cleanup_type_execution, cleanup_type_execution_with,
    owned_cleanup_type_dependencies,
};
pub use atomic::atomic_target_representation;
pub use constant::{
    ArrayLengthError, CheckedConstantTerms, CheckedConstantTermsBuildError, ConstantCallRequest,
    ConstantCallResolution, ConstantCallResolver, ConstantEvaluationInput,
    ConstantEvaluationLimits, ConstantEvaluationUsage, ConstantLiteralError,
    ConstantReferenceResolution, ConstantTemplateResolver, EvaluatedConstant,
    EvaluatedConstantCall, check_array_length, check_constant_literal,
    evaluate_constant_callable_template, evaluate_constant_definition_template,
    evaluate_generic_constraint_template, evaluate_static_initializer_template,
    normalize_integer_literal, resolve_callable_signature_template,
    resolve_trait_application_template, resolve_type_expression_template,
};
pub use context::{
    CheckerConstantEvaluationFailure, CheckerConstantInputFailure, CheckerConstantOperationFailure,
    CheckerInfrastructureError, CheckerInputKind, CheckerLiteralValueFailure,
    CheckerPatternInputFailure, CheckerQueryError, CheckerQueryResult, CheckerRequestContext,
    CheckerSemanticQueryProvider, CheckerSource, CheckerStorageFlowFailure,
    ImplementationHookResolution, StorageFlowInputKind,
};
pub use contract::{instantiate_condition, prove_condition};
pub use contract_implication::{
    CallableConditionMismatch, CallableTypeContractMismatch, callable_type_contract_is_compatible,
    check_callable_condition_implication, check_callable_type_contract,
};
pub use diagnostic::diagnostic_type;
pub use execution::{
    ExecutionCallArgument, ExecutionCallInput, ExecutionGuaranteeInput, ExecutionLifecycleInput,
    check_execution_property,
};
pub use expression::{NestedCallableEvidence, check_generic_arguments};
pub use finalization::prove_finalizer_completion;
pub use guarantee::{
    check_execution_guarantee_implication, compiler_projection_guarantees,
    contract_guard_conditions, storage_projection_proof_dependencies,
};
pub use outcome::{CheckerOutcome, ControlFlowCheckResult};
pub use pattern::{
    GuardConstantEvidence, IterationPatternType, PatternCheckInput, PatternConstantEvidence,
};
pub use predicate::{PredicateConditionResolution, expand_predicate_condition};
pub use proof_dependency::{CallableProofFailure, check_callable_proof_dependencies};
pub use selection::{
    CallableCandidate, CallableCandidateState, CallableCandidateTemplate,
    CallableCandidateTemplateState, CallableCandidateTemplates,
    CallableDeclarationCandidateTemplate, CallableParameterDefaultTemplate,
    CallableSelectionRequest, CallableValueCandidateTemplate, CandidateAbsence, CandidateSelection,
    CompilerKnownOperationEvidence, ConstructionInputSurface, ExpressionCandidateSet,
    ImplementationSelectionEvidence, IterationSourceCandidate, IterationSourceSelectionRequest,
    OperationCandidate, OperationCandidateSource, OperationCandidateState,
    OperationSelectionRequest, PredicateCandidateTemplate, ReceiverCapability, ReceiverSelection,
    SelectionCandidateKey, SelectionCandidateSignature, SelectionFailure,
    SelectionFailureCandidate, SelectionInaccessibility, built_in_conversion_plan,
    built_in_conversion_plan_for_context, built_in_operation_result_type,
    built_in_operator_supported, built_in_trait_constraint_outcome, compiler_known_operation_role,
    composite_conversion_children,
};
pub use semantic_context::{
    AnonymousCallableContext, ContractClauseContext, DeclaredUnitContext, SemanticUnitContext,
};
pub use service::{
    AsyncChecker, BodyBehaviorCollector, BodySemanticChecker, ConstantChecker, ConstantEvaluator,
    ControlFlowChecker, DefaultAsyncChecker, DefaultBodyBehaviorCollector,
    DefaultBodySemanticChecker, DefaultConstantChecker, DefaultConstantEvaluator,
    DefaultControlFlowChecker, DefaultDependencyContractChecker, DefaultExpressionSemanticChecker,
    DefaultExpressionTypeChecker, DefaultLivenessAnalyzer, DefaultMemoryOperationChecker,
    DefaultPatternChecker, DefaultRefinementAnalyzer, DefaultSemanticSelector,
    DefaultStorageFlowChecker, DefaultStoragePlanner, DefaultTargetValidityChecker,
    DependencyContractChecker, ExpressionSemanticChecker, ExpressionTypeChecker, LivenessAnalyzer,
    MemoryOperationChecker, PatternChecker, RefinementAnalyzer, SemanticSelector,
    StorageFlowChecker, StoragePlanner, TargetValidityChecker,
};
pub use target::{
    TargetAbiValue, TargetAggregateAbi, TargetCallableAbiRequirement, TargetLayoutRequirement,
    TargetLayoutUse, TargetValidity, TargetValidityContext, TargetValidityRequest,
    TargetValidityRequirement,
};
pub use type_check::{ExpressionTypeEvidence, ExpressionTypeExpectation, ExpressionTypeInput};
pub use type_normalization::{
    normalize_callable_signature_type_valued_members, normalize_type_valued_members,
};
pub use type_representation::{
    DeclaredStorageMember, DeclaredStorageMemberIdentity, DeclaredTypeDefinition,
    DeclaredUnionVariant, RepresentationIntegerType, TypeRepresentationContext,
    check_declared_type_representation,
};
pub use unit::{CheckerUnitRoot, CheckerUnitView, CheckerUnitViewError};
