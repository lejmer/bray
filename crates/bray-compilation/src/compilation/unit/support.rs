use bray_binder::{
    BinderDependency, BindingQueryContext, BoundUnitBindingError, BoundUnitComputation,
    bind_anonymous_callable, bind_callable_body, bind_constant_template, bind_constraint,
    bind_contract_clause, bind_embedded_constant, bind_expression_candidates,
    bind_predicate_definition, bind_runtime_default, bind_target_gate,
};
use bray_bound_tree::{
    AnyBoundNodeId, BoundUnit, BoundUnitKey, BoundUnitKind, BoundWalkControl, BoundWalkEvent,
    BoundWalkOutcome, CheckedControlFlow, CheckedExpressionTypes, CheckedPatterns,
    CheckedSemanticSelections, DeclaredValueTypeTemplates, StoragePlan, walk_bound_unit_view,
};
use bray_checker::{
    ControlFlowChecker, DefaultControlFlowChecker, DefaultPatternChecker, DefaultStoragePlanner,
    ExpressionCandidateSet, PatternCheckInput, PatternChecker, SemanticUnitContext, StoragePlanner,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

use crate::compilation::binder::{
    CompilationBindingContext, binding_error, binding_query_error, type_scope,
};
use crate::compilation::checker::{CompilationCheckerContext, checker_result};
use crate::fact::FactQueryError;

pub(super) fn bind_unit(
    binding_context: &CompilationBindingContext<'_>,
    unit: bray_bound_tree::BoundUnitId,
    key: BoundUnitKey,
) -> Result<BoundUnitComputation, BoundUnitBindingError<FactQueryError>> {
    match key.kind() {
        BoundUnitKind::CallableBody => bind_callable_body(binding_context, unit, key),
        BoundUnitKind::AnonymousCallable => bind_anonymous_callable(binding_context, unit, key),
        BoundUnitKind::RuntimeDefault => bind_runtime_default(binding_context, unit, key),
        BoundUnitKind::ConstantTemplate => bind_constant_template(binding_context, unit, key),
        BoundUnitKind::EmbeddedConstant => bind_embedded_constant(binding_context, unit, key),
        BoundUnitKind::PredicateDefinition => bind_predicate_definition(binding_context, unit, key),
        BoundUnitKind::Constraint => bind_constraint(binding_context, unit, key),
        BoundUnitKind::ContractClause => bind_contract_clause(binding_context, unit, key),
        BoundUnitKind::TargetGate => bind_target_gate(binding_context, unit, key),
    }
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
    let unit = bray_checker::CheckerUnitView::new(bound, semantic_context, context);

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
        .ok_or_else(|| {
            unit_contract_failure(
                bound.key(),
                crate::compilation::SemanticQueryViolation::Missing(
                    crate::compilation::SemanticDataKind::Symbol,
                ),
            )
        })?;

    let type_scope = type_scope(binding_context, owner).map_err(binding_query_error)?;

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
        return Err(binding_query_error(error));
    }

    if outcome != BoundWalkOutcome::Completed {
        return Err(unit_walk_failure(bound.key(), outcome));
    }

    Ok(DiagnosticResult::new(candidates, diagnostics))
}

pub(super) fn unit_walk_failure(key: &BoundUnitKey, outcome: BoundWalkOutcome) -> FactQueryError {
    let violation = match outcome {
        BoundWalkOutcome::MissingNode(node) => {
            crate::compilation::SemanticQueryViolation::MissingBoundNode(node)
        }
        BoundWalkOutcome::Stopped | BoundWalkOutcome::Completed => {
            crate::compilation::SemanticQueryViolation::UnexpectedWalkOutcome(outcome)
        }
    };

    unit_contract_failure(key, violation)
}

pub(super) fn unit_contract_failure(
    key: &BoundUnitKey,
    violation: crate::compilation::SemanticQueryViolation,
) -> FactQueryError {
    crate::compilation::SemanticQueryFailure::contract(
        crate::compilation::SemanticQueryContext::Unit(key.clone()),
        violation,
    )
    .into()
}

pub(super) fn check_patterns(
    bound: &BoundUnit,
    semantic_context: &SemanticUnitContext,
    context: &CompilationCheckerContext<'_>,
    types: &CheckedExpressionTypes,
    input: &PatternCheckInput,
) -> Result<DiagnosticResult<CheckedPatterns>, FactQueryError> {
    let unit = bray_checker::CheckerUnitView::new(bound, semantic_context, context);

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
    let unit = bray_checker::CheckerUnitView::new(bound, semantic_context, context);

    checker_result(DefaultStoragePlanner.plan_storage(
        unit,
        declared_types,
        types,
        patterns,
        selections,
    ))
}

pub(super) fn map_binding_error(error: BoundUnitBindingError<FactQueryError>) -> FactQueryError {
    match error {
        BoundUnitBindingError::Cancelled => FactQueryError::Cancelled,
        BoundUnitBindingError::CheckerInfrastructure(error) => {
            FactQueryError::CheckerInfrastructure(error)
        }
        BoundUnitBindingError::Upstream(error) => error,
        BoundUnitBindingError::InvalidUnitKey => {
            FactQueryError::Binding(BoundUnitBindingError::InvalidUnitKey)
        }
        BoundUnitBindingError::MissingSyntax { source } => {
            FactQueryError::Binding(BoundUnitBindingError::MissingSyntax { source })
        }
        BoundUnitBindingError::MissingOwner {
            source,
            owner,
            symbol,
        } => FactQueryError::Binding(BoundUnitBindingError::MissingOwner {
            source,
            owner,
            symbol,
        }),
        BoundUnitBindingError::MissingModule { source, owner } => {
            FactQueryError::Binding(BoundUnitBindingError::MissingModule { source, owner })
        }
        BoundUnitBindingError::InvalidSurfaceName { source, symbol } => {
            FactQueryError::Binding(BoundUnitBindingError::InvalidSurfaceName { source, symbol })
        }
        BoundUnitBindingError::SemanticValue(error) => {
            FactQueryError::Binding(BoundUnitBindingError::SemanticValue(error))
        }
        BoundUnitBindingError::Construction(error) => {
            FactQueryError::Binding(BoundUnitBindingError::Construction(error))
        }
        BoundUnitBindingError::Binding(error) => binding_error(error),
    }
}
