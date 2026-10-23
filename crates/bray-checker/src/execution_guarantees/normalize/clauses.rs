use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundUnit, BoundUnitRoot, CheckedExpressionSemantics,
};
use bray_symbols::SemanticValueStore;

use crate::ExecutionCondition;

use super::{condition_literals, expression_condition};

/// Normalizes checked contract predicates while preserving unknown meaning.
pub fn execution_conditions(
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    patterns: &bray_bound_tree::CheckedPatterns,
    values: &SemanticValueStore,
) -> Result<Vec<ExecutionCondition>, bray_symbols::SemanticValueStoreError> {
    Ok(predicate_conditions(unit, semantics, patterns, values)?
        .into_iter()
        .map(|(_, condition, _)| condition)
        .collect())
}

/// Normalizes predicate meaning and preserves explicit trusted contract requirements.
pub fn predicate_conditions(
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    patterns: &bray_bound_tree::CheckedPatterns,
    values: &SemanticValueStore,
) -> Result<
    Vec<(
        BoundExpressionId,
        ExecutionCondition,
        bray_symbols::PredicateDomain,
    )>,
    bray_symbols::SemanticValueStoreError,
> {
    predicate_conditions_with_inputs(
        unit,
        semantics,
        patterns,
        values,
        &BTreeMap::new(),
        None,
        None,
    )
}

/// Normalizes contract type clauses using exact declaration-local input placeholders.
pub fn predicate_conditions_with_inputs(
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    patterns: &bray_bound_tree::CheckedPatterns,
    values: &SemanticValueStore,
    inputs: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    storage: Option<(&bray_bound_tree::StoragePlan, &bray_bound_tree::StorageFlow)>,
    predicates: Option<
        &BTreeMap<bray_symbols::PredicateInstanceData, bray_symbols::PredicateSemanticSummary>,
    >,
) -> Result<
    Vec<(
        BoundExpressionId,
        ExecutionCondition,
        bray_symbols::PredicateDomain,
    )>,
    bray_symbols::SemanticValueStoreError,
> {
    let roots = match unit.root() {
        BoundUnitRoot::Expression(expression) => vec![expression],
        BoundUnitRoot::ExpressionSequence(block) => unit
            .view()
            .block(block)
            .map(|block| {
                block
                    .items()
                    .iter()
                    .filter_map(|item| item.expression())
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    };

    let literals = condition_literals(semantics, values);
    let patterns = super::pattern_conditions(unit, patterns, values);

    let references = storage.map_or_else(BTreeMap::new, |(plan, flow)| {
        reference_conditions(plan, flow)
    });

    let mut pending = roots
        .into_iter()
        .rev()
        .map(|root| (root, false))
        .collect::<Vec<_>>();

    let mut conditions = Vec::new();

    while let Some((root, trusted)) = pending.pop() {
        match unit.view().expression(root) {
            Some(BoundExpression::Structured(structured))
                if matches!(
                    structured.kind(),
                    bray_bound_tree::BoundStructuredExpressionKind::TrustBoundary
                        | bray_bound_tree::BoundStructuredExpressionKind::Condition
                ) =>
            {
                if let [operand] = structured.operands() {
                    pending.push((
                        *operand,
                        trusted
                            || structured.kind()
                                == bray_bound_tree::BoundStructuredExpressionKind::TrustBoundary,
                    ));

                    continue;
                }
            }
            _ => {}
        }

        let mut condition = expression_condition(
            unit,
            semantics,
            values,
            &literals,
            &patterns,
            inputs,
            &references,
            root,
            &mut { crate::ExecutionCondition::WORK_LIMIT },
            predicates,
        );

        let domain = predicate_domain(unit, semantics, root, predicates).max(if trusted {
            bray_symbols::PredicateDomain::Trusted
        } else {
            bray_symbols::PredicateDomain::Value
        });

        if trusted {
            condition = condition.requiring_trusted_predicates();
        }

        conditions.push((root, condition, domain));
    }

    Ok(conditions)
}

fn predicate_domain(
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    root: BoundExpressionId,
    predicates: Option<
        &BTreeMap<bray_symbols::PredicateInstanceData, bray_symbols::PredicateSemanticSummary>,
    >,
) -> bray_symbols::PredicateDomain {
    use bray_bound_tree::{AnyBoundNodeId, BoundWalkControl, BoundWalkEvent, SemanticSelection};
    use bray_symbols::PredicateDomain;

    let mut domain = PredicateDomain::Value;

    let outcome = bray_bound_tree::walk_bound_unit_view(unit.view(), root, |event| {
        let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) = event else {
            return BoundWalkControl::Continue;
        };

        if matches!(unit.view().expression(expression), Some(BoundExpression::Structured(operation))
            if operation.kind() == bray_bound_tree::BoundStructuredExpressionKind::TrustBoundary)
        {
            domain = domain.max(PredicateDomain::Trusted);
        }

        let predicate = match semantics.selections().expression(expression) {
            Some(SemanticSelection::Call(call)) => {
                if call
                    .implementation_hook()
                    .is_some_and(bray_compiler_known::ImplementationHook::is_execution_requirement)
                {
                    domain = PredicateDomain::Execution;

                    return BoundWalkControl::Stop;
                }

                match call.target() {
                    bray_bound_tree::BoundCallableTarget::Predicate(predicate) => Some(predicate),
                    _ => None,
                }
            }
            Some(SemanticSelection::Predicate(predicate)) => {
                Some(bray_symbols::PredicateInstanceData::new(
                    predicate.predicate(),
                    predicate.substitution(),
                ))
            }
            _ => None,
        };

        if let Some(semantic) = predicate
            .and_then(|predicate| predicates.and_then(|predicates| predicates.get(&predicate)))
        {
            domain = domain.max(semantic.domain());
        }

        if domain == PredicateDomain::Execution {
            BoundWalkControl::Stop
        } else {
            BoundWalkControl::Continue
        }
    });

    assert!(
        matches!(
            outcome,
            bray_bound_tree::BoundWalkOutcome::Completed
                | bray_bound_tree::BoundWalkOutcome::Stopped
        ),
        "checked predicate domain retains its complete source graph: {outcome:?}"
    );

    domain
}

// Use the owning storage analysis's exact normal-completion relation. This does not
// evaluate a const body or equate equal contents with a borrowed storage identity.
fn reference_conditions(
    storage: &bray_bound_tree::StoragePlan,
    flow: &bray_bound_tree::StorageFlow,
) -> BTreeMap<BoundExpressionId, ExecutionCondition> {
    let mut references = BTreeMap::new();

    for (identity, kind) in storage.identity_entries() {
        let bray_bound_tree::StorageIdentity::Temporary(expression) = kind else {
            continue;
        };

        let Some(completion) = flow.completion(bray_bound_tree::StorageMutation::Operation(
            expression.into(),
        )) else {
            continue;
        };

        let path = bray_bound_tree::StoragePath {
            root: identity,
            projections: [].into(),
        };

        let Some(target) = completion.references().get(&path) else {
            continue;
        };

        let Some(place) = crate::ExecutionPlace::storage_path(storage, target) else {
            continue;
        };

        let mut condition = match place.reference() {
            Some(bray_bound_tree::BoundReferenceTarget::Local(
                bray_symbols::AnyLocalSymbolId::PostconditionResult(_),
            )) => ExecutionCondition::Result,
            Some(reference) => ExecutionCondition::Input(reference.into()),
            None => continue,
        };

        for projection in place.projections.iter() {
            condition = condition.component(*projection);
        }

        references.insert(expression, condition);
    }

    references
}
