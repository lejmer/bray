use bray_bound_tree::{
    BoundCallResult, BoundExpression, BoundExpressionId, BoundReferenceTarget,
    BoundStructuredExpressionKind, CheckedMemoryOperationKind, CheckedMemoryOperations,
    CheckedSemanticSelections, ConstructionTarget, OperatorTarget, SelectedArgument,
    SelectedConstructionInput, SelectedConversion, SelectedOperation, SemanticSelection,
    StorageIdentity, StoragePlan, StorageProjection,
};
use bray_symbols::AnySymbolId;

use crate::execution_guarantees::{ExecutionDependency, ExecutionProperty};
use crate::{CheckerRequestContext, CheckerUnitView};

pub(super) fn collect_preservation_dependencies(
    nodes: impl Iterator<Item = bray_bound_tree::AnyBoundNodeId>,
    selections: &CheckedSemanticSelections,
    memory: &CheckedMemoryOperations,
    dependencies: &mut Vec<ExecutionDependency>,
) {
    // Retaining entry facts across a pure call depends on that call's proof, even when
    // the enclosing obligation is only a completion predicate or termination promise.
    for node in nodes {
        let bray_bound_tree::AnyBoundNodeId::Expression(expression) = node else {
            continue;
        };

        let Some(SemanticSelection::Call(call)) = selections.expression(expression) else {
            continue;
        };

        if call
            .phase_behaviors()
            .invocation()
            .execution_properties()
            .contains(&ExecutionProperty::Pure)
            && !memory
                .operations()
                .iter()
                .any(|operation| operation.expression() == expression)
        {
            dependencies.push(ExecutionDependency {
                target: call.target(),
                property: ExecutionProperty::Pure,
                node,
            });
        }
    }
}

pub(super) fn check_expression<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    property: ExecutionProperty,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
    memory: &CheckedMemoryOperations,
    dependencies: &mut Vec<ExecutionDependency>,
) -> bool {
    let Some(bound) = request.view().expression(expression) else {
        return false;
    };

    if bound.is_recovered() {
        return false;
    }

    if !check_storage_accesses(request, expression.into(), storage, property, dependencies) {
        return false;
    }

    if let Some(operation) = memory
        .operations()
        .iter()
        .find(|operation| operation.expression() == expression)
    {
        return matches!(
            operation.kind(),
            CheckedMemoryOperationKind::BorrowFrom { .. }
                | CheckedMemoryOperationKind::UninitPointer { .. }
        );
    }

    match selections.expression(expression) {
        Some(SemanticSelection::Call(call)) => {
            // TODO(BRA-500): Certify trait dispatch from its preserved contract and dependencies.
            if call.resolution().trait_dispatch().is_some()
                || !matches!(call.resolution().result(), BoundCallResult::Immediate(_))
                || call
                    .arguments()
                    .iter()
                    .any(|argument| matches!(argument, SelectedArgument::Default { .. }))
            {
                return false;
            }

            for argument in call.arguments() {
                if let SelectedArgument::Explicit { conversion, .. } = argument
                    && !collect_conversion_dependencies(
                        conversion,
                        property,
                        expression,
                        dependencies,
                    )
                {
                    return false;
                }
            }

            dependencies.push(ExecutionDependency {
                target: call.target(),
                property,
                node: expression.into(),
            });

            return true;
        }
        Some(SemanticSelection::Operation(operation)) => {
            if !check_operation(operation, property, expression, dependencies) {
                return false;
            }
        }
        Some(SemanticSelection::Propagation(propagation)) => {
            if let Some(conversion) = propagation.error_conversion()
                && !collect_conversion_dependencies(conversion, property, expression, dependencies)
            {
                return false;
            }
        }
        Some(SemanticSelection::Iteration(_) | SemanticSelection::StaticReference(_)) => {
            return false;
        }
        _ => {}
    }

    match bound {
        BoundExpression::Error(_)
        | BoundExpression::ErrorCall(_)
        | BoundExpression::ErrorConversion(_)
        | BoundExpression::UnresolvedReference(_)
        | BoundExpression::BoxConstruction(_)
        | BoundExpression::Await(_)
        | BoundExpression::Generator(_)
        | BoundExpression::For(_) => false,
        BoundExpression::Assignment(_) => property != ExecutionProperty::Pure,
        BoundExpression::Name(name) => !matches!(
            name.target(),
            BoundReferenceTarget::Surface(AnySymbolId::Static(_))
        ),
        BoundExpression::Structured(expression)
            if expression.kind() == BoundStructuredExpressionKind::Panic =>
        {
            property == ExecutionProperty::Total
        }
        BoundExpression::Structured(expression) => !matches!(
            expression.kind(),
            BoundStructuredExpressionKind::With
                | BoundStructuredExpressionKind::GeneralGenerator
                | BoundStructuredExpressionKind::ArrayGenerator
                | BoundStructuredExpressionKind::BooleanAllFold
                | BoundStructuredExpressionKind::BooleanAnyFold
        ),
        BoundExpression::Call(_)
        | BoundExpression::Unary(_)
        | BoundExpression::Binary(_)
        | BoundExpression::Conversion(_)
        | BoundExpression::StructConstruction(_) => selections.expression(expression).is_some(),
        _ => true,
    }
}

