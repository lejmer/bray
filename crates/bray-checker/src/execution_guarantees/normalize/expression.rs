use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundUnit,
    CheckedExpressionSemantics, OperatorTarget, SelectedOperation, SemanticSelection,
};
use bray_symbols::{AnyLocalSymbolId, AnySymbolId, ConstantValueKind, SemanticValueStore};

use crate::ExecutionCondition;

use super::calls::selected_condition;
use super::values::{constructed_condition, conversion_condition, nullable_condition};

pub(crate) fn expression_condition(
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    values: &SemanticValueStore,
    literals: &BTreeMap<BoundExpressionId, ExecutionCondition>,
    current: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    evaluated: &BTreeMap<BoundExpressionId, ExecutionCondition>,
    expression: BoundExpressionId,
    budget: &mut usize,
) -> ExecutionCondition {
    let Some(next) = budget.checked_sub(1) else {
        return ExecutionCondition::Unknown;
    };

    *budget = next;

    let Some(bound) = unit
        .view()
        .expression(expression)
        .filter(|bound| !bound.is_recovered())
    else {
        return ExecutionCondition::Unknown;
    };

    if let Some(literal) = evaluated
        .get(&expression)
        .or_else(|| literals.get(&expression))
    {
        // Literal payloads are immutable and shared by the expression and flow state.
        return literal.clone();
    }

    if let Some(condition) = selected_condition(
        unit, semantics, values, literals, current, evaluated, expression, budget,
    ) {
        return condition;
    }

    if let Some(constructed) =
        constructed_condition(expression, bound, semantics, values, &mut |source| {
            expression_condition(
                unit, semantics, values, literals, current, evaluated, source, budget,
            )
        })
    {
        return constructed;
    }

    if let Some(condition) = nullable_condition(unit, semantics, expression, bound, &mut |source| {
        expression_condition(
            unit, semantics, values, literals, current, evaluated, source, budget,
        )
    }) {
        return condition;
    }

    if let Some(condition) = conversion_condition(expression, bound, semantics, &mut |source| {
        expression_condition(
            unit, semantics, values, literals, current, evaluated, source, budget,
        )
    }) {
        return condition;
    }

    match bound {
        BoundExpression::Name(name) => {
            if let Some(value) = current.get(&name.target().into()) {
                // The current state and expression retain the same immutable term independently.
                return value.clone();
            }

            match name.target() {
                BoundReferenceTarget::Surface(
                    AnySymbolId::CallableParameter(_) | AnySymbolId::ReceiverParameter(_),
                )
                | BoundReferenceTarget::Local(AnyLocalSymbolId::AnonymousCallableParameter(_)) => {
                    ExecutionCondition::Input(name.target().into())
                }
                BoundReferenceTarget::Local(AnyLocalSymbolId::PostconditionResult(_)) => {
                    ExecutionCondition::Result
                }
                _ => ExecutionCondition::Unknown,
            }
        }
        BoundExpression::PatternReference(reference) => current
            .get(&BoundReferenceTarget::Local(reference.binding().into()).into())
            .cloned()
            .unwrap_or(ExecutionCondition::Unknown),
        BoundExpression::MemberAccess(member) => {
            if let Some(bray_bound_tree::BoundMemberSelector::TupleElement(index)) =
                member.selector()
            {
                return expression_condition(
                    unit,
                    semantics,
                    values,
                    literals,
                    current,
                    evaluated,
                    member.receiver(),
                    budget,
                )
                .project(bray_bound_tree::StorageProjection::TupleElement(
                    bray_symbols::SymbolOrdinal::new(*index),
                ));
            }

            let Some(SemanticSelection::Operation(SelectedOperation::Member(target))) =
                semantics.selections().expression(expression)
            else {
                return ExecutionCondition::Unknown;
            };

            if let Some(value) = current.get(&BoundReferenceTarget::Surface(target.member()).into())
            {
                return value.clone();
            }

            let Some(place) = expression_place(unit, semantics, values, expression) else {
                return ExecutionCondition::Unknown;
            };

            if let Some(value) = current.get(&place) {
                return value.clone();
            }

            let receiver = expression_condition(
                unit,
                semantics,
                values,
                literals,
                current,
                evaluated,
                member.receiver(),
                budget,
            );

            ExecutionCondition::field(target.member(), receiver)
        }
        BoundExpression::Unary(_) | BoundExpression::Binary(_) => {
            let (operator, operands) = match bound {
                BoundExpression::Unary(operation) => (operation.operator(), operation.operands()),
                BoundExpression::Binary(operation) => (operation.operator(), operation.operands()),
                _ => return ExecutionCondition::Unknown,
            };

            let Some(SemanticSelection::Operation(SelectedOperation::Operator {
                target: OperatorTarget::BuiltIn(_),
                ..
            })) = semantics.selections().expression(expression)
            else {
                return ExecutionCondition::Unknown;
            };

            let operands = operands
                .iter()
                .map(|operand| {
                    expression_condition(
                        unit, semantics, values, literals, current, evaluated, *operand, budget,
                    )
                })
                .collect();

            ExecutionCondition::operation(operator, operands)
        }
        BoundExpression::Structured(operation)
            if operation.kind() == bray_bound_tree::BoundStructuredExpressionKind::ElementIndex
                && matches!(
                    semantics.selections().expression(expression),
                    Some(SemanticSelection::Operation(SelectedOperation::Index {
                        target: bray_bound_tree::IndexTarget::ArrayElement,
                        ..
                    }))
                ) =>
        {
            let [receiver, index] = operation.operands() else {
                return ExecutionCondition::Unknown;
            };

            let index = expression_condition(
                unit, semantics, values, literals, current, evaluated, *index, budget,
            );

            let ExecutionCondition::Literal(index) = index else {
                return ExecutionCondition::Unknown;
            };

            let ConstantValueKind::Integer(index) = index.kind() else {
                return ExecutionCondition::Unknown;
            };

            let Some(index) = index.to_u64().and_then(|index| u32::try_from(index).ok()) else {
                return ExecutionCondition::Unknown;
            };

            expression_condition(
                unit, semantics, values, literals, current, evaluated, *receiver, budget,
            )
            .project(bray_bound_tree::StorageProjection::ElementFromStart(
                bray_symbols::SymbolOrdinal::new(index),
            ))
        }
        BoundExpression::Structured(operation)
            if matches!(
                operation.kind(),
                bray_bound_tree::BoundStructuredExpressionKind::Condition
                    | bray_bound_tree::BoundStructuredExpressionKind::Borrow
                    | bray_bound_tree::BoundStructuredExpressionKind::TrustBoundary
            ) =>
        {
            let Some(operand) = operation.operands().first() else {
                return ExecutionCondition::Unknown;
            };

            let value = expression_condition(
                unit, semantics, values, literals, current, evaluated, *operand, budget,
            );

            match operation.kind() {
                bray_bound_tree::BoundStructuredExpressionKind::TrustBoundary => {
                    value.requiring_trusted_predicates()
                }
                bray_bound_tree::BoundStructuredExpressionKind::Borrow
                    if matches!(
                        value,
                        ExecutionCondition::Literal(_)
                            | ExecutionCondition::Boolean(_)
                            | ExecutionCondition::Unknown
                    ) =>
                {
                    // Equal scalar contents do not identify the borrowed storage.
                    expression_place(unit, semantics, values, *operand)
                        .map(ExecutionCondition::Input)
                        .unwrap_or(ExecutionCondition::Expression(*operand))
                }
                _ => value,
            }
        }
        _ => ExecutionCondition::Unknown,
    }
}

