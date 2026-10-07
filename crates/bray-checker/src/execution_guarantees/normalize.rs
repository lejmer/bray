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
    Ok(predicate_conditions(unit, semantics, values)?
        .into_iter()
        .map(|(_, condition, _)| condition)
        .collect())
}

/// Normalizes predicate meaning and preserves explicit trusted contract requirements.
pub fn predicate_conditions(
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    values: &SemanticValueStore,
) -> Result<Vec<(BoundExpressionId, ExecutionCondition, bool)>, bray_symbols::SemanticValueStoreError> {
    predicate_conditions_with_inputs(unit, semantics, values, &BTreeMap::new())
}

/// Normalizes contract type clauses using exact declaration-local input placeholders.
pub fn predicate_conditions_with_inputs(
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    values: &SemanticValueStore,
    inputs: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
) -> Result<Vec<(BoundExpressionId, ExecutionCondition, bool)>, bray_symbols::SemanticValueStoreError> {
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

    let mut pending = roots.into_iter().rev().map(|root| (root, false)).collect::<Vec<_>>();
    let mut conditions = Vec::new();

    while let Some((root, trusted)) = pending.pop() {
        match unit.view().expression(root) {
            Some(BoundExpression::Binary(binary)) if binary.operator() == bray_bound_tree::BoundOperator::LogicalAnd => {
                pending.extend(binary.operands().iter().rev().map(|operand| (*operand, trusted)));

                continue;
            },
            Some(BoundExpression::Structured(structured)) if matches!(structured.kind(),
                bray_bound_tree::BoundStructuredExpressionKind::TrustBoundary
                | bray_bound_tree::BoundStructuredExpressionKind::Condition) => {
                if let [operand] = structured.operands() {
                    pending.push((*operand, trusted || structured.kind() == bray_bound_tree::BoundStructuredExpressionKind::TrustBoundary));

                    continue;
                }
            },
            _ => {},
        }

            let condition = expression_condition(
                unit,
                semantics,
                values,
                &literals,
                inputs,
                &BTreeMap::new(),
                root,
                &mut { crate::ExecutionCondition::WORK_LIMIT },
            );

            let is_trusted = trusted || contains_trusted_condition(unit, root);

            conditions.push((root, condition, is_trusted));
    }

    Ok(conditions)
}

fn contains_trusted_condition(unit: &BoundUnit, root: BoundExpressionId) -> bool {
    let mut pending = vec![root];
    let mut budget = ExecutionCondition::WORK_LIMIT;

    while let Some(expression) = pending.pop() {
        let Some(remaining) = budget.checked_sub(1) else {
            return true;
        };

        budget = remaining;

        let Some(bound) = unit.view().expression(expression) else {
            continue;
        };

        if matches!(bound, BoundExpression::Structured(expression)
            if expression.kind() == bray_bound_tree::BoundStructuredExpressionKind::TrustBoundary) {
            return true;
        }

        pending.extend(bound.child_expressions());
    }

    false
}