pub(super) fn check_storage_accesses<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    node: bray_bound_tree::AnyBoundNodeId,
    storage: &StoragePlan,
    property: ExecutionProperty,
    dependencies: &mut Vec<ExecutionDependency>,
) -> bool {
    for plan in storage
        .access_plans()
        .iter()
        .filter(|plan| plan.node() == node)
    {
        let Some(identity) = storage.root_identity(plan.access()) else {
            return false;
        };

        if storage.identity(identity).is_none_or(|identity| {
            matches!(
                identity,
                StorageIdentity::Static(_) | StorageIdentity::Error(_)
            )
        }) {
            return false;
        }

        let Some(path) = storage.resolved_projections(plan.access()) else {
            return false;
        };

        let kind = plan.purpose().projection_borrow_kind();

        for (index, projection) in path.iter().enumerate() {
            if *projection != StorageProjection::OwnedTarget {
                continue;
            }

            let owner = storage
                .access_at(identity, &path[..index])
                .and_then(|access| storage.access(access))
                .map(|access| access.reached_type())
                .and_then(|ty| request.semantic_values().unborrowed_type(ty).ok());

            let Some(call) = owner.and_then(|owner| storage.owned_borrow(owner, kind)) else {
                return false;
            };

            dependencies.push(ExecutionDependency {
                target: bray_bound_tree::BoundCallableTarget::Declaration(call.callable()),
                property,
                node,
            });
        }
    }

    true
}

fn check_operation(
    operation: &SelectedOperation,
    property: ExecutionProperty,
    expression: BoundExpressionId,
    dependencies: &mut Vec<ExecutionDependency>,
) -> bool {
    let mut calls = Vec::new();
    let mut defaults = Vec::new();

    let resolved =
        crate::behavior::collect_operation_behavior(operation, None, &mut calls, &mut defaults);

    if !resolved || !defaults.is_empty() {
        return false;
    }

    dependencies.extend(calls.iter().map(|call| ExecutionDependency {
        target: call.target(),
        property,
        node: expression.into(),
    }));

    match operation {
        SelectedOperation::Operator {
            target: OperatorTarget::BuiltIn(operator),
            ..
        } => property == ExecutionProperty::Pure || !operator.builtin_may_panic(),
        SelectedOperation::Construction(construction) => {
            matches!(construction.target(), ConstructionTarget::TypeForm { .. })
                || construction
                    .inputs()
                    .iter()
                    .all(|input| matches!(input, SelectedConstructionInput::Explicit { .. }))
        }
        SelectedOperation::CompoundAssignment(_) => {
            property != ExecutionProperty::Pure && !calls.is_empty()
        }
        SelectedOperation::Index { .. } => !calls.is_empty(),
        _ => true,
    }
}

fn collect_conversion_dependencies(
    conversion: &SelectedConversion,
    property: ExecutionProperty,
    expression: BoundExpressionId,
    dependencies: &mut Vec<ExecutionDependency>,
) -> bool {
    let mut calls = Vec::new();
    let resolved = crate::behavior::collect_conversion_behavior(conversion, None, &mut calls);

    dependencies.extend(calls.iter().map(|call| ExecutionDependency {
        target: call.target(),
        property,
        node: expression.into(),
    }));

    resolved
}
