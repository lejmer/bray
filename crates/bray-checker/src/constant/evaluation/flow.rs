use bray_bound_tree::{
    BoundBlockId, BoundBlockItem, BoundControlTransferKind, BoundExpression, BoundExpressionId,
    BoundStructuredExpression, BoundStructuredExpressionKind,
};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    AnyLocalSymbolId, ConstantTermData, ConstantTermId, ConstantValueKind, TypeId, UnionSymbolId,
};

use crate::CheckerRequestContext;
use crate::representation::type_representation;

use super::engine::Evaluator;
use super::support::EvaluationFailure;

pub(super) enum EvaluationFlow {
    Value(ConstantTermId),
    Yield {
        term: ConstantTermId,
        source_type: TypeId,
    },
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
        let bound =
            self.request
                .view()
                .expression(expression).unwrap_or_else(|| panic!("evaluate_flow requires checked expression type or node, expression: {expression:?}"));

        let evaluated = match bound {
            BoundExpression::Block(block) => {
                let ty = self.expression_type(expression);

                self.evaluate_block(block.block(), ty?)
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
                let (term, source_type) = match transfer.operand() {
                    Some(operand) => (self.evaluate(operand)?, self.expression_type(operand)?),
                    None => {
                        let term = self.unit_term(expression)?;

                        (term, self.expression_type(expression)?)
                    }
                };

                match transfer.kind() {
                    BoundControlTransferKind::Yield => Ok(EvaluationFlow::Yield {
                        term,
                        source_type: source_type,
                    }),
                    BoundControlTransferKind::Return => {
                        let result_type = self.input.result_type().unwrap_or(source_type);

                        let term = self.adapt_nullable_present(term, source_type, result_type)?;

                        Ok(EvaluationFlow::Return(term))
                    }
                    BoundControlTransferKind::Break | BoundControlTransferKind::Continue => {
                        Err(EvaluationFailure::invalid_expression(expression))
                    }
                }
            }
            BoundExpression::Match(matched) => self.evaluate_match(expression, matched),
            _ => self.evaluate_direct(expression).map(EvaluationFlow::Value),
        };

        let evaluated = match evaluated {
            Err(EvaluationFailure::Propagate(value)) => Ok(EvaluationFlow::Propagate(value)),
            evaluated => evaluated,
        }?;

        if self.input.destination() == crate::constant::input::ConstantDestination::Definition {
            let (term, known_type) = match evaluated {
                EvaluationFlow::Value(term) | EvaluationFlow::Propagate(term) => (term, None),
                EvaluationFlow::Return(term) => (term, self.input.result_type()),
                EvaluationFlow::Yield { term, source_type } => (term, Some(source_type)),
            };

            let result = if let Some(value) = self.term_value(term) {
                crate::constant::materialization::nonmaterializable_value_tree(
                    self.request.context(),
                    value,
                    &mut self.checked_materialization,
                    &mut self.diagnostics,
                )
            } else {
                let ty = match known_type {
                    Some(ty) => ty,
                    None => self.expression_type(expression)?,
                };

                crate::constant::materialization::nonmaterializable_type(
                    self.request.context(),
                    ty,
                    &mut self.diagnostics,
                )
            };

            if let Some(ty) = result.map_err(|error| self.record_query_failure(error))? {
                return Err(EvaluationFailure::Source {
                    expression,
                    diagnostic: crate::constant::diagnostic::ConstantDiagnostic::NonMaterializable(
                        ty,
                    ),
                });
            }
        }

