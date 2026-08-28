use bray_bound_tree::{BoundExpressionId, BoundStructuredExpression};
use bray_ir::{MirBlockId, MirBlockKind, MirEdge, MirOperand, MirTerminatorKind};

use super::super::super::LoweringError;
use super::super::super::block::LoweredExpression;
use super::super::super::lowerer::{LoopTarget, Lowerer};

impl Lowerer<'_> {
    pub(super) fn lower_while(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let [condition_id] = expression.operands() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let ([body] | [body, _]) = expression.blocks() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let source = self.source(expression.origin());

        let header = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        self.builder.set_terminator(
            current,
            Self::retained_source(&source),
            MirTerminatorKind::Goto(MirEdge::new(header, [])),
        )?;

        let condition = self.lower_expression(*condition_id, header)?;

        let Some(condition_block) = condition.block else {
            return Ok(condition);
        };

        let Some(condition) = condition.value else {
            return Err(LoweringError::MissingOperationResult(*condition_id));
        };

        let body_entry = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let exhausted = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let (join, result, result_type) = self.push_result_join(id, expression.origin())?;

        let terminator = if self.condition_is_always_true(&condition)? {
            MirTerminatorKind::Goto(MirEdge::new(body_entry, []))
        } else {
            MirTerminatorKind::Branch {
                condition,
                then_edge: MirEdge::new(body_entry, []),
                else_edge: MirEdge::new(exhausted, []),
            }
        };

        self.builder
            .set_terminator(condition_block, Self::retained_source(&source), terminator)?;

        self.loop_targets.push(LoopTarget {
            syntax: expression.origin().source_anchor().syntax(),
            continue_block: header,
            break_block: join,
            result_type,
            scope_depth: self.active_scopes.len(),
        });

        let body = self.lower_block(*body, body_entry)?;
        self.finish_edge(body, header)?;

        let exhausted = match expression.blocks().get(1).copied() {
            Some(else_body) => self.lower_block(else_body, exhausted)?,
            None => LoweredExpression::continuing(
                exhausted,
                Some(self.unit_operand(result_type)),
                Self::retained_source(&source),
            ),
        };

        self.finish_result_edge(exhausted, join, result_type)?;

        self.loop_targets.pop();

        Ok(LoweredExpression::continuing(
            join,
            Some(MirOperand::Value(result)),
            source,
        ))
    }

    fn condition_is_always_true(&self, condition: &MirOperand) -> Result<bool, LoweringError> {
        Ok(self.constant_boolean(condition)? == Some(true))
    }

    pub(super) fn lower_loop(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let [body] = expression.blocks() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let source = self.source(expression.origin());

        let header = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let (join, result, result_type) = self.push_result_join(id, expression.origin())?;

        self.builder.set_terminator(
            current,
            Self::retained_source(&source),
            MirTerminatorKind::Goto(MirEdge::new(header, [])),
        )?;

        self.loop_targets.push(LoopTarget {
            syntax: expression.origin().source_anchor().syntax(),
            continue_block: header,
            break_block: join,
            result_type,
            scope_depth: self.active_scopes.len(),
        });

        let body = self.lower_block(*body, header)?;
        self.finish_edge(body, header)?;

        self.loop_targets.pop();

        Ok(LoweredExpression::continuing(
            join,
            Some(MirOperand::Value(result)),
            source,
        ))
    }

    pub(super) fn lower_with(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let [pattern] = expression.patterns() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let [initializer_id] = expression.operands() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let initializer = self.lower_expression(*initializer_id, current)?;

        let Some(current) = initializer.block else {
            return Ok(initializer);
        };

        let Some(value) = initializer.value else {
            return Err(LoweringError::MissingOperationResult(*initializer_id));
        };

        let current = self.lower_pattern_bindings(*pattern, value, current)?;

        let [body] = expression.blocks() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let (join, result, result_type) = self.push_result_join(id, expression.origin())?;

        let body = self.lower_yielding_block(*body, current, join, result_type)?;

        self.finish_result_edge(body, join, result_type)?;

        Ok(LoweredExpression::continuing(
            join,
            Some(MirOperand::Value(result)),
            self.source(expression.origin()),
        ))
    }
}
