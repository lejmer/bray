use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundUnit,
    CheckedExpressionSemantics, OperatorTarget, SelectedOperation, SemanticSelection,
};
use bray_symbols::{AnyLocalSymbolId, AnySymbolId, ConstantValueKind, SemanticValueStore};

use crate::ExecutionCondition;

use super::calls::selected_condition;
use super::values::{constructed_condition, conversion_condition, pattern_condition};

pub(crate) fn expression_condition(
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    values: &SemanticValueStore,
    literals: &BTreeMap<BoundExpressionId, ExecutionCondition>,
    patterns: &BTreeMap<
        bray_bound_tree::BoundPatternId,
        (bray_bound_tree::BoundOperator, ExecutionCondition),
    >,
    current: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    evaluated: &BTreeMap<BoundExpressionId, ExecutionCondition>,
    expression: BoundExpressionId,
    budget: &mut usize,
    predicates: Option<
        &BTreeMap<bray_symbols::PredicateInstanceData, bray_symbols::PredicateSemanticSummary>,
    >,
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
        unit, semantics, values, literals, patterns, current, evaluated, expression, budget,
        predicates,
    ) {
        return condition;
    }

    if let Some(constructed) =
        constructed_condition(expression, bound, semantics, values, &mut |source| {
            expression_condition(
                unit, semantics, values, literals, patterns, current, evaluated, source, budget,
                predicates,
            )
        })
    {
        return constructed;
    }

    if let Some(condition) = pattern_condition(
        patterns,
        semantics,
        values,
        expression,
        bound,
        &mut |source| {
            expression_condition(
                unit, semantics, values, literals, patterns, current, evaluated, source, budget,
                predicates,
            )
        },
    ) {
        return condition;
    }

    if let Some(condition) = conversion_condition(expression, bound, semantics, &mut |source| {
        expression_condition(
            unit, semantics, values, literals, patterns, current, evaluated, source, budget,
            predicates,
        )
    }) {
        return condition;
    }

    if matches!(
        semantics.selections().expression(expression),
        Some(SemanticSelection::CallableReference(_))
    ) {
        return ExecutionCondition::Expression(expression);
    }

    match bound {
        BoundExpression::AnonymousCallable(_) => ExecutionCondition::Expression(expression),
        BoundExpression::Name(name) => name_condition(
            name.target(),
            semantics.selections().expression(expression),
            current,
        ),
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
                    patterns,
                    current,
                    evaluated,
                    member.receiver(),
                    budget,
                    predicates,
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
                patterns,
                current,
                evaluated,
                member.receiver(),
                budget,
                predicates,
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

            if unit.key().kind().uses_machine_arithmetic()
                && !matches!(operator,
                    bray_bound_tree::BoundOperator::Add if operands.len() == 1)
                // Division and remainder do not wrap on normal completion. Retain their
                // operation in short-circuit snapshots before an operand executes.
                // Trapping input domains are still checked at the actual operation.
                && !matches!(operator, bray_bound_tree::BoundOperator::Divide
                    | bray_bound_tree::BoundOperator::Remainder | bray_bound_tree::BoundOperator::LogicalNot
                    | bray_bound_tree::BoundOperator::LogicalAnd | bray_bound_tree::BoundOperator::LogicalOr
                    | bray_bound_tree::BoundOperator::Equal | bray_bound_tree::BoundOperator::NotEqual
                    | bray_bound_tree::BoundOperator::Less | bray_bound_tree::BoundOperator::LessEqual
                    | bray_bound_tree::BoundOperator::Greater | bray_bound_tree::BoundOperator::GreaterEqual)
            {
                return ExecutionCondition::Expression(expression);
            }

            let operands = operands
                .iter()
                .map(|operand| {
                    expression_condition(
                        unit, semantics, values, literals, patterns, current, evaluated, *operand,
                        budget, predicates,
                    )
                })
                .collect();

            ExecutionCondition::operation(operator, operands)
        }
        BoundExpression::Structured(operation) => super::values::structured_condition(
            unit,
            semantics,
            values,
            expression,
            operation,
            &mut |source| {
                expression_condition(
                    unit, semantics, values, literals, patterns, current, evaluated, source,
                    budget, predicates,
                )
            },
        ),
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
                let mut place = match semantics.selections().expression(expression) {
                    Some(SemanticSelection::StaticReference(selection)) => {
                        crate::ExecutionPlace::static_instance(selection.clone())
                    }
                    _ => crate::ExecutionPlace::from(name.target()),
                };

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

fn name_condition(
    target: BoundReferenceTarget,
    selection: Option<&SemanticSelection>,
    current: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
) -> ExecutionCondition {
    let place = match selection {
        Some(SemanticSelection::StaticReference(selection)) => {
            crate::ExecutionPlace::static_instance(selection.clone())
        }
        _ => target.into(),
    };

    if let Some(value) = current.get(&place) {
        // The current state and expression retain the same immutable term independently.
        return value.clone();
    }

    match target {
        BoundReferenceTarget::Surface(
            AnySymbolId::CallableParameter(_)
            | AnySymbolId::PredicateParameter(_)
            | AnySymbolId::ReceiverParameter(_)
            | AnySymbolId::GenericConstParameter(_),
        )
        | BoundReferenceTarget::Local(AnyLocalSymbolId::AnonymousCallableParameter(_)) => {
            ExecutionCondition::Input(target.into())
        }
        BoundReferenceTarget::Local(AnyLocalSymbolId::PostconditionResult(_)) => {
            ExecutionCondition::Result
        }
        _ => ExecutionCondition::Unknown,
    }
}
