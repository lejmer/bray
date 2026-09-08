use bray_bound_tree::{AnyBoundNodeId, BoundControlTransferKind, BoundExpression};
use bray_symbols::{CallableContractClause, ProofOutcome};

use crate::contract::prove_condition;
use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

use super::super::fixed_point::{FixedPointDomain, FixedPointResult};
use super::super::model::{AnalysisExit, AnalysisExitKind};
use super::flow::{DomainState, GuaranteeDomain};

pub(super) fn first_postcondition_failure<C>(
    request: CheckerUnitView<'_, C>,
    domain: &GuaranteeDomain<'_>,
    result: &FixedPointResult<Result<DomainState, CheckerInfrastructureError>>,
    clause: CallableContractClause,
) -> Result<Option<AnalysisExit>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    for exit in domain.graph.exits() {
        if !matches!(
            exit.kind(),
            AnalysisExitKind::NormalFallthrough
                | AnalysisExitKind::Return
                | AnalysisExitKind::ResultErrorPropagation
                | AnalysisExitKind::Yield
        ) {
            continue;
        }

        let Some(state) = result.state(exit.block()) else {
            return Ok(Some(*exit));
        };

        let Some(block) = domain.graph.block(exit.block()) else {
            return Ok(Some(*exit));
        };

        let state = domain.transfer(block, state)?;

        if !state.reachable {
            continue;
        }

        let Some(condition) = clause
            .predicate()
            .and_then(|predicate| predicate.condition())
        else {
            return Ok(Some(*exit));
        };

        let returned = if exit.kind() == AnalysisExitKind::ResultErrorPropagation {
            match exit.origin() {
                Some(AnyBoundNodeId::Expression(expression)) => {
                    domain.propagated_return(&state, expression)?
                }
                _ => None,
            }
        } else {
            match returned_expression(request, *exit) {
                Some(returned) => domain.current_expression(&state, returned)?,
                None => None,
            }
        };

        let Some(condition) = domain.current_observation(&state, condition, returned, None)? else {
            return Ok(Some(*exit));
        };

        let conditions = state.conditions.iter().copied().collect::<Vec<_>>();

        if prove_condition(domain.values, &conditions, condition)? != ProofOutcome::Proven {
            return Ok(Some(*exit));
        }
    }

    Ok(None)
}

fn returned_expression<C>(
    request: CheckerUnitView<'_, C>,
    exit: AnalysisExit,
) -> Option<bray_bound_tree::BoundExpressionId>
where
    C: CheckerRequestContext + ?Sized,
{
    let AnyBoundNodeId::Expression(expression) = exit.origin()? else {
        return None;
    };

    let BoundExpression::ControlTransfer(transfer) = request.view().expression(expression)? else {
        return None;
    };

    (exit.kind() == AnalysisExitKind::Return && transfer.kind() == BoundControlTransferKind::Return)
        .then(|| transfer.operand())
        .flatten()
}
