use bray_bound_tree::{
    BoundBlockId, BoundBlockItem, BoundControlTransferKind, BoundExpression, BoundExpressionId,
    BoundStructuredExpression, BoundStructuredExpressionKind,
};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    AnyLocalSymbolId, ConstantTermData, ConstantTermId, ConstantValueKind, TypeData, TypeId,
    UnionSymbolId,
};

use crate::representation::type_representation;
use crate::{CheckerInfrastructureError, CheckerRequestContext};

use super::engine::Evaluator;
use super::support::EvaluationFailure;

pub(super) enum EvaluationFlow {
    Value(ConstantTermId),
    Yield(ConstantTermId),
    Return(ConstantTermId),
    Propagate(ConstantTermId),
}

impl<'view, 'input, 'types, C> Evaluator<'view, 'input, 'types, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn evaluate_flow(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<EvaluationFlow, EvaluationFailure> {
        let bound = self
            .request
            .view()
            .expression(expression)
            .ok_or(EvaluationFailure::invalid_input())?;

        let evaluated = match bound {
            BoundExpression::Block(block) => {
                let ty = self.expression_type(expression)?;

                self.evaluate_block(block.block(), ty)
            }
            BoundExpression::Structured(structured)
                if structured.kind() == BoundStructuredExpressionKind::Conditional =>
            {
                self.evaluate_conditional(expression, structured)
            }
            BoundExpression::Structured(structured)
                if structured.kind() == BoundStructuredExpressionKind::TrustBoundary =>
            {
                let [operand] = structured.operands() else {
                    return Err(EvaluationFailure::invalid_expression(expression));
                };

                self.evaluate_flow(*operand)
            }
            BoundExpression::Structured(structured)
                if structured.kind() == BoundStructuredExpressionKind::Assertion =>
            {
                self.evaluate_assertion(expression, structured)
            }
            BoundExpression::Structured(structured)
                if structured.kind() == BoundStructuredExpressionKind::NullablePropagation =>
            {
                self.evaluate_nullable_propagation(expression, structured)
            }
            BoundExpression::Structured(structured)
                if structured.kind() == BoundStructuredExpressionKind::ResultPropagation =>
            {
                self.evaluate_result_propagation(expression, structured)
            }
            BoundExpression::ControlTransfer(transfer) => {
                let term = match transfer.operand() {
                    Some(operand) => self.evaluate(operand)?,
                    None => self.unit_term(expression)?,
                };

                match transfer.kind() {
                    BoundControlTransferKind::Yield => Ok(EvaluationFlow::Yield(term)),
                    BoundControlTransferKind::Return => Ok(EvaluationFlow::Return(term)),
                    BoundControlTransferKind::Break | BoundControlTransferKind::Continue => {
                        Err(EvaluationFailure::invalid_expression(expression))
                    }
                }
            }
            BoundExpression::Match(matched) => self.evaluate_match(expression, matched),
            _ => self.evaluate_direct(expression).map(EvaluationFlow::Value),
        };

        match evaluated {
            Err(EvaluationFailure::Propagate(value)) => Ok(EvaluationFlow::Propagate(value)),
            evaluated => evaluated,
        }
    }

    pub(super) fn evaluate_block(
        &mut self,
        block: BoundBlockId,
        result_type: TypeId,
    ) -> Result<EvaluationFlow, EvaluationFailure> {
        let block = self
            .request
            .view()
            .block(block)
            .ok_or(EvaluationFailure::invalid_input())?;

        let mut result = self.intern_value_term(result_type, ConstantValueKind::Unit)?;

        for item in block.items() {
            self.observe_cancellation()?;

            match item {
                BoundBlockItem::LocalConstant(constant) => {
                    let Some(symbol) = constant.symbol() else {
                        return Err(EvaluationFailure::invalid_expression(
                            constant.initializer(),
                        ));
                    };

                    let mut value = self.evaluate(constant.initializer())?;

                    if let Some(declared) = constant.declared_type().ty() {
                        let initializer_type = self.expression_type(constant.initializer())?;

                        let declared_data = self
                            .request
                            .semantic_values()
                            .type_data(declared)
                            .map_err(|_| {
                                EvaluationFailure::Infrastructure(
                                    CheckerInfrastructureError::SemanticValueUnavailable,
                                )
                            })?;

                        if matches!(declared_data.as_ref(), TypeData::Nullable(contained) if *contained == initializer_type)
                        {
                            value = self.intern_typed_term(
                                declared,
                                ConstantTermData::NullablePresent(value),
                            )?;
                        }
                    }

                    self.locals.insert(AnyLocalSymbolId::from(symbol), value);
                }
                BoundBlockItem::LocalBinding(binding) => {
                    return Err(EvaluationFailure::invalid_expression(binding.initializer()));
                }
                BoundBlockItem::Expression(expression) => match self.evaluate_flow(*expression)? {
                    EvaluationFlow::Value(value) => result = value,
                    EvaluationFlow::Yield(value) => {
                        return Ok(EvaluationFlow::Value(value));
                    }
                    EvaluationFlow::Return(value) => {
                        return Ok(EvaluationFlow::Return(value));
                    }
                    EvaluationFlow::Propagate(value) => {
                        if let Some(value) = self.materialize_propagation(value, result_type)? {
                            return Ok(EvaluationFlow::Value(value));
                        }

                        return Ok(EvaluationFlow::Propagate(value));
                    }
                },
            }
        }

        Ok(EvaluationFlow::Value(result))
    }

    fn evaluate_conditional(
        &mut self,
        expression: BoundExpressionId,
        conditional: &BoundStructuredExpression,
    ) -> Result<EvaluationFlow, EvaluationFailure> {
        let [condition] = conditional.operands() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let condition = self.evaluate(*condition)?;
        let condition = self.closed_value(condition, expression)?;
        let condition = self.constant_value(condition)?;

        let ConstantValueKind::Boolean(condition) = condition.kind() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let ty = self.expression_type(expression)?;

        match (*condition, conditional.blocks()) {
            (true, [then, ..]) => self.evaluate_block(*then, ty),
            (false, [_, otherwise, ..]) => self.evaluate_block(*otherwise, ty),
            (false, [_]) | (false, []) => self
                .intern_value_term(ty, ConstantValueKind::Unit)
                .map(EvaluationFlow::Value),
            (true, []) => Err(EvaluationFailure::invalid_expression(expression)),
        }
    }

    fn evaluate_assertion(
        &mut self,
        expression: BoundExpressionId,
        assertion: &BoundStructuredExpression,
    ) -> Result<EvaluationFlow, EvaluationFailure> {
        let Some(condition) = assertion.operands().first().copied() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let condition = self.evaluate(condition)?;
        let condition = self.closed_value(condition, expression)?;
        let condition = self.constant_value(condition)?;

        match condition.kind() {
            ConstantValueKind::Boolean(true) => {
                self.unit_term(expression).map(EvaluationFlow::Value)
            }
            ConstantValueKind::Boolean(false) => {
                Err(EvaluationFailure::invalid_expression(expression))
            }
            _ => Err(EvaluationFailure::invalid_expression(expression)),
        }
    }

    fn evaluate_nullable_propagation(
        &mut self,
        expression: BoundExpressionId,
        propagation: &BoundStructuredExpression,
    ) -> Result<EvaluationFlow, EvaluationFailure> {
        let [operand] = propagation.operands() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let operand = self.evaluate(*operand)?;
        let value = self.closed_value(operand, expression)?;
        let value = self.constant_value(value)?;

        match value.kind() {
            ConstantValueKind::NullablePresent(value) => self
                .intern_term(bray_symbols::ConstantTermData::Value(*value))
                .map(EvaluationFlow::Value),
            ConstantValueKind::NullableAbsent => Ok(EvaluationFlow::Propagate(operand)),
            _ => Err(EvaluationFailure::invalid_expression(expression)),
        }
    }

    fn evaluate_result_propagation(
        &mut self,
        expression: BoundExpressionId,
        propagation: &BoundStructuredExpression,
    ) -> Result<EvaluationFlow, EvaluationFailure> {
        let [operand] = propagation.operands() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let representation = type_representation(self.request, self.expression_type(*operand)?)
            .map_err(EvaluationFailure::Infrastructure)?;

        if representation != Some(RepresentationRole::Result) {
            return Err(EvaluationFailure::invalid_expression(expression));
        }

        let operand = self.evaluate(*operand)?;
        let value = self.closed_value(operand, expression)?;
        let value = self.constant_value(value)?;

        let ConstantValueKind::Union { variant, fields } = value.kind() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let (success, failure) = self.result_variants()?;

        if *variant == success {
            let [field] = fields.as_ref() else {
                return Err(EvaluationFailure::invalid_expression(expression));
            };

            return self
                .intern_term(ConstantTermData::Value(*field.value()))
                .map(EvaluationFlow::Value);
        }

        if *variant == failure {
            return Ok(EvaluationFlow::Propagate(operand));
        }

        Err(EvaluationFailure::invalid_expression(expression))
    }

    pub(super) fn materialize_propagation(
        &self,
        propagated: ConstantTermId,
        result_type: TypeId,
    ) -> Result<Option<ConstantTermId>, EvaluationFailure> {
        let Some(value) = self.term_value(propagated)? else {
            return Ok(None);
        };

        let value = self.constant_value(value)?;

        if self.is_nullable_type(result_type)?
            && matches!(value.kind(), ConstantValueKind::NullableAbsent)
        {
            return self
                .intern_value_term(result_type, ConstantValueKind::NullableAbsent)
                .map(Some);
        }

        if type_representation(self.request, result_type)
            .map_err(EvaluationFailure::Infrastructure)?
            == Some(RepresentationRole::Result)
            && let ConstantValueKind::Union { variant, .. } = value.kind()
        {
            let (_, failure) = self.result_variants()?;

            if *variant == failure {
                return self
                    .intern_value_term(result_type, value.kind().clone())
                    .map(Some);
            }
        }

        Ok(None)
    }

    fn is_nullable_type(&self, ty: TypeId) -> Result<bool, EvaluationFailure> {
        self.request
            .semantic_values()
            .type_data(ty)
            .map(|ty| matches!(ty.as_ref(), bray_symbols::TypeData::Nullable(_)))
            .map_err(|_| {
                EvaluationFailure::Infrastructure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                )
            })
    }

    fn result_variants(
        &self,
    ) -> Result<
        (
            bray_symbols::UnionVariantSymbolId,
            bray_symbols::UnionVariantSymbolId,
        ),
        EvaluationFailure,
    > {
        let Some(result) = self
            .request
            .available_compiler_known_symbols()
            .representation_symbol::<UnionSymbolId>(RepresentationRole::Result)
        else {
            return Err(EvaluationFailure::Infrastructure(
                CheckerInfrastructureError::SemanticValueUnavailable,
            ));
        };

        let Some(result) = self.request.symbols().union(result) else {
            return Err(EvaluationFailure::Infrastructure(
                CheckerInfrastructureError::SemanticValueUnavailable,
            ));
        };

        let [success, failure] = result.variants() else {
            return Err(EvaluationFailure::Infrastructure(
                CheckerInfrastructureError::SemanticValueUnavailable,
            ));
        };

        Ok((*success, *failure))
    }

    fn unit_term(
        &self,
        expression: BoundExpressionId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        self.intern_value_term(self.expression_type(expression)?, ConstantValueKind::Unit)
    }
}
