use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundBlockId, BoundBlockItem, BoundExpression, BoundExpressionId, BoundLiteralKind,
    BoundStructuredExpressionKind,
};
use bray_symbols::{TypeData, TypeId};

use crate::{CheckerInfrastructureError, CheckerRequestContext, UnitCheckRequest};

use super::ExpressionTypeExpectation;
use super::dependencies::ExpressionTypeDependencies;
use super::inference::{InferenceTypeId, TypeInferenceContext};

pub(super) fn add_intrinsic_constraints(
    expression: &BoundExpression,
    expression_id: BoundExpressionId,
    variable: InferenceTypeId,
    types: &ExpressionTypeDependencies,
    inference: &mut TypeInferenceContext,
) {
    match expression {
        BoundExpression::Assignment(_) => {
            inference.add_evidence(variable, types.unit, expression_id);
        }
        BoundExpression::Literal(literal) => match literal.kind() {
            BoundLiteralKind::Boolean => {
                inference.add_evidence(variable, types.boolean, expression_id);
            }
            BoundLiteralKind::Character => {
                inference.add_evidence(variable, types.character, expression_id);
            }
            BoundLiteralKind::String => {
                inference.add_evidence(variable, types.string, expression_id);
            }
            BoundLiteralKind::Integer | BoundLiteralKind::Real | BoundLiteralKind::Imaginary => {}
        },
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

pub(super) fn add_relationship_constraints<C>(
    request: UnitCheckRequest<'_, C>,
    expressions: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    block_variables: &BTreeMap<BoundBlockId, InferenceTypeId>,
    types: &ExpressionTypeDependencies,
    inference: &mut TypeInferenceContext,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    for &expression_id in expressions {
        if request.is_cancelled() {
            return false;
        }

        let Some(expression) = request.view().expression(expression_id) else {
            continue;
        };

        let Some(variable) = variables.get(&expression_id).copied() else {
            continue;
        };

        match expression {
            BoundExpression::Block(block) => {
                if let Some(block_variable) = block_variables.get(&block.block()).copied() {
                    inference.unify(variable, block_variable, expression_id);
                }
            }
            BoundExpression::Structured(structured)
                if structured.kind() == BoundStructuredExpressionKind::Conditional =>
            {
                add_operand_expectation(
                    structured.operands().first().copied(),
                    Some(types.boolean),
                    variables,
                    inference,
                );

                for block in structured.blocks() {
                    if let Some(block_variable) = block_variables.get(block).copied() {
                        inference.unify(variable, block_variable, expression_id);
                    }
                }

                if structured.blocks().len() < 2 {
                    inference.add_evidence(variable, types.unit, expression_id);
                }
            }
            BoundExpression::Structured(structured)
                if structured.kind() == BoundStructuredExpressionKind::While =>
            {
                add_operand_expectation(
                    structured.operands().first().copied(),
                    Some(types.boolean),
                    variables,
                    inference,
                );

                if let Some(else_block) = structured.blocks().get(1)
                    && let Some(block_variable) = block_variables.get(else_block).copied()
                {
                    inference.unify(variable, block_variable, expression_id);
                } else {
                    inference.add_evidence(variable, types.unit, expression_id);
                }
            }
            BoundExpression::Structured(structured)
                if structured.kind() == BoundStructuredExpressionKind::Loop =>
            {
                inference.add_evidence(variable, types.never, expression_id);
            }
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

    true
}

pub(super) fn add_operand_expectation(
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

pub(super) fn block_expectations<C>(
    request: UnitCheckRequest<'_, C>,
    blocks: &[BoundBlockId],
) -> Option<Vec<ExpressionTypeExpectation>>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut expectations = Vec::new();

    for &block in blocks {
        if request.is_cancelled() {
            return None;
        }

        let Some(block) = request.view().block(block) else {
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

    Some(expectations)
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

pub(super) fn add_expectations<C>(
    request: UnitCheckRequest<'_, C>,
    expectations: impl IntoIterator<Item = ExpressionTypeExpectation>,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) -> Result<Option<()>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut pending = expectations.into_iter().collect::<Vec<_>>();

    while let Some(expectation) = pending.pop() {
        if request.is_cancelled() {
            return Ok(None);
        }

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

    Ok(Some(()))
}
