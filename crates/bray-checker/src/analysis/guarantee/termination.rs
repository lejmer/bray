use std::collections::BTreeSet;

use bray_bound_tree::{
    AnyBoundNodeId, BoundAssignmentOperator, BoundBlockItem, BoundExpression, BoundExpressionId,
    BoundOperator, BoundStructuredExpressionKind, OperatorTarget, SelectedOperation,
    SemanticSelection, StorageAccessPurpose, StorageIdentity, StorageIdentityId, StoragePlan,
};

use super::super::id::AnalysisBlockId;
use super::super::model::{AnalysisOperationKind, ControlFlowGraph};
use crate::{CheckerRequestContext, CheckerUnitView, ExecutionCondition};

/// Certifies the closed unsigned-counter case. Arbitrary loops still need a proof.
pub(super) fn bounded_loop_headers<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    graph: &ControlFlowGraph,
    semantics: &bray_bound_tree::CheckedExpressionSemantics,
    storage: &StoragePlan,
    literals: &std::collections::BTreeMap<BoundExpressionId, ExecutionCondition>,
) -> BTreeSet<AnalysisBlockId> {
    graph
        .blocks()
        .iter()
        .filter_map(|block| {
            block
                .operations()
                .iter()
                .filter_map(|id| graph.operation(*id))
                .find_map(|operation| {
                    let AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(id)) =
                        operation.kind()
                    else {
                        return None;
                    };

                    certifies_counter_loop(request, semantics, storage, literals, id)
                        .then_some(block.id())
                })
        })
        .collect()
}

fn certifies_counter_loop<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    semantics: &bray_bound_tree::CheckedExpressionSemantics,
    storage: &StoragePlan,
    literals: &std::collections::BTreeMap<BoundExpressionId, ExecutionCondition>,
    id: BoundExpressionId,
) -> bool {
    let Some(BoundExpression::Structured(region)) = request.view().expression(id) else {
        return false;
    };

    if region.kind() != BoundStructuredExpressionKind::While {
        return false;
    }

    let ([condition], [body, ..]) = (region.operands(), region.blocks()) else {
        return false;
    };

    let condition_id = *condition;

    let Some(BoundExpression::Binary(condition)) = request.view().expression(condition_id) else {
        return false;
    };

    if condition.operator() != BoundOperator::Less
        || !matches!(
            semantics.selections().expression(condition_id),
            Some(SemanticSelection::Operation(SelectedOperation::Operator {
                target: OperatorTarget::BuiltIn(BoundOperator::Less),
                ..
            }))
        )
    {
        return false;
    }

    let [counter, bound] = condition.operands() else {
        return false;
    };

    let Some(counter_root) = local_unsigned_scalar(request, semantics, storage, *counter) else {
        return false;
    };

    let Some(bound_root) = local_unsigned_scalar(request, semantics, storage, *bound) else {
        return false;
    };

    if counter_root == bound_root {
        return false;
    }

    let Some(BoundBlockItem::Expression(increment)) = request
        .view()
        .block(*body)
        .and_then(|body| body.items().last())
    else {
        return false;
    };

    let Some(BoundExpression::Assignment(assignment)) = request.view().expression(*increment)
    else {
        return false;
    };

    let [destination, amount] = assignment.operands() else {
        return false;
    };

    if assignment.operator() != BoundAssignmentOperator::Add
        || local_unsigned_scalar(request, semantics, storage, *destination) != Some(counter_root)
        || !matches!(semantics.selections().expression(*increment), Some(SemanticSelection::Operation(SelectedOperation::CompoundAssignment(selection))) if matches!(selection.target(), OperatorTarget::BuiltIn(BoundOperator::Add)))
        || !matches!(literals.get(amount), Some(ExecutionCondition::Literal(value)) if matches!(value.kind(), bray_symbols::ConstantValueKind::Integer(value) if value.to_u64() == Some(1)))
    {
        return false;
    }

    // The final increment executes on every fallthrough back edge. No address of
    // either scalar may escape, and no other operation may change either value.
    // With counter < bound <= usize::MAX, adding one cannot wrap.
    storage.access_plans().iter().all(|plan| {
        let Some(root) = storage.root_identity(plan.access()) else {
            return true;
        };

        if root != counter_root && root != bound_root {
            return true;
        }

        match plan.purpose() {
            StorageAccessPurpose::Borrow(_) => false,
            StorageAccessPurpose::Assignment => {
                root == counter_root && plan.occurrence() == (*increment).into()
            }
            StorageAccessPurpose::Move => false,
            _ => true,
        }
    })
}

fn local_unsigned_scalar<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    semantics: &bray_bound_tree::CheckedExpressionSemantics,
    storage: &StoragePlan,
    expression: BoundExpressionId,
) -> Option<StorageIdentityId> {
    if !matches!(
        request.view().expression(expression),
        Some(BoundExpression::Name(_))
    ) {
        return None;
    }

    let ty = semantics.types().expression(expression)?.ty();

    if crate::representation::type_representation(request, ty)
        != Some(bray_compiler_known::RepresentationRole::ScalarUsize)
    {
        return None;
    }

    storage.expression_plans(expression).find_map(|plan| {
        let root = storage.root_identity(plan.access())?;

        matches!(storage.identity(root), Some(StorageIdentity::LocalOwned(_))).then_some(root)
    })
}
