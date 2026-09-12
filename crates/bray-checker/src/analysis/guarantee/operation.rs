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

    if storage
        .access_plans()
        .iter()
        .filter(|plan| plan.node() == expression.into())
        .any(|plan| {
            storage
                .root_identity(plan.access())
                .and_then(|id| storage.identity(id))
                .is_none_or(|identity| {
                    matches!(
                        identity,
                        StorageIdentity::Static(_) | StorageIdentity::Error(_)
                    )
                })
                || storage
                    .resolved_projections(plan.access())
                    .is_none_or(|path| path.contains(&StorageProjection::OwnedTarget))
        })
    {
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
