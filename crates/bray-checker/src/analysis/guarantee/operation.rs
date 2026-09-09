use std::collections::BTreeSet;

use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundExpressionId, BoundOperator, BoundReferenceTarget,
    BoundStructuredExpressionKind, CheckedAsync, CheckedSemanticSelections, ConstructionTarget,
    ConversionTarget, OperatorTarget, SelectedConstructionInput, SelectedConversion,
    SelectedOperation, SemanticSelection, StorageIdentity, StoragePlan, StorageProjection,
};
use bray_symbols::{AnySymbolId, ExecutionProperty};

use crate::{CheckerRequestContext, CheckerUnitView};

use super::super::model::{AnalysisCallPhase, AnalysisOperationKind, AnalysisScopeExitPhase};

pub(super) fn operation_preserves_property<C>(
    request: CheckerUnitView<'_, C>,
    operation: AnalysisOperationKind,
    property: ExecutionProperty,
    selections: &CheckedSemanticSelections,
    storage_calls: &BTreeSet<AnyBoundNodeId>,
    asynchronous: &CheckedAsync,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    if storage_calls.contains(&operation.node()) {
        return false;
    }

    match operation {
        AnalysisOperationKind::Recovery(_)
        | AnalysisOperationKind::Suspension { .. }
        | AnalysisOperationKind::TaskOperation { .. } => false,
        AnalysisOperationKind::Call {
            phase: AnalysisCallPhase::Completion,
            ..
        }
        | AnalysisOperationKind::PatternObservation(_) => true,
        AnalysisOperationKind::Call { .. } => false,
        AnalysisOperationKind::ScopeExit {
            block, exit, phase, ..
        } => asynchronous
            .scope_exits()
            .iter()
            .filter(|plan| plan.scope() == block && plan.exit() == exit)
            .all(|plan| {
                !plan.is_recovered()
                    && match phase {
                        AnalysisScopeExitPhase::TaskCancellationBroadcast => {
                            plan.cancellation_broadcast().is_empty()
                        }
                        AnalysisScopeExitPhase::LifecycleResolution => {
                            plan.lifecycle_resolution().is_empty()
                        }
                    }
            }),
        AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression))
        | AnalysisOperationKind::PropagationFailure(expression) => {
            expression_preserves_property(request, expression, property, selections)
        }
        AnalysisOperationKind::Bound(_) => true,
    }
}

pub(super) fn storage_operations_requiring_proof(
    storage: &StoragePlan,
    projections_verified: bool,
) -> BTreeSet<AnyBoundNodeId> {
    storage
        .access_plans()
        .iter()
        .filter_map(|plan| {
            let requires_proof = storage
                .access(plan.access())
                .is_none_or(|access| access.is_recovered())
                || storage
                    .resolved_projections(plan.access())
                    .is_none_or(|projections| {
                        !projections_verified
                            && projections.contains(&StorageProjection::OwnedTarget)
                    })
                || storage
                    .root_identity(plan.access())
                    .and_then(|root| storage.identity(root))
                    .is_none_or(|identity| {
                        matches!(
                            identity,
                            StorageIdentity::Static(_) | StorageIdentity::Error(_)
                        )
                    });

            requires_proof.then_some(plan.node())
        })
        .collect()
}

fn expression_preserves_property<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    property: ExecutionProperty,
    selections: &CheckedSemanticSelections,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(bound) = request.view().expression(expression) else {
        return false;
    };

    if bound.is_recovered() {
        return false;
    }

    if let Some(selection) = selections.expression(expression) {
        match selection {
            SemanticSelection::Call(_)
            | SemanticSelection::Iteration(_)
            | SemanticSelection::StaticReference(_)
            | SemanticSelection::Reference(BoundReferenceTarget::Surface(AnySymbolId::Static(_))) =>
            {
                return false;
            }
            SemanticSelection::Operation(operation)
                if !selected_preserves_property(operation, property) =>
            {
                return false;
            }
            SemanticSelection::Propagation(bray_bound_tree::SelectedPropagation::Result {
                error_conversion,
                ..
            }) if !conversion_preserves_properties(error_conversion) => return false,
            _ => {}
        }
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
        BoundExpression::Call(_) => matches!(
            selections.expression(expression),
            Some(SemanticSelection::Operation(
                SelectedOperation::Construction(_)
            ))
        ),
        BoundExpression::Name(name) => !matches!(
            name.target(),
            BoundReferenceTarget::Surface(AnySymbolId::Static(_))
        ),
        BoundExpression::Structured(structured) => match structured.kind() {
            BoundStructuredExpressionKind::With
            | BoundStructuredExpressionKind::GeneralGenerator
            | BoundStructuredExpressionKind::ArrayGenerator
            | BoundStructuredExpressionKind::BooleanAllFold
            | BoundStructuredExpressionKind::BooleanAnyFold
            | BoundStructuredExpressionKind::Panic => false,
            _ => true,
        },
        BoundExpression::Unary(_)
        | BoundExpression::Binary(_)
        | BoundExpression::Conversion(_)
        | BoundExpression::StructConstruction(_) => selections.expression(expression).is_some(),
        BoundExpression::Block(_)
        | BoundExpression::Literal(_)
        | BoundExpression::PatternReference(_)
        | BoundExpression::AnonymousCallable(_)
        | BoundExpression::MemberAccess(_)
        | BoundExpression::LeadingDotVariant(_)
        | BoundExpression::UnqualifiedVariant(_)
        | BoundExpression::TraitQualifiedMember(_)
        | BoundExpression::ControlTransfer(_)
        | BoundExpression::Match(_) => true,
    }
}

fn selected_preserves_property(operation: &SelectedOperation, property: ExecutionProperty) -> bool {
    match operation {
        SelectedOperation::Operator {
            target: OperatorTarget::BuiltIn(operator),
            ..
        } => {
            property == ExecutionProperty::Pure
                || matches!(
                    operator,
                    BoundOperator::LogicalNot
                        | BoundOperator::LogicalAnd
                        | BoundOperator::LogicalOr
                        | BoundOperator::Equal
                        | BoundOperator::NotEqual
                        | BoundOperator::Less
                        | BoundOperator::LessEqual
                        | BoundOperator::Greater
                        | BoundOperator::GreaterEqual
                )
        }
        SelectedOperation::Construction(construction) => {
            !matches!(construction.target(), ConstructionTarget::TypeForm { .. })
                && construction
                    .inputs()
                    .iter()
                    .all(|input| matches!(input, SelectedConstructionInput::Explicit { .. }))
        }
        SelectedOperation::Conversion(conversion) => conversion_preserves_properties(conversion),
        SelectedOperation::Member(_) | SelectedOperation::Implementation(_) => true,
        SelectedOperation::Operator { .. }
        | SelectedOperation::CompoundAssignment(_)
        | SelectedOperation::Index { .. } => false,
    }
}

fn conversion_preserves_properties(conversion: &SelectedConversion) -> bool {
    let mut pending = vec![conversion];
    let mut remaining = crate::contract::MAX_CONDITION_STEPS;

    while let Some(conversion) = pending.pop() {
        let Some(next) = remaining.checked_sub(1) else {
            return false;
        };

        remaining = next;

        match conversion.target() {
            ConversionTarget::Identity
            | ConversionTarget::CallableContract
            | ConversionTarget::NullablePresent
            | ConversionTarget::BuiltInScalar
            | ConversionTarget::CVariadicPromotion => {}
            ConversionTarget::Composite(elements) => pending.extend(elements.iter()),
            ConversionTarget::Trait { .. } | ConversionTarget::TraitConstraint { .. } => {
                return false;
            }
        }
    }

    true
}
