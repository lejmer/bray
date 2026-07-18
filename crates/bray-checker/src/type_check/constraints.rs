use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundBlockId, BoundBlockItem, BoundExpression, BoundExpressionId,
    BoundStructuredExpressionKind, BoundUnitView,
};
use bray_symbols::{TypeData, TypeId};

use crate::{CheckerInfrastructureError, CheckerRequestContext, UnitCheckRequest};

use super::canonical::CanonicalTypes;
use super::inference::{InferenceTypeId, TypeInferenceContext};
use super::{ExpressionTypeEvidence, ExpressionTypeExpectation, ExpressionTypeInput};

pub(super) fn add_intrinsic_constraints(
    expression: &BoundExpression,
    expression_id: BoundExpressionId,
    variable: InferenceTypeId,
    types: &CanonicalTypes,
    inference: &mut TypeInferenceContext,
) {
    match expression {
        BoundExpression::Assignment(_) => {
            inference.add_evidence(variable, types.unit, expression_id);
        }
        BoundExpression::Conversion(conversion) => {
            if let Some(target) = conversion.target_type() {
                inference.add_evidence(variable, target, expression_id);
            }
        }
        BoundExpression::ControlTransfer(_) => {
            inference.add_evidence(variable, types.never, expression_id);
        }
        BoundExpression::Structured(structured) => match structured.kind() {
            BoundStructuredExpressionKind::Unit | BoundStructuredExpressionKind::Assertion => {
                inference.add_evidence(variable, types.unit, expression_id);
            }
            BoundStructuredExpressionKind::Panic => {
                inference.add_evidence(variable, types.never, expression_id);
            }
            _ => {}
        },
        _ => {}
    }
}

pub(super) fn add_relationship_constraints(
    view: BoundUnitView<'_>,
    expressions: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    types: &CanonicalTypes,
    inference: &mut TypeInferenceContext,
) {
    for &expression_id in expressions {
        let Some(expression) = view.expression(expression_id) else {
            continue;
        };

        let Some(variable) = variables.get(&expression_id).copied() else {
            continue;
        };

        match expression {
            BoundExpression::Structured(structured)
                if structured.kind() == BoundStructuredExpressionKind::TrustBoundary =>
            {
                if let Some(operand) = structured.operands().first()
                    && let Some(operand) = variables.get(operand).copied()
                {
                    inference.unify(variable, operand, expression_id);
                }
            }
            BoundExpression::Structured(structured)
                if structured.kind() == BoundStructuredExpressionKind::BooleanFold =>
            {
                inference.add_evidence(variable, types.boolean, expression_id);
            }
            BoundExpression::Structured(structured)
                if matches!(
                    structured.kind(),
                    BoundStructuredExpressionKind::Conditional
                        | BoundStructuredExpressionKind::While
                ) =>
            {
                add_operand_expectation(
                    structured.operands().first().copied(),
                    Some(types.boolean),
                    variables,
                    inference,
                );
            }
            BoundExpression::Structured(structured)
                if structured.kind() == BoundStructuredExpressionKind::Assertion =>
            {
                add_operand_expectation(
                    structured.operands().first().copied(),
                    Some(types.boolean),
                    variables,
                    inference,
                );
                add_operand_expectation(
                    structured.operands().get(1).copied(),
                    Some(types.string),
                    variables,
                    inference,
                );
            }
            BoundExpression::Structured(structured)
                if structured.kind() == BoundStructuredExpressionKind::Panic =>
            {
                add_operand_expectation(
                    structured.operands().first().copied(),
                    Some(types.string),
                    variables,
                    inference,
                );
            }
            _ => {}
        }
    }
}

fn add_operand_expectation(
    expression: Option<BoundExpressionId>,
    expected: Option<TypeId>,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) {
    let Some(expected) = expected else {
        return;
    };

    let Some(expression) = expression else {
        return;
    };

    let Some(variable) = variables.get(&expression).copied() else {
        return;
    };

    inference.add_expectation(variable, expected, expression);
}

