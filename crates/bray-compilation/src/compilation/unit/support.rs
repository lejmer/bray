use bray_binder::{
    BinderDependency, BinderFactContext, BoundUnitBindingError, BoundUnitComputation,
    bind_anonymous_callable, bind_callable_body, bind_constant_template, bind_constraint,
    bind_contract_clause, bind_embedded_constant, bind_expression_candidates,
    bind_predicate_definition, bind_runtime_default, bind_target_gate, semantic_unit_context,
};
use bray_bound_tree::{
    AnyBoundNodeId, BoundUnit, BoundUnitKey, BoundUnitKind, BoundWalkControl, BoundWalkEvent,
    BoundWalkOutcome, CheckedControlFlow, CheckedExpressionTypes, CheckedMemoryOperations,
    CheckedPatterns, CheckedRefinements, CheckedSemanticSelections,
    DeclaredValueTypeTemplates, Liveness, StoragePlan, walk_bound_unit_view,
};
use bray_checker::{
    CheckerInfrastructureError, CheckerUnitView, ControlFlowChecker, DefaultControlFlowChecker,
    DefaultLivenessAnalyzer, DefaultPatternChecker, DefaultRefinementAnalyzer,
    DefaultStoragePlanner, ExpressionCandidateSet, LivenessAnalyzer, PatternCheckInput,
    PatternChecker, RefinementAnalyzer, SemanticUnitContext, StoragePlanner,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::SymbolGraph;

use crate::compilation::binder::{CompilationBindingContext, binder_fact_error, type_scope};
use crate::compilation::checker::{CompilationCheckerContext, checker_result};
use crate::fact::FactQueryError;

pub(super) fn bind_unit(
    binding_context: &CompilationBindingContext<'_>,
    unit: bray_bound_tree::BoundUnitId,
    key: BoundUnitKey,
) -> Result<BoundUnitComputation, BoundUnitBindingError> {
    match key.kind() {
        BoundUnitKind::CallableBody => bind_callable_body(binding_context, unit, key)?.finish(),
        BoundUnitKind::AnonymousCallable => bind_anonymous_callable(binding_context, unit, key)?.finish(),
        BoundUnitKind::RuntimeDefault => bind_runtime_default(binding_context, unit, key)?.finish(),
        BoundUnitKind::ConstantTemplate => bind_constant_template(binding_context, unit, key)?.finish(),
        BoundUnitKind::EmbeddedConstant => bind_embedded_constant(binding_context, unit, key)?.finish(),
        BoundUnitKind::PredicateDefinition => bind_predicate_definition(binding_context, unit, key)?.finish(),
        BoundUnitKind::Constraint => bind_constraint(binding_context, unit, key)?.finish(),
        BoundUnitKind::ContractClause => bind_contract_clause(binding_context, unit, key)?.finish(),
        BoundUnitKind::TargetGate => bind_target_gate(binding_context, unit, key)?.finish(),
    }
}

pub(in crate::compilation) fn semantic_unit_context_for(
    symbols: &SymbolGraph,
    bound: &BoundUnit,
) -> Result<SemanticUnitContext, FactQueryError> {
    semantic_unit_context(symbols, bound).map_err(FactQueryError::SemanticUnitContext)
}

pub(super) fn check_control_flow(
    bound: &BoundUnit,
    semantic_context: &SemanticUnitContext,
    context: &CompilationCheckerContext<'_>,
) -> Result<
    (
        DiagnosticResult<CheckedControlFlow>,
        Box<[BinderDependency]>,
    ),
    FactQueryError,
> {
    let unit = CheckerUnitView::new(bound, semantic_context, context).map_err(|error| {
        FactQueryError::CheckerInfrastructure(CheckerInfrastructureError::InvalidUnitView(error))
    })?;

    let result = checker_result(DefaultControlFlowChecker.check_control_flow(unit))?
        .map(|result| result.into_control_flow());

    Ok((result, Box::new([])))
}

pub(super) fn expression_candidates(
    binding_context: &CompilationBindingContext<'_>,
    bound: &BoundUnit,
) -> Result<DiagnosticResult<Vec<ExpressionCandidateSet>>, FactQueryError> {
    let owner = binding_context
        .symbols()
        .symbol_for_key(bound.key().declared_owner())
        .ok_or(FactQueryError::InfrastructureFailure)?;

    let type_scope = type_scope(binding_context, owner).map_err(binder_fact_error)?;

    let mut candidates = Vec::new();
    let mut diagnostics = DiagnosticBag::new();
    let mut failure = None;

    let outcome = walk_bound_unit_view(bound.view(), bound.root(), |event| {
        let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) = event else {
            return BoundWalkControl::Continue;
        };

        match bind_expression_candidates(binding_context, bound, expression, &type_scope) {
            Ok(result) => {
                let (candidate, candidate_diagnostics) = result.into_parts();

                if !matches!(candidate, ExpressionCandidateSet::NotApplicable(_)) {
                    candidates.push(candidate);
                }

                diagnostics.add_range(candidate_diagnostics);
            }
            Err(error) => {
                failure = Some(error);

                return BoundWalkControl::Stop;
            }
        }

        BoundWalkControl::Continue
    });

    if let Some(error) = failure {
        return Err(binder_fact_error(error));
    }

    match outcome {
        BoundWalkOutcome::Completed => Ok(DiagnosticResult::new(candidates, diagnostics)),
        BoundWalkOutcome::Stopped | BoundWalkOutcome::MissingNode(_) => {
            Err(FactQueryError::InfrastructureFailure)
        }
    }
}

