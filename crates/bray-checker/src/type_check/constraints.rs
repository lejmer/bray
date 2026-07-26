use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundBlockId, BoundBlockItem, BoundControlTransferKind, BoundExpression, BoundExpressionId,
    BoundLiteralKind, BoundStructuredExpressionKind,
};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{GenericArgument, TypeData, TypeId};

use crate::representation::type_representation;
use crate::{
    CheckerInfrastructureError, CheckerRequestContext, CheckerUnitRoot, CheckerUnitView,
    SemanticUnitContext,
};

use super::ExpressionTypeExpectation;
use super::dependencies::ExpressionTypeDependencies;
use super::inference::{InferenceTypeId, TypeInferenceContext};
use super::region::{ExpressionTypeRegions, ResultRegionKind};

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
        BoundExpression::Generator(_) => {
            inference.add_evidence(variable, types.unit, expression_id);
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

pub(super) fn add_semantic_context_constraints<C>(
    request: CheckerUnitView<'_, C>,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    boolean: TypeId,
    inference: &mut TypeInferenceContext,
) where
    C: CheckerRequestContext + ?Sized,
{
    match (request.semantic_context(), request.root()) {
        (
            SemanticUnitContext::PredicateDefinition(_) | SemanticUnitContext::TargetGate(_),
            CheckerUnitRoot::Expression(expression),
        ) => add_operand_expectation(Some(expression), Some(boolean), variables, inference),
        (
            SemanticUnitContext::Constraint(_) | SemanticUnitContext::ContractClause(_),
            CheckerUnitRoot::ExpressionSequence(block),
        ) => {
            let Some(block) = request.view().block(block) else {
                return;
            };

            for expression in block.items().iter().filter_map(|item| item.expression()) {
                add_operand_expectation(Some(expression), Some(boolean), variables, inference);
            }
        }
        (
            SemanticUnitContext::CallableBody(_)
            | SemanticUnitContext::AnonymousCallable(_)
            | SemanticUnitContext::RuntimeDefault(_)
            | SemanticUnitContext::ConstantTemplate(_)
            | SemanticUnitContext::EmbeddedConstant(_),
            _,
        )
        | (
            SemanticUnitContext::PredicateDefinition(_)
            | SemanticUnitContext::Constraint(_)
            | SemanticUnitContext::ContractClause(_)
            | SemanticUnitContext::TargetGate(_),
            _,
        ) => {}
    }
}

pub(super) fn add_relationship_constraints<C>(
    request: CheckerUnitView<'_, C>,
    expressions: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    block_variables: &BTreeMap<BoundBlockId, InferenceTypeId>,
    regions: &ExpressionTypeRegions,
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
            BoundExpression::ControlTransfer(transfer) => {
                let ty = match transfer.kind() {
                    BoundControlTransferKind::Yield => transfer
                        .target()
                        .and_then(|target| regions.result(target))
                        .map_or(types.never, |region| match region.kind() {
                            ResultRegionKind::SingleYield => types.never,
                            ResultRegionKind::ArrayGenerator
                            | ResultRegionKind::GeneralGenerator => types.unit,
                        }),
                    BoundControlTransferKind::Return
                    | BoundControlTransferKind::Break
                    | BoundControlTransferKind::Continue => types.never,
                };

                inference.add_evidence(variable, ty, expression_id);
            }
            BoundExpression::Block(block) => {
                if let Some(block_variable) = block_variables.get(&block.block()).copied() {
                    inference.unify(variable, block_variable, expression_id);
                }
            }
            BoundExpression::Match(expression) => {
                for arm in expression.arms() {
                    add_operand_expectation(arm.guard(), Some(types.boolean), variables, inference);

                    if let Some(block_variable) = block_variables.get(&arm.body()).copied() {
                        inference.unify(variable, block_variable, expression_id);
                    }
                }
            }
            BoundExpression::For(expression) => {
                if let Some(else_block) = expression.else_body()
                    && let Some(block_variable) = block_variables.get(&else_block).copied()
                {
                    inference.unify(variable, block_variable, expression_id);
                } else {
                    inference.add_evidence(variable, types.unit, expression_id);
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
                if structured.kind() == BoundStructuredExpressionKind::With =>
            {
                if let Some(block) = structured.blocks().first()
                    && let Some(block_variable) = block_variables.get(block).copied()
                {
                    inference.unify(variable, block_variable, expression_id);
                }
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
                if matches!(
                    structured.kind(),
                    BoundStructuredExpressionKind::BooleanAllFold
                        | BoundStructuredExpressionKind::BooleanAnyFold
                ) =>
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
                if structured.kind() == BoundStructuredExpressionKind::ResultPropagation =>
            {
                if let Some(ty) = propagation_output_type(request, structured) {
                    inference.add_evidence(variable, ty, expression_id);
                }
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

fn propagation_output_type<C>(
    request: CheckerUnitView<'_, C>,
    propagation: &bray_bound_tree::BoundStructuredExpression,
) -> Option<TypeId>
where
    C: CheckerRequestContext + ?Sized,
{
    let operand = propagation.operands().first().copied()?;
    let operand_type = request.view().expression(operand)?.ty()?;
    let data = request.semantic_values().type_data(operand_type).ok()?;

    let TypeData::Named { substitution, .. } = data.as_ref() else {
        return None;
    };

    let role = type_representation(request, operand_type).ok()??;

    if !matches!(
        role,
        RepresentationRole::Result | RepresentationRole::RunResult
    ) {
        return None;
    }

    let substitution = request
        .semantic_values()
        .generic_substitution_data(*substitution)
        .ok()?;

    match substitution.bindings().first()?.argument() {
        GenericArgument::Type(ty) => Some(ty),
        GenericArgument::Constant(_) => None,
    }
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
    request: CheckerUnitView<'_, C>,
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
    request: CheckerUnitView<'_, C>,
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
