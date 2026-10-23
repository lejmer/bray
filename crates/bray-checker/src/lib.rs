//! Semantic checking for bound Bray programs.

#![forbid(unsafe_code)]

mod analysis;
mod asynchronous;
mod atomic;
mod behavior;
mod body_semantics;
mod constant;
mod context;
mod dependency;
mod diagnostic;
mod execution_guarantees;
mod expression;
mod lifecycle;
mod memory;
mod memory_diagnostics;
mod numeric;
mod outcome;
pub use numeric::{normalize_numeric_phase_requirements, normalize_numeric_requirements};
mod pattern;
mod representation;
pub use representation::{integer_conversion_representations, representation_type_for_context};
mod selection;
mod semantic_context;
mod service;
mod storage;
pub use storage::projected_storage_type;
mod target;
mod target_control;
mod target_control_contract;
mod target_control_symbol;
mod trusted_contracts;
mod type_check;
mod type_normalization;
mod type_representation;
mod unit;

pub use trusted_contracts::{TrustedCallContract, TrustedContractInputs};

#[cfg(test)]
mod test_support;

pub use analysis::implementation_hook_may_propagate_synchronous_panic;
pub use analysis::{PreparedBodyAnalysis, prepare_body_analysis};
pub use analysis::{
    UnwindCall, UnwindCandidate, check_execution_candidate, check_unwind_candidate,
};
pub use analysis::{closed_type_is_copyable, type_is_copyable, type_is_copyable_in_context};
pub use atomic::atomic_target_representation;
pub use constant::{
    ArrayLengthError, CheckedConstantTerms, CheckedConstantTermsBuildError, ConstantCallContract,
    ConstantCallRequest, ConstantCallResolution, ConstantCallResolver, ConstantCallTarget,
    ConstantEvaluationInput, ConstantEvaluationLimits, ConstantEvaluationUsage,
    ConstantLiteralError, ConstantReferenceResolution, ConstantTemplateResolver, EvaluatedConstant,
    EvaluatedConstantCall, check_array_length, check_constant_literal,
    constant_eligibility_diagnostic, constant_invocation_targets, evaluate_array_length,
    evaluate_constant_call, evaluate_constant_definition_template, evaluate_constant_intrinsic,
    evaluate_generic_constraint_template, evaluate_static_initializer_template,
    fold_machine_integer_binary, fold_machine_integer_truncate, fold_machine_integer_unary,
    normalize_integer_literal, resolve_callable_signature_template,
    resolve_trait_application_template, resolve_type_expression_template,
};
pub use context::{
    CheckerQueryError, CheckerQueryResult, CheckerRequestContext, CheckerSemanticQueryProvider,
    CheckerSource, ImplementationHookResolution,
};
pub use diagnostic::diagnostic_type;
pub use execution_guarantees::{
    DeclaredExecutionProperty, ExecutionCallEvidence, ExecutionCandidate, ExecutionCandidates,
    ExecutionCertification, ExecutionClauseId, ExecutionCleanup, ExecutionCompletionContract,
    ExecutionCompletionDependency, ExecutionCondition, ExecutionDeclaration, ExecutionDependency,
    ExecutionDependencyTarget, ExecutionDomain, ExecutionObligation, ExecutionPlace,
    ExecutionProperty, check_execution_guarantees, declared_execution_properties,
    execution_condition_from_term, execution_condition_from_type_term,
    execution_condition_is_implied, execution_condition_term, execution_conditions,
    map_execution_condition_references, predicate_conditions, predicate_conditions_with_inputs,
    remap_execution_condition_inputs, substitute_execution_condition,
};
pub use expression::{
    NestedCallableEvidence, call_result, check_generic_arguments, generic_constraint_outcome,
};
pub use lifecycle::{
    FinalizerCompletion, LifecycleSelectionContext, check_implicit_finalization,
    invocation_execution_requirements, select_lifecycle_action,
};
pub use outcome::{CheckerOutcome, ControlFlowCheckResult};
pub use pattern::{
    GuardConstantEvidence, PatternCheckInput, PatternConstantEvidence, PatternSubjectType,
};
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
    built_in_operator_supported, built_in_trait_constraint_outcome,
    callable_contract_conversion_is_valid, compiler_known_operation_role,
    composite_conversion_children,
};
pub use semantic_context::{
    AnonymousCallableContext, ContractClauseContext, DeclaredUnitContext, SemanticUnitContext,
};
pub use service::{
    BodyBehaviorCollector, BodySemanticChecker, ConstantChecker, ConstantEvaluator,
    ControlFlowChecker, DefaultBodyBehaviorCollector, DefaultBodySemanticChecker,
    DefaultConstantChecker, DefaultConstantEvaluator, DefaultControlFlowChecker,
    DefaultExpressionSemanticChecker, DefaultExpressionTypeChecker, DefaultMemoryOperationChecker,
    DefaultPatternChecker, DefaultSemanticSelector, DefaultStoragePlanner,
    DefaultTargetValidityChecker, ExpressionSemanticChecker, ExpressionTypeChecker,
    MemoryOperationChecker, PatternChecker, SemanticSelector, StoragePlanner,
    TargetValidityChecker,
};
pub use target::{
    TargetAbiValue, TargetAggregateAbi, TargetCallableAbiRequirement, TargetLayoutRequirement,
    TargetLayoutUse, TargetValidity, TargetValidityContext, TargetValidityRequest,
    TargetValidityRequirement,
};
pub use type_check::{ExpressionTypeEvidence, ExpressionTypeExpectation, ExpressionTypeInput};
pub use type_normalization::{
    TypeNormalizer, normalize_callable_signature, normalize_type_valued_members,
};
pub use type_representation::{
    DeclaredStorageMember, DeclaredStorageMemberIdentity, DeclaredTypeDefinition,
    DeclaredUnionVariant, RepresentationIntegerType, TypeRepresentationContext,
    check_declared_type_representation,
};
pub use unit::{CheckerUnitRoot, CheckerUnitView, ResultDependencyOwner};
