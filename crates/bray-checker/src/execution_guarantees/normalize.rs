use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundUnit, BoundUnitRoot,
    CheckedExpressionSemantics, OperatorTarget, SelectedOperation, SemanticSelection,
};
use bray_symbols::{AnyLocalSymbolId, AnySymbolId, ConstantValueKind, SemanticValueStore};

use super::ExecutionCondition;

/// Normalizes checked contract predicates while preserving unknown meaning.
pub fn execution_conditions(
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    values: &SemanticValueStore,
) -> Result<Vec<ExecutionCondition>, bray_symbols::SemanticValueStoreError> {
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

    let literals = condition_literals(semantics, values)?;

    Ok(roots
        .into_iter()
        .map(|root| {
            expression_condition(
                unit,
                semantics,
                &literals,
                &BTreeMap::new(),
                &BTreeMap::new(),
                root,
                &mut { crate::ExecutionCondition::WORK_LIMIT },
            )
        })
        .collect())
}

pub(crate) fn expression_condition(
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    literals: &BTreeMap<BoundExpressionId, ExecutionCondition>,
    current: &BTreeMap<super::ExecutionPlace, ExecutionCondition>,
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

    if let Some(SemanticSelection::Call(call)) = semantics.selections().expression(expression) {
        if let bray_bound_tree::BoundCallableTarget::Predicate(predicate) = call.target() {
            let mut arguments = Vec::new();

            for argument in call.arguments() {
                let bray_bound_tree::SelectedArgument::Explicit {
                    expression,
                    ordinal,
                    conversion,
                    ..
                } = argument
                else {
                    return ExecutionCondition::Unknown;
                };

                let value = if matches!(
                    conversion.target(),
                    bray_bound_tree::ConversionTarget::Identity
                ) {
                    expression_condition(
                        unit,
                        semantics,
                        literals,
                        current,
                        evaluated,
                        *expression,
                        budget,
                    )
                } else {
                    ExecutionCondition::Unknown
                };

                arguments.push((*ordinal, value));
            }

            arguments.sort_by_key(|(ordinal, _)| *ordinal);

            return ExecutionCondition::predicate(
                predicate.definition(),
                predicate.substitution(),
                arguments.into_iter().map(|(_, value)| value).collect(),
            );
        }
    }

    if let Some(SemanticSelection::Predicate(predicate)) =
        semantics.selections().expression(expression)
    {
        let mut arguments = predicate
            .arguments()
            .iter()
            .map(|argument| {
                (
                    argument.parameter(),
                    expression_condition(
                        unit,
                        semantics,
                        literals,
                        current,
                        evaluated,
                        argument.expression(),
                        budget,
                    ),
                )
            })
            .collect::<Vec<_>>();

        arguments.sort_by_key(|(parameter, _)| *parameter);

        return ExecutionCondition::predicate(
            predicate.predicate(),
            predicate.substitution(),
            arguments.into_iter().map(|(_, value)| value).collect(),
        );
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
        BoundExpression::MemberAccess(member) => {
            let Some(SemanticSelection::Operation(SelectedOperation::Member(target))) =
                semantics.selections().expression(expression)
            else {
                return ExecutionCondition::Unknown;
            };

            let Some(place) = expression_place(unit, semantics, expression) else {
                return ExecutionCondition::Unknown;
            };

            if let Some(value) = current.get(&place) {
                return value.clone();
            }

            let receiver = expression_condition(
                unit,
                semantics,
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
                        unit, semantics, literals, current, evaluated, *operand, budget,
                    )
                })
                .collect();

            ExecutionCondition::operation(operator, operands)
        }
        BoundExpression::Structured(operation)
            if matches!(
                operation.kind(),
                bray_bound_tree::BoundStructuredExpressionKind::Condition
            ) =>
        {
            operation
                .operands()
                .first()
                .map(|operand| {
                    expression_condition(
                        unit, semantics, literals, current, evaluated, *operand, budget,
                    )
                })
                .unwrap_or(ExecutionCondition::Unknown)
        }
        _ => ExecutionCondition::Unknown,
    }
}

pub(crate) fn expression_place(
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    mut expression: BoundExpressionId,
) -> Option<super::ExecutionPlace> {
    let mut fields = Vec::new();

    for _ in 0..ExecutionCondition::WORK_LIMIT {
        match unit.view().expression(expression)? {
            BoundExpression::Name(name) => {
                let mut place = super::ExecutionPlace::from(name.target());

                for field in fields.into_iter().rev() {
                    place = place.field(field);
                }

                return Some(place);
            }
            BoundExpression::MemberAccess(member) => {
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

                fields.push(target.member());
                expression = member.receiver();
            }
            _ => return None,
        }
    }

    None
}
pub(crate) fn condition_literals(
    semantics: &CheckedExpressionSemantics,
    values: &SemanticValueStore,
) -> Result<BTreeMap<BoundExpressionId, ExecutionCondition>, bray_symbols::SemanticValueStoreError>
{
    semantics
        .literals()
        .entries()
        .iter()
        .map(|literal| {
            let value = values.constant_value_data(literal.value())?;

            let condition = match value.kind() {
                ConstantValueKind::Boolean(value) => ExecutionCondition::Boolean(*value),
                _ => ExecutionCondition::Literal(value),
            };

            Ok((literal.expression(), condition))
        })
        .collect()
}