pub(crate) fn expression_condition(
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    values: &SemanticValueStore,
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
        if call.implementation_hook() == Some(bray_compiler_known::ImplementationHook::RawPointerReinterpret)
            && let [bray_bound_tree::SelectedArgument::Explicit { expression: pointer, conversion, .. }] = call.arguments()
            && matches!(conversion.target(), bray_bound_tree::ConversionTarget::Identity) {
            return expression_condition(unit, semantics, values, literals, current, evaluated, *pointer, budget);
        }

        if let bray_bound_tree::BoundCallableTarget::Declaration(callable) = call.target()
            && let BoundExpression::Call(bound_call) = bound
            && semantics.types().expression(bound_call.callee()).is_some_and(|ty|
                matches!(values.type_data(ty.ty()).as_ref(), bray_symbols::TypeData::Callable(callable)
                    if callable.constness() == bray_symbols::CallableConstness::Constant)) {
            let arguments = call_argument_conditions(call, unit, semantics, values, literals, current, evaluated, budget);

            return ExecutionCondition::call(callable, arguments);
        }

        if let bray_bound_tree::BoundCallableTarget::Predicate(predicate) = call.target() {
            let arguments = call_argument_conditions(call, unit, semantics, values, literals, current, evaluated, budget);

            return ExecutionCondition::predicate(
                predicate.definition(),
                predicate.substitution(),
                arguments,
            );
        }

        // A runtime call has a value identity even when its contents are opaque.
        return ExecutionCondition::Expression(expression);
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
                        values,
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

    if let Some(constructed) = constructed_condition(expression, bound, semantics, &mut |source|
        expression_condition(unit, semantics, values, literals, current, evaluated, source, budget)) {
        return constructed;
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
        BoundExpression::PatternReference(reference) => current.get(&BoundReferenceTarget::Local(reference.binding().into()).into())
            .cloned().unwrap_or(ExecutionCondition::Unknown),
        BoundExpression::MemberAccess(member) => {
            if let Some(bray_bound_tree::BoundMemberSelector::TupleElement(index)) = member.selector() {
                return expression_condition(unit, semantics, values, literals, current, evaluated, member.receiver(), budget)
                    .project(bray_bound_tree::StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(*index)));
            }

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
        BoundExpression::Structured(operation) if operation.kind() == bray_bound_tree::BoundStructuredExpressionKind::ElementIndex
            && matches!(semantics.selections().expression(expression),
                Some(SemanticSelection::Operation(SelectedOperation::Index { target: bray_bound_tree::IndexTarget::ArrayElement, .. }))) => {
            let [receiver, index] = operation.operands() else { return ExecutionCondition::Unknown; };

            let index = expression_condition(unit, semantics, values, literals, current, evaluated, *index, budget);

            let ExecutionCondition::Literal(index) = index else { return ExecutionCondition::Unknown; };

            let ConstantValueKind::Integer(index) = index.kind() else { return ExecutionCondition::Unknown; };

            let Some(index) = index.to_u64().and_then(|index| u32::try_from(index).ok()) else { return ExecutionCondition::Unknown; };

            expression_condition(unit, semantics, values, literals, current, evaluated, *receiver, budget)
                .project(bray_bound_tree::StorageProjection::ElementFromStart(bray_symbols::SymbolOrdinal::new(index)))
        }
        BoundExpression::Structured(operation)
            if matches!(
                operation.kind(),
                bray_bound_tree::BoundStructuredExpressionKind::Condition
                    | bray_bound_tree::BoundStructuredExpressionKind::Borrow
                    | bray_bound_tree::BoundStructuredExpressionKind::TrustBoundary
            ) =>
        {
            if operation.kind() == bray_bound_tree::BoundStructuredExpressionKind::Borrow
                && let Some(operand) = operation.operands().first() {
                let value = expression_condition(unit, semantics, values, literals, current, evaluated, *operand, budget);

                return if matches!(value, ExecutionCondition::Literal(_) | ExecutionCondition::Boolean(_)) {
                    // Equal scalar contents do not identify the storage being borrowed.
                    expression_place(unit, semantics, *operand).map(ExecutionCondition::Input)
                        .unwrap_or(ExecutionCondition::Expression(*operand))
                } else { value };
            }

            operation
                .operands()
                .first()
                .map(|operand| {
                    expression_condition(
                        unit, semantics, values, literals, current, evaluated, *operand, budget,
                    )
                })
                .unwrap_or(ExecutionCondition::Unknown)
        }
        _ => ExecutionCondition::Unknown,
    }
}