        Ok(evaluated)
    }

    pub(super) fn evaluate_block(
        &mut self,
        block: BoundBlockId,
        result_type: TypeId,
    ) -> Result<EvaluationFlow, EvaluationFailure> {
        let block = self
            .request
            .view()
            .block(block).unwrap_or_else(|| panic!("evaluate_block requires checked input, block: {block:?}, result_type: {result_type:?}"));

        let initial = self.intern_value_term(result_type, ConstantValueKind::Unit);
        let mut result = (initial, result_type);

        for item in block.items() {
            self.observe_cancellation()?;

            match item {
                BoundBlockItem::LocalConstant(constant) => {
                    let Some(symbol) = constant.symbol() else {
                        return Err(EvaluationFailure::invalid_expression(
                            constant.initializer(),
                        ));
                    };

                    let value = self.evaluate(constant.initializer())?;

                    let value = if let Some(declared) = constant.declared_type().ty() {
                        let initializer_type = self.expression_type(constant.initializer());

                        self.adapt_nullable_present(value, initializer_type?, declared)?
                    } else {
                        value
                    };

                    self.locals.insert(AnyLocalSymbolId::from(symbol), value);
                }
                BoundBlockItem::LocalBinding(binding) => {
                    return Err(EvaluationFailure::invalid_expression(binding.initializer()));
                }
                BoundBlockItem::Expression(expression) => match self.evaluate_flow(*expression)? {
                    EvaluationFlow::Value(value) => {
                        result = (value, self.expression_type(*expression)?);
                    }
                    EvaluationFlow::Yield { term, source_type } => {
                        let term = self.adapt_nullable_present(term, source_type, result_type)?;

                        return Ok(EvaluationFlow::Value(term));
                    }
                    EvaluationFlow::Return(value) => {
                        return Ok(EvaluationFlow::Return(value));
                    }
                    EvaluationFlow::Propagate(value) => {
                        if let Some(value) = self.materialize_propagation(value, result_type) {
                            return Ok(EvaluationFlow::Value(value));
                        }

                        return Ok(EvaluationFlow::Propagate(value));
                    }
                },
            }
        }

        self.adapt_nullable_present(result.0, result.1, result_type)
            .map(EvaluationFlow::Value)
    }

    fn evaluate_conditional(
        &mut self,
        expression: BoundExpressionId,
        conditional: &BoundStructuredExpression,
    ) -> Result<EvaluationFlow, EvaluationFailure> {
        if conditional.operands().is_empty()
            || conditional.blocks().len() < conditional.operands().len()
        {
            return Err(EvaluationFailure::invalid_expression(expression));
        }

        let ty = self.expression_type(expression);

        for (condition, block) in conditional.operands().iter().zip(conditional.blocks()) {
            if self.boolean_value(*condition)? {
                return self.evaluate_block(*block, ty?);
            }
        }

        match conditional.blocks().get(conditional.operands().len()) {
            Some(block) => self.evaluate_block(*block, ty?),
            None => Ok(EvaluationFlow::Value(
                self.intern_value_term(ty?, ConstantValueKind::Unit),
            )),
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

        let condition = self
            .request
            .semantic_values()
            .constant_value_data(condition);

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
        let value = self.request.semantic_values().constant_value_data(value);

        match value.kind() {
            ConstantValueKind::NullablePresent(value) => Ok(EvaluationFlow::Value(
                self.intern_term(bray_symbols::ConstantTermData::Value(*value)),
            )),
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

        let representation = type_representation(self.request, self.expression_type(*operand)?);

        if representation != Some(RepresentationRole::Result) {
            return Err(EvaluationFailure::invalid_expression(expression));
        }

        let operand = self.evaluate(*operand)?;
        let value = self.closed_value(operand, expression)?;
        let value = self.request.semantic_values().constant_value_data(value);

        let ConstantValueKind::Union { variant, fields } = value.kind() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let (success, failure) = self.result_variants();

        if *variant == success {
            let [field] = fields.as_ref() else {
                return Err(EvaluationFailure::invalid_expression(expression));
            };

            return Ok(EvaluationFlow::Value(
                self.intern_term(ConstantTermData::Value(*field.value())),
            ));
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
    ) -> Option<ConstantTermId> {
        let Some(value) = self.term_value(propagated) else {
            return None;
        };

        let value = self.request.semantic_values().constant_value_data(value);

        if matches!(
            self.request
                .semantic_values()
                .type_data(result_type)
                .as_ref(),
            bray_symbols::TypeData::Nullable(_)
        ) && matches!(value.kind(), ConstantValueKind::NullableAbsent)
        {
            return Some(self.intern_value_term(result_type, ConstantValueKind::NullableAbsent));
        }

        if type_representation(self.request, result_type) == Some(RepresentationRole::Result)
            && let ConstantValueKind::Union { variant, .. } = value.kind()
        {
            let (_, failure) = self.result_variants();

            if *variant == failure {
                return Some(self.intern_value_term(result_type, value.kind().clone()));
            }
        }

        None
    }

    fn result_variants(
        &self,
    ) -> (
        bray_symbols::UnionVariantSymbolId,
        bray_symbols::UnionVariantSymbolId,
    ) {
        let Some(result) = self
            .request
            .available_compiler_known_symbols()
            .representation_symbol::<UnionSymbolId>(RepresentationRole::Result)
        else {
            panic!("Result representation must have a compiler-known union symbol");
        };

        let Some(result) = self.request.symbols().union(result) else {
            panic!("Result union symbol {result:?} must exist in the symbol graph");
        };

        let [success, failure] = result.variants() else {
            panic!(
                "Result union must have exactly success and failure variants, actual: {result:?}"
            );
        };

        (*success, *failure)
    }

    fn unit_term(
        &self,
        expression: BoundExpressionId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        Ok(self.intern_value_term(self.expression_type(expression)?, ConstantValueKind::Unit))
    }
}