pub(super) fn block_expectations(
    view: BoundUnitView<'_>,
    blocks: &[BoundBlockId],
) -> Vec<ExpressionTypeExpectation> {
    let mut expectations = Vec::new();

    for &block in blocks {
        let Some(block) = view.block(block) else {
            continue;
        };

        for item in block.items() {
            match item {
                BoundBlockItem::LocalBinding(binding) => {
                    let expected = binding.declared_type().and_then(|reference| reference.ty());

                    push_expected_initializer(&mut expectations, binding.initializer(), expected);
                }
                BoundBlockItem::LocalConstant(constant) => {
                    push_expected_initializer(
                        &mut expectations,
                        constant.initializer(),
                        constant.declared_type().ty(),
                    );
                }
                BoundBlockItem::Expression(_) => {}
            }
        }
    }

    expectations
}

fn push_expected_initializer(
    expectations: &mut Vec<ExpressionTypeExpectation>,
    expression: BoundExpressionId,
    expected: Option<TypeId>,
) {
    let Some(expected) = expected else {
        return;
    };

    expectations.push(ExpressionTypeExpectation::new(expression, expected));
}

pub(super) fn add_input_evidence(
    input: &ExpressionTypeInput,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) {
    for evidence in input.evidence() {
        add_evidence(*evidence, variables, inference);
    }
}

fn add_evidence(
    evidence: ExpressionTypeEvidence,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) {
    if let Some(variable) = variables.get(&evidence.expression()).copied() {
        inference.add_evidence(variable, evidence.ty(), evidence.expression());
    }
}

pub(super) fn add_expectations<C>(
    request: UnitCheckRequest<'_, C>,
    expectations: impl IntoIterator<Item = ExpressionTypeExpectation>,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut pending = expectations.into_iter().collect::<Vec<_>>();

    while let Some(expectation) = pending.pop() {
        let Some(variable) = variables.get(&expectation.expression()).copied() else {
            continue;
        };

        inference.add_expectation(variable, expectation.ty(), expectation.expression());

        let Some(expression) = request.view().expression(expectation.expression()) else {
            continue;
        };

        let data = request
            .semantic_values()
            .type_data(expectation.ty())
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        match (expression, data.as_ref()) {
            (BoundExpression::Structured(structured), TypeData::Tuple(expected_elements))
                if structured.kind() == BoundStructuredExpressionKind::Tuple
                    && structured.operands().len() == expected_elements.len() =>
            {
                pending.extend(
                    structured
                        .operands()
                        .iter()
                        .copied()
                        .zip(expected_elements.iter().copied())
                        .map(|(expression, ty)| ExpressionTypeExpectation::new(expression, ty)),
                );
            }
            (BoundExpression::Structured(structured), TypeData::Array { element, .. })
                if structured.kind() == BoundStructuredExpressionKind::Array =>
            {
                pending.extend(
                    structured
                        .operands()
                        .iter()
                        .copied()
                        .map(|expression| ExpressionTypeExpectation::new(expression, *element)),
                );
            }
            (BoundExpression::Structured(structured), _)
                if structured.kind() == BoundStructuredExpressionKind::TrustBoundary =>
            {
                pending.extend(structured.operands().first().copied().map(|expression| {
                    ExpressionTypeExpectation::new(expression, expectation.ty())
                }));
            }
            _ => {}
        }
    }

    Ok(())
}

pub(super) fn infer_tuples<C>(
    request: UnitCheckRequest<'_, C>,
    expressions: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    for &expression_id in expressions {
        let Some(BoundExpression::Structured(expression)) =
            request.view().expression(expression_id)
        else {
            continue;
        };

        if expression.kind() != BoundStructuredExpressionKind::Tuple {
            continue;
        }

        let mut elements = Vec::with_capacity(expression.operands().len());

        for operand in expression.operands() {
            let Some(variable) = variables.get(operand).copied() else {
                elements.clear();
                break;
            };

            let Some(ty) = inference.evidence(variable) else {
                elements.clear();
                break;
            };

            elements.push(ty);
        }

        if elements.len() != expression.operands().len() {
            continue;
        }

        let ty = request
            .semantic_values()
            .intern_type(TypeData::tuple(elements))
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        let Some(variable) = variables.get(&expression_id).copied() else {
            continue;
        };

        inference.add_evidence(variable, ty, expression_id);
    }

    Ok(())
}
