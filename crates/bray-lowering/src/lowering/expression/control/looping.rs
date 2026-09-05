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

        let body_entry = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let exhausted = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let (join, result, result_type) = self.push_result_join(id, expression.origin())?;

        let depth = self.active_scopes.len();
        let scope = self.begin_condition_scope(*condition_id)?;

        if !self.lower_condition(*condition_id, header, body_entry, exhausted)? {
            self.finish_unreachable_blocks(&[body_entry, exhausted], &source)?;

            if scope.is_some() {
                self.active_scopes.pop();
            }

            self.finish_unreachable_blocks(&[join], &source)?;

            return Ok(LoweredExpression::terminated(source));
        }

        self.loop_targets.push(LoopTarget {
            syntax: expression.origin().source_anchor().syntax(),
            continue_block: header,
            break_block: join,
            result_type,
            scope_depth: depth,
        });

        let mut completion = if self.builder.is_reachable(header, body_entry)? {
            self.lower_block(*body, body_entry)?
        } else {
            self.finish_unreachable_blocks(&[body_entry], &source)?;

            LoweredExpression::terminated(Self::retained_source(&source))
        };

        if let (Some(scope), Some(block)) = (scope, completion.block) {
            completion.block = Some(self.finish_scope(scope, block, &source, (*body).into())?);
        }

        self.finish_edge(completion, header)?;

        let exhausted = if let Some(scope) = scope {
            let completion =
                self.finish_scope(scope, exhausted, &source, (*condition_id).into())?;

            self.active_scopes.pop();

            completion
        } else {
            exhausted
        };

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

        let body =
            self.lower_yielding_block(*body, current, join, result_type, self.active_scopes.len())?;

        self.finish_result_edge(body, join, result_type)?;

        Ok(LoweredExpression::continuing(
            join,
            Some(MirOperand::Value(result)),
            self.source(expression.origin()),
        ))
    }
}
