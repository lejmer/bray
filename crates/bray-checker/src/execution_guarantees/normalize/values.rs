use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundUnit, CheckedExpressionSemantics, SelectedOperation,
    SemanticSelection,
};
use bray_symbols::{ConstantValueKind, SemanticValueStore};

use crate::ExecutionCondition;

pub(super) fn constructed_condition(
    expression: BoundExpressionId,
    bound: &BoundExpression,
    semantics: &CheckedExpressionSemantics,
    values: &SemanticValueStore,
    value: &mut impl FnMut(BoundExpressionId) -> ExecutionCondition,
) -> Option<ExecutionCondition> {
    if let Some(SemanticSelection::Operation(SelectedOperation::Construction(construction))) =
        semantics.selections().expression(expression)
        && matches!(
            construction.target(),
            bray_bound_tree::ConstructionTarget::Struct(_)
                | bray_bound_tree::ConstructionTarget::UnionVariant(_)
        )
    {
        let fields = construction
            .inputs()
            .iter()
            .filter_map(|input| {
                let (input, source, target) = match input {
                    bray_bound_tree::SelectedConstructionInput::Explicit {
                        input,
                        expression,
                        ty,
                        ..
                    } => (*input, Some(*expression), *ty),
                    bray_bound_tree::SelectedConstructionInput::Default { input, ty, .. } => {
                        (*input, None, *ty)
                    }
                };

                let field = match input {
                    bray_bound_tree::ConstructionInputId::StructField(field) => {
                        bray_bound_tree::StorageProjection::ProductField(field)
                    }
                    bray_bound_tree::ConstructionInputId::UnionPayloadField(field) => {
                        let bray_bound_tree::ConstructionTarget::UnionVariant(variant) =
                            construction.target()
                        else {
                            unreachable!("checked union payload input must select a union variant");
                        };

                        bray_bound_tree::StorageProjection::ActiveUnionPayloadField {
                            variant,
                            field,
                        }
                    }
                    bray_bound_tree::ConstructionInputId::CallableParameter(_) => return None,
                };

                Some((
                    field,
                    source
                        .map(|source| {
                            value(source).coerce(
                                values,
                                semantics
                                    .types()
                                    .expression(source)
                                    .expect("construction source retains its type")
                                    .ty(),
                                target,
                            )
                        })
                        .unwrap_or(ExecutionCondition::Unknown),
                ))
            })
            .collect::<Vec<_>>();

        return Some(ExecutionCondition::Constructed(
            std::sync::Arc::new(ExecutionCondition::Expression(expression)),
            fields.into(),
        ));
    }

    match bound {
        BoundExpression::Structured(operation)
            if matches!(
                operation.kind(),
                bray_bound_tree::BoundStructuredExpressionKind::Tuple
                    | bray_bound_tree::BoundStructuredExpressionKind::Array
            ) =>
        {
            let fields = operation
                .operands()
                .iter()
                .enumerate()
                .map(|(index, operand)| {
                    let index = bray_symbols::SymbolOrdinal::new(
                        u32::try_from(index)
                            .expect("checked aggregate index must fit its bound unit"),
                    );

                    let projection = if operation.kind()
                        == bray_bound_tree::BoundStructuredExpressionKind::Tuple
                    {
                        bray_bound_tree::StorageProjection::TupleElement(index)
                    } else {
                        bray_bound_tree::StorageProjection::ElementFromStart(index)
                    };

                    (projection, value(*operand))
                })
                .collect::<Vec<_>>();

            Some(ExecutionCondition::Constructed(
                std::sync::Arc::new(ExecutionCondition::Expression(expression)),
                fields.into(),
            ))
        }
        BoundExpression::Structured(operation)
            if operation.kind()
                == bray_bound_tree::BoundStructuredExpressionKind::RepeatedArray =>
        {
            let [operand, count] = operation.operands() else {
                return None;
            };

            let ExecutionCondition::Literal(count) = value(*count) else {
                return None;
            };

            let ConstantValueKind::Integer(count) = count.kind() else {
                return None;
            };

            let count = usize::try_from(count.to_u64()?)
                .ok()?
                .min(ExecutionCondition::WORK_LIMIT);

            let value = value(*operand);

            let fields = (0..count)
                .map(|index| {
                    (
                        bray_bound_tree::StorageProjection::ElementFromStart(
                            bray_symbols::SymbolOrdinal::new(
                                u32::try_from(index).expect("bounded repeat index must fit"),
                            ),
                        ),
                        value.clone(),
                    )
                })
                .collect::<Vec<_>>();

            Some(ExecutionCondition::Constructed(
                std::sync::Arc::new(ExecutionCondition::Expression(expression)),
                fields.into(),
            ))
        }
        _ => None,
    }
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

pub(super) fn nullable_condition(
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    expression: BoundExpressionId,
    bound: &BoundExpression,
    value: &mut impl FnMut(BoundExpressionId) -> ExecutionCondition,
) -> Option<ExecutionCondition> {
    use bray_bound_tree::{BoundPatternKind, BoundStructuredExpressionKind};

    let BoundExpression::Structured(operation) = bound else {
        return None;
    };

    let absent = |expression| {
        let ty = semantics.types().expression(expression)?.ty();

        Some(ExecutionCondition::Literal(std::sync::Arc::new(
            bray_symbols::ConstantValueData::new(ty, ConstantValueKind::NullableAbsent),
        )))
    };

    match operation.kind() {
        BoundStructuredExpressionKind::Absence => absent(expression),
        BoundStructuredExpressionKind::PatternTest => {
            let ([subject], [pattern]) = (operation.operands(), operation.patterns()) else {
                return None;
            };

            if unit.view().pattern(*pattern)?.kind() != BoundPatternKind::NullableAbsent {
                return None;
            }

            Some(ExecutionCondition::operation(
                bray_bound_tree::BoundOperator::Equal,
                vec![value(*subject), absent(*subject)?],
            ))
        }
        _ => None,
    }
}

pub(super) fn conversion_condition(
    expression: bray_bound_tree::BoundExpressionId,
    bound: &bray_bound_tree::BoundExpression,
    semantics: &bray_bound_tree::CheckedExpressionSemantics,
    resolve: &mut impl FnMut(bray_bound_tree::BoundExpressionId) -> ExecutionCondition,
) -> Option<ExecutionCondition> {
    use bray_bound_tree::{
        BoundExpression, ConversionTarget, SelectedOperation, SemanticSelection,
    };

    let BoundExpression::Conversion(conversion) = bound else {
        return None;
    };

    let Some(SemanticSelection::Operation(SelectedOperation::Conversion(selected))) =
        semantics.selections().expression(expression)
    else {
        return None;
    };

    match selected.target() {
        ConversionTarget::Identity
        | ConversionTarget::CallableContract
        | ConversionTarget::BuiltInScalar => Some(resolve(conversion.operand())),
        ConversionTarget::NullablePresent => Some(resolve(conversion.operand()).present()),
        _ => None,
    }
}