fn constructed_condition(
    expression: BoundExpressionId,
    bound: &BoundExpression,
    semantics: &CheckedExpressionSemantics,
    value: &mut impl FnMut(BoundExpressionId) -> ExecutionCondition,
) -> Option<ExecutionCondition> {
    if let Some(SemanticSelection::Operation(SelectedOperation::Construction(construction))) =
        semantics.selections().expression(expression)
        && matches!(construction.target(), bray_bound_tree::ConstructionTarget::Struct(_)
            | bray_bound_tree::ConstructionTarget::UnionVariant(_))
    {
        let fields = construction.inputs().iter().filter_map(|input| {
            let (input, source) = match input {
                bray_bound_tree::SelectedConstructionInput::Explicit { input, expression, .. } => (*input, Some(*expression)),
                bray_bound_tree::SelectedConstructionInput::Default { input, .. } => (*input, None),
            };

            let field = match input {
                bray_bound_tree::ConstructionInputId::StructField(field) => bray_bound_tree::StorageProjection::ProductField(field),
                bray_bound_tree::ConstructionInputId::UnionPayloadField(field) => {
                    let bray_bound_tree::ConstructionTarget::UnionVariant(variant) = construction.target() else {
                        unreachable!("checked union payload input must select a union variant");
                    };

                    bray_bound_tree::StorageProjection::ActiveUnionPayloadField { variant, field }
                },
                bray_bound_tree::ConstructionInputId::CallableParameter(_) => return None,
            };

            Some((field, source.map(&mut *value).unwrap_or(ExecutionCondition::Unknown)))
        }).collect::<Vec<_>>();

        return Some(ExecutionCondition::Constructed(expression, fields.into()));
    }

    match bound {
        BoundExpression::Structured(operation) if matches!(operation.kind(),
            bray_bound_tree::BoundStructuredExpressionKind::Tuple | bray_bound_tree::BoundStructuredExpressionKind::Array) => {
            let fields = operation.operands().iter().enumerate().map(|(index, operand)| {
                let index = bray_symbols::SymbolOrdinal::new(u32::try_from(index)
                    .expect("checked aggregate index must fit its bound unit"));

                let projection = if operation.kind() == bray_bound_tree::BoundStructuredExpressionKind::Tuple {
                    bray_bound_tree::StorageProjection::TupleElement(index)
                } else { bray_bound_tree::StorageProjection::ElementFromStart(index) };

                (projection, value(*operand))
            }).collect::<Vec<_>>();

            Some(ExecutionCondition::Constructed(expression, fields.into()))
        }
        BoundExpression::Structured(operation) if operation.kind() == bray_bound_tree::BoundStructuredExpressionKind::RepeatedArray => {
            let [operand, count] = operation.operands() else { return None; };

            let ExecutionCondition::Literal(count) = value(*count) else { return None; };

            let ConstantValueKind::Integer(count) = count.kind() else { return None; };

            let count = usize::try_from(count.to_u64()?).ok()?.min(ExecutionCondition::WORK_LIMIT);
            let value = value(*operand);

            let fields = (0..count).map(|index| (bray_bound_tree::StorageProjection::ElementFromStart(
                bray_symbols::SymbolOrdinal::new(u32::try_from(index).expect("bounded repeat index must fit"))), value.clone())).collect::<Vec<_>>();

            Some(ExecutionCondition::Constructed(expression, fields.into()))
        }
        _ => None,
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
            BoundExpression::Structured(operation)
                if matches!(operation.kind(), bray_bound_tree::BoundStructuredExpressionKind::Borrow
                    | bray_bound_tree::BoundStructuredExpressionKind::Condition
                    | bray_bound_tree::BoundStructuredExpressionKind::TrustBoundary) => {
                expression = *operation.operands().first()?;
            }
            _ => return None,
        }
    }

    None
}
fn call_argument_conditions(
    call: &bray_bound_tree::SelectedCall,
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    values: &SemanticValueStore,
    literals: &BTreeMap<BoundExpressionId, ExecutionCondition>,
    current: &BTreeMap<super::ExecutionPlace, ExecutionCondition>,
    evaluated: &BTreeMap<BoundExpressionId, ExecutionCondition>,
    budget: &mut usize,
) -> Vec<ExecutionCondition> {
    let mut arguments = Vec::new();

    if let Some(receiver) = call.receiver() {
        arguments.push(expression_condition(unit, semantics, values, literals, current, evaluated,
            receiver.expression(), budget));
    }

    let mut parameters = call.arguments().iter().map(|argument| match argument {
        bray_bound_tree::SelectedArgument::Explicit { expression, ordinal, conversion, .. }
            if matches!(conversion.target(), bray_bound_tree::ConversionTarget::Identity
                | bray_bound_tree::ConversionTarget::CallableContract) =>
            (*ordinal, expression_condition(unit, semantics, values, literals, current, evaluated, *expression, budget)),
        bray_bound_tree::SelectedArgument::Explicit { ordinal, .. }
            | bray_bound_tree::SelectedArgument::Default { ordinal, .. } => (*ordinal, ExecutionCondition::Unknown),
    }).collect::<Vec<_>>();

    parameters.sort_by_key(|(ordinal, _)| *ordinal);
    arguments.extend(parameters.into_iter().map(|(_, value)| value));

    arguments
}

pub(crate) fn condition_literals(
    semantics: &CheckedExpressionSemantics,
    values: &SemanticValueStore,
) -> BTreeMap<BoundExpressionId, ExecutionCondition> {
    semantics
        .literals()
        .entries()
        .iter()
        .map(|literal| {
            let value = values.constant_value_data(literal.value());

            let condition = match value.kind() {
                ConstantValueKind::Boolean(value) => ExecutionCondition::Boolean(*value),
                _ => ExecutionCondition::Literal(value),
            };

            (literal.expression(), condition)
        })
        .collect()
}
