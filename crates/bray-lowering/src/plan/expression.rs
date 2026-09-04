use std::collections::BTreeMap;

use bray_bound_tree::{
    AsyncSuspensionKind, AsyncTaskOperationKind, BoundExpression, BoundExpressionId, BoundUnit,
    CheckedAsync, CheckedDependencyContracts, CheckedSemanticSelections, SemanticSelection,
    StoragePlan,
};
use bray_compiler_known::ImplementationHook;
use bray_symbols::AvailableCompilerKnownSymbols;

use super::verify::dependency_subject_exists;
use super::{LoweringPlanFailure, LoweringPlanFailureCause, LoweringPlanKind};

pub(super) fn verify_suspensions(
    unit: &BoundUnit,
    storage: &StoragePlan,
    dependencies: &CheckedDependencyContracts,
    selections: &CheckedSemanticSelections,
    symbols: &AvailableCompilerKnownSymbols,
    analysis: &CheckedAsync,
) -> Result<BTreeMap<BoundExpressionId, usize>, LoweringPlanFailure> {
    let mut expected = BTreeMap::new();

    for (expression, node) in unit.tree().expressions() {
        if let BoundExpression::Await(await_expression) = node {
            expected.insert(
                expression,
                AsyncSuspensionKind::Await {
                    operand: await_expression.operand(),
                },
            );
        }
    }

    for entry in selections.entries() {
        if selected_hook(entry.selection(), symbols).is_some_and(|hook| {
            matches!(
                hook,
                ImplementationHook::TaskYield | ImplementationHook::TaskEventWait
            )
        }) {
            expected.insert(entry.expression(), AsyncSuspensionKind::Yield);
        }
    }

    let mut verified = BTreeMap::new();

    for (index, suspension) in analysis.suspensions().iter().enumerate() {
        let expression = suspension.expression();

        if verified.insert(expression, index).is_some() {
            return Err(LoweringPlanFailure::for_expression(
                LoweringPlanKind::Suspension,
                LoweringPlanFailureCause::Duplicate,
                expression,
            ));
        }

        let Some(kind) = expected.remove(&expression) else {
            return Err(LoweringPlanFailure::for_expression(
                LoweringPlanKind::Suspension,
                LoweringPlanFailureCause::Unexpected,
                expression,
            ));
        };

        if suspension.is_recovered() {
            return Err(LoweringPlanFailure::for_expression(
                LoweringPlanKind::Suspension,
                LoweringPlanFailureCause::Recovered,
                expression,
            ));
        }

        if suspension.kind() != kind
            || suspension
                .dependency_contract()
                .is_some_and(|contract| dependencies.contract(contract).is_none())
            || !suspension
                .retained_subjects()
                .iter()
                .all(|subject| dependency_subject_exists(unit, storage, *subject))
        {
            return Err(LoweringPlanFailure::for_expression(
                LoweringPlanKind::Suspension,
                LoweringPlanFailureCause::Contradictory,
                expression,
            ));
        }
    }

    if let Some((&expression, _)) = expected.first_key_value() {
        return Err(LoweringPlanFailure::for_expression(
            LoweringPlanKind::Suspension,
            LoweringPlanFailureCause::Missing,
            expression,
        ));
    }

    Ok(verified)
}

pub(super) fn verify_task_operations(
    unit: &BoundUnit,
    selections: &CheckedSemanticSelections,
    symbols: &AvailableCompilerKnownSymbols,
    analysis: &CheckedAsync,
) -> Result<BTreeMap<BoundExpressionId, AsyncTaskOperationKind>, LoweringPlanFailure> {
    let mut expected = selections
        .entries()
        .iter()
        .filter_map(|entry| {
            selected_task_operation(entry.selection(), symbols)
                .map(|kind| (entry.expression(), kind))
        })
        .collect::<BTreeMap<_, _>>();

    let mut verified = BTreeMap::new();

    for operation in analysis.task_operations() {
        let expression = operation.expression();

        if unit.view().expression(expression).is_none() {
            return Err(LoweringPlanFailure::for_expression(
                LoweringPlanKind::TaskOperation,
                LoweringPlanFailureCause::Unexpected,
                expression,
            ));
        }

        if verified.insert(expression, operation.kind()).is_some() {
            return Err(LoweringPlanFailure::for_expression(
                LoweringPlanKind::TaskOperation,
                LoweringPlanFailureCause::Duplicate,
                expression,
            ));
        }

        if expected.remove(&expression) != Some(operation.kind()) {
            return Err(LoweringPlanFailure::for_expression(
                LoweringPlanKind::TaskOperation,
                LoweringPlanFailureCause::Contradictory,
                expression,
            ));
        }
    }

    if let Some((&expression, _)) = expected.first_key_value() {
        return Err(LoweringPlanFailure::for_expression(
            LoweringPlanKind::TaskOperation,
            LoweringPlanFailureCause::Missing,
            expression,
        ));
    }

    Ok(verified)
}

fn selected_task_operation(
    selection: &SemanticSelection,
    symbols: &AvailableCompilerKnownSymbols,
) -> Option<AsyncTaskOperationKind> {
    match selected_hook(selection, symbols)? {
        ImplementationHook::FutureStart => Some(AsyncTaskOperationKind::Start),
        ImplementationHook::TaskJoin => Some(AsyncTaskOperationKind::Join),
        ImplementationHook::TaskCancel => Some(AsyncTaskOperationKind::Cancel),
        _ => None,
    }
}

fn selected_hook(
    selection: &SemanticSelection,
    symbols: &AvailableCompilerKnownSymbols,
) -> Option<ImplementationHook> {
    let SemanticSelection::Call(call) = selection else {
        return None;
    };

    call.implementation_hook().or_else(|| {
        let target = call.target().declaration()?;

        symbols.symbol_implementation(target.symbol())
    })
}
