use bray_bound_tree::{BoundExpressionId, BoundStructuredExpression};
use bray_ir::{MirBlockId, MirBlockKind, MirEdge, MirTerminatorKind};

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
            panic!(
                "lowering contract violation: UnsupportedExpression {value:?}",
                value = id
            );
        };

        let ([body] | [body, _]) = expression.blocks() else {
            panic!(
                "lowering contract violation: UnsupportedExpression {value:?}",
                value = id
            );
        };

        let source = self.source(expression.origin());

        let header = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        self.set_terminator(
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
        let scope = self.begin_condition_scope(*condition_id);

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

        let mut completion = self.lower_block(*body, body_entry)?;

        if let (Some(scope), Some(block)) = (scope, completion.block) {
            completion.block =
                Some(self.finish_scope(scope.into(), block, &source, (*body).into())?);
        }

        self.finish_edge(completion, header)?;

        let exhausted = if let Some(scope) = scope {
            let completion =
                self.finish_scope(scope.into(), exhausted, &source, (*condition_id).into())?;

            self.active_scopes.pop();

            completion
        } else {
            exhausted
        };

        let exhausted = match expression.blocks().get(1).copied() {
            _ if !self.is_reachable(header, exhausted) => {
                self.finish_unreachable_blocks(&[exhausted], &source)?;

                LoweredExpression::terminated(Self::retained_source(&source))
            }
            Some(else_body) => self.lower_block(else_body, exhausted)?,
            None => LoweredExpression::continuing(
                exhausted,
                Some(self.unit_operand(result_type)),
                Self::retained_source(&source),
            ),
        };

        self.finish_result_edge(exhausted, join, result_type)?;

        self.loop_targets.pop();

        self.finish_result_join(header, join, result, source)
    }

    pub(super) fn lower_loop(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let [body] = expression.blocks() else {
            panic!(
                "lowering contract violation: UnsupportedExpression {value:?}",
                value = id
            );
        };

        let source = self.source(expression.origin());

        let header = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let (join, result, result_type) = self.push_result_join(id, expression.origin())?;

        self.set_terminator(
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

        self.finish_result_join(header, join, result, source)
    }

    pub(super) fn lower_with(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let retained = self.input_temporaries.len();
        let result = self.lower_with_body(id, expression, current);

        self.input_temporaries.truncate(retained);

        result
    }

    fn lower_with_body(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let entry = current;

        let [pattern] = expression.patterns() else {
            panic!(
                "lowering contract violation: UnsupportedExpression {value:?}",
                value = id
            );
        };

        let [initializer_id] = expression.operands() else {
            panic!(
                "lowering contract violation: UnsupportedExpression {value:?}",
                value = id
            );
        };

        let (join, result, result_type) = self.push_result_join(id, expression.origin())?;

        let initializer = self.lower_scope_enter(id, current, join, result_type)?;

        let Some(current) = initializer.block else {
            return self.finish_result_join(entry, join, result, initializer.source);
        };

        let Some(value) = initializer.value else {
            panic!(
                "lowering contract violation: MissingOperationResult {value:?}",
                value = *initializer_id
            );
        };

        let [body] = expression.blocks() else {
            panic!(
                "lowering contract violation: UnsupportedExpression {value:?}",
                value = id
            );
        };

        let current = self.bind_scoped_capability(id, *pattern, *body, value, current)?;

        let success_type = match self.input.semantic_selections().expression(id) {
            Some(bray_bound_tree::SemanticSelection::ScopedUse(scoped))
                if scoped.failure_type().is_some() =>
            {
                let ([success, _], _) = self.input.available_compiler_known_symbols()
                    .result_payloads(self.input.semantic_values(), result_type)
                    .expect("a fallible with expression retains its checked Result");

                Some(success)
            }
            _ => None,
        };

        let body = self.lower_yielding_block(
            *body,
            current,
            join,
            result_type,
            self.active_scopes.len(),
            success_type,
        )?;

        self.finish_result_edge(body, join, result_type)?;

        if !self.invocation_completes(id.into()) {
            let source = self.source(expression.origin());

            self.finish_unreachable_blocks(&[join], &source)?;

            return Ok(LoweredExpression::terminated(source));
        }

        self.finish_result_join(entry, join, result, self.source(expression.origin()))
    }
}