pub(crate) fn expression_place(
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    values: &SemanticValueStore,
    mut expression: BoundExpressionId,
) -> Option<crate::ExecutionPlace> {
    use bray_symbols::ConstantProjectionKind;

    let mut projections = Vec::new();

    for _ in 0..ExecutionCondition::WORK_LIMIT {
        match unit.view().expression(expression)? {
            BoundExpression::Name(name) => {
                let mut place = crate::ExecutionPlace::from(name.target());

                for projection in projections.into_iter().rev() {
                    place = place.component(projection);
                }

                return Some(place);
            }
            BoundExpression::PatternReference(reference) => {
                let mut place = crate::ExecutionPlace::from(BoundReferenceTarget::Local(
                    reference.binding().into(),
                ));

                for projection in projections.into_iter().rev() {
                    place = place.component(projection);
                }

                return Some(place);
            }
            BoundExpression::MemberAccess(member) => {
                if let Some(bray_bound_tree::BoundMemberSelector::TupleElement(index)) =
                    member.selector()
                {
                    projections.push(ConstantProjectionKind::TupleElement(
                        bray_symbols::SymbolOrdinal::new(*index),
                    ));

                    expression = member.receiver();

                    continue;
                }

                let SemanticSelection::Operation(SelectedOperation::Member(target)) =
                    semantics.selections().expression(expression)?
                else {
                    return None;
                };

                if !matches!(
                    target.member(),
                    AnySymbolId::StructField(_) | AnySymbolId::UnionPayloadField(_)
                ) {
                    return None;
                }

                projections.push(match target.member() {
                    AnySymbolId::StructField(field) => ConstantProjectionKind::ProductField(field),
                    AnySymbolId::UnionPayloadField(field) => {
                        ConstantProjectionKind::UnionPayloadField(field)
                    }
                    _ => return None,
                });

                expression = member.receiver();
            }
            BoundExpression::Structured(operation)
                if operation.kind()
                    == bray_bound_tree::BoundStructuredExpressionKind::ElementIndex
                    && matches!(
                        semantics.selections().expression(expression),
                        Some(SemanticSelection::Operation(SelectedOperation::Index {
                            target: bray_bound_tree::IndexTarget::ArrayElement,
                            ..
                        }))
                    ) =>
            {
                let [receiver, index] = operation.operands() else {
                    return None;
                };

                let literal = semantics
                    .literals()
                    .entries()
                    .iter()
                    .find(|literal| literal.expression() == *index)?;

                let value = values.constant_value_data(literal.value());

                let ConstantValueKind::Integer(index) = value.kind() else {
                    return None;
                };

                let index = index.to_u64().and_then(|index| u32::try_from(index).ok())?;

                projections.push(ConstantProjectionKind::ArrayElementOrdinal(
                    bray_symbols::SymbolOrdinal::new(index),
                ));

                expression = *receiver;
            }
            BoundExpression::Structured(operation)
                if matches!(
                    operation.kind(),
                    bray_bound_tree::BoundStructuredExpressionKind::Borrow
                        | bray_bound_tree::BoundStructuredExpressionKind::Condition
                        | bray_bound_tree::BoundStructuredExpressionKind::TrustBoundary
                ) =>
            {
                expression = *operation.operands().first()?;
            }
            _ => return None,
        }
    }

    None
}