pub(super) fn check_patterns(
    bound: &BoundUnit,
    semantic_context: &SemanticUnitContext,
    context: &CompilationCheckerContext<'_>,
    types: &CheckedExpressionTypes,
    input: &PatternCheckInput,
) -> Result<DiagnosticResult<CheckedPatterns>, FactQueryError> {
    let unit = CheckerUnitView::new(bound, semantic_context, context).map_err(|error| {
        FactQueryError::CheckerInfrastructure(CheckerInfrastructureError::InvalidUnitView(error))
    })?;

    checker_result(DefaultPatternChecker.check_patterns(unit, types, input))
}

pub(super) fn plan_storage(
    bound: &BoundUnit,
    semantic_context: &SemanticUnitContext,
    context: &CompilationCheckerContext<'_>,
    declared_types: &DeclaredValueTypeTemplates,
    types: &CheckedExpressionTypes,
    patterns: &CheckedPatterns,
    selections: &CheckedSemanticSelections,
) -> Result<DiagnosticResult<StoragePlan>, FactQueryError> {
    let unit = CheckerUnitView::new(bound, semantic_context, context).map_err(|error| {
        FactQueryError::CheckerInfrastructure(CheckerInfrastructureError::InvalidUnitView(error))
    })?;

    checker_result(DefaultStoragePlanner.plan_storage(
        unit,
        declared_types,
        types,
        patterns,
        selections,
    ))
}

pub(super) fn analyze_liveness(
    bound: &BoundUnit,
    semantic_context: &SemanticUnitContext,
    context: &CompilationCheckerContext<'_>,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
    memory: &CheckedMemoryOperations,
) -> Result<DiagnosticResult<Liveness>, FactQueryError> {
    let unit = CheckerUnitView::new(bound, semantic_context, context).map_err(|error| {
        FactQueryError::CheckerInfrastructure(CheckerInfrastructureError::InvalidUnitView(error))
    })?;

    checker_result(DefaultLivenessAnalyzer.analyze_liveness(unit, selections, storage, memory))
}

pub(super) fn analyze_refinements(
    bound: &BoundUnit,
    semantic_context: &SemanticUnitContext,
    context: &CompilationCheckerContext<'_>,
    patterns: &CheckedPatterns,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
) -> Result<DiagnosticResult<CheckedRefinements>, FactQueryError> {
    let unit = CheckerUnitView::new(bound, semantic_context, context).map_err(|error| {
        FactQueryError::CheckerInfrastructure(CheckerInfrastructureError::InvalidUnitView(error))
    })?;

    checker_result(
        DefaultRefinementAnalyzer.analyze_refinements(unit, patterns, selections, storage),
    )
}

pub(super) const fn map_binding_error(error: BoundUnitBindingError) -> FactQueryError {
    match error {
        BoundUnitBindingError::Cancelled => FactQueryError::Cancelled,
        BoundUnitBindingError::InvalidUnitKey
        | BoundUnitBindingError::MissingSyntax
        | BoundUnitBindingError::MissingOwner
        | BoundUnitBindingError::MissingModule
        | BoundUnitBindingError::SemanticValue(_)
        | BoundUnitBindingError::Construction
        | BoundUnitBindingError::Binding
        | BoundUnitBindingError::Assembly => FactQueryError::InfrastructureFailure,
    }
}
