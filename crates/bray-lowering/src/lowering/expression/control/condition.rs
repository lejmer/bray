use bray_bound_tree::{
    BoundBlockId, BoundExpression, BoundExpressionId, BoundOperator, BoundStructuredExpression,
    BoundStructuredExpressionKind,
};
use bray_ir::{
    MirBlockId, MirBlockKind, MirEdge, MirImmediateValue, MirOperand, MirTerminatorKind,
};
use bray_symbols::ConstantValueKind;

use super::super::super::LoweringError;
use super::super::super::block::LoweredExpression;
use super::super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn lower_condition(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
        matched: MirBlockId,
        unmatched: MirBlockId,
    ) -> Result<bool, LoweringError> {
        let expression = self
            .input
            .unit()
            .view()
            .expression(id)
            .ok_or(LoweringError::MissingBoundNode(id.into()))?;

        if let BoundExpression::Structured(condition) = expression
            && condition.kind() == BoundStructuredExpressionKind::Condition
        {
            let [operand] = condition.operands() else {
                return Err(LoweringError::UnsupportedExpression(id));
            };

            return self.lower_condition(*operand, current, matched, unmatched);
        }

        if let BoundExpression::Binary(binary) = expression
            && binary.operator() == BoundOperator::LogicalAnd
        {
            let [left, right] = binary.operands() else {
                return Err(LoweringError::UnsupportedExpression(id));
            };

            let (left, right) = (*left, *right);

            let source = self.expression_source(id)?;

            let next = self
                .builder
                .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

            if !self.lower_condition(left, current, next, unmatched)? {
                self.finish_unreachable_blocks(&[next], &source)?;

                return Ok(false);
            }

            self.lower_condition(right, next, matched, unmatched)?;

            return Ok(self.builder.is_reachable(current, matched)?
                || self.builder.is_reachable(current, unmatched)?);
        }

        if let BoundExpression::Structured(test) = expression
            && test.kind() == BoundStructuredExpressionKind::PatternBinding
        {
            return self.lower_pattern_condition(id, test, current, matched, unmatched);
        }

        let condition = self.lower_expression(id, current)?;

        let Some(current) = condition.block else {
            return Ok(false);
        };

        let Some(value) = condition.value else {
            return Err(LoweringError::MissingOperationResult(id));
        };

        let terminator = match self.constant_boolean(&value)? {
            Some(true) => MirTerminatorKind::Goto(MirEdge::new(matched, [])),
            Some(false) => MirTerminatorKind::Goto(MirEdge::new(unmatched, [])),
            None => MirTerminatorKind::Branch {
                condition: value,
                then_edge: MirEdge::new(matched, []),
                else_edge: MirEdge::new(unmatched, []),
            },
        };

        self.set_terminator(current, condition.source, terminator)?;

        Ok(true)
    }

    pub(super) fn begin_condition_scope(
        &mut self,
        id: BoundExpressionId,
    ) -> Result<Option<BoundBlockId>, LoweringError> {
        let Some(BoundExpression::Structured(test)) = self.input.unit().view().expression(id)
        else {
            return Ok(None);
        };

        if test.kind() != BoundStructuredExpressionKind::Condition {
            return Ok(None);
        }

        let [scope] = test.blocks() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let scope = *scope;

        self.active_scopes.push(scope);

        Ok(Some(scope))
    }

    fn lower_pattern_condition(
        &mut self,
        id: BoundExpressionId,
        test: &BoundStructuredExpression,
        current: MirBlockId,
        matched: MirBlockId,
        unmatched: MirBlockId,
    ) -> Result<bool, LoweringError> {
        let ([subject_id], [pattern]) = (test.operands(), test.patterns()) else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let subject = self.lower_expression(*subject_id, current)?;
        let subject = self.materialize_match_subject(*subject_id, subject)?;

        let Some(current) = subject.block else {
            return Ok(false);
        };

        let Some(value) = subject.value else {
            return Err(LoweringError::MissingOperationResult(*subject_id));
        };

        self.lower_pattern_branch(*pattern, value, current, matched, unmatched)?;

        Ok(true)
    }

    pub(super) fn finish_unreachable_blocks(
        &mut self,
        blocks: &[MirBlockId],
        source: &bray_ir::MirSourceAnchor,
    ) -> Result<(), LoweringError> {
        for block in blocks {
            self.set_terminator(
                *block,
                Self::retained_source(source),
                MirTerminatorKind::Unreachable,
            )?;
        }

        Ok(())
    }

    pub(super) fn lower_pattern_test(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let source = self.source(expression.origin());

        let [scope] = expression.blocks() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        self.active_scopes.push(*scope);

        let matched = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let unmatched = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        if !self.lower_pattern_condition(id, expression, current, matched, unmatched)? {
            self.finish_unreachable_blocks(&[matched, unmatched], &source)?;
            self.active_scopes.pop();

            return Ok(LoweredExpression::terminated(source));
        }

        let (join, result, ty) = self.push_result_join(id, expression.origin())?;

        for (block, value) in [(matched, true), (unmatched, false)] {
            let value = Self::immediate_operand(ty, MirImmediateValue::Boolean(value));

            self.set_terminator(
                block,
                Self::retained_source(&source),
                MirTerminatorKind::Goto(MirEdge::new(join, [value])),
            )?;
        }

        let (continuation, completed_result, _) = self.push_result_join(id, expression.origin())?;

        self.finish_exit_to_block(
            join,
            &source,
            self.active_scopes.len().saturating_sub(1),
            continuation,
            Some((MirOperand::Value(result), ty)),
            id.into(),
        )?;

        self.active_scopes.pop();

        Ok(LoweredExpression::continuing(
            continuation,
            Some(MirOperand::Value(completed_result)),
            source,
        ))
    }

    pub(super) fn constant_boolean(
        &self,
        operand: &MirOperand,
    ) -> Result<Option<bool>, LoweringError> {
        let MirOperand::Constant { value, .. } = operand else {
            return Ok(None);
        };

        let data = self.input.semantic_values().constant_value_data(*value)?;

        match data.kind() {
            ConstantValueKind::Boolean(value) => Ok(Some(*value)),
            _ => Ok(None),
        }
    }
}
