use bray_bound_tree::{
    BoundBlockId, BoundExpressionId, BoundOperator, BoundStructuredExpressionKind,
};

use crate::CheckerRequestContext;

use super::build::{ControlFlowGraphBuilder, ResultYieldContext};
use super::id::AnalysisBlockId;
use super::model::{AnalysisEdgeKind, AnalysisRefinement};

impl<C> ControlFlowGraphBuilder<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn build_branches(
        &mut self,
        branches: &[BoundBlockId],
        conditions: &[BoundExpressionId],
        mut current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let join = self.push_block();

        for (condition, branch) in conditions.iter().copied().zip(branches.iter().copied()) {
            let entry = self.push_block();
            let next = self.push_block();
            let depth = self.scope_depth();
            let scope = self.begin_condition_scope(condition, current);

            self.build_condition(
                condition,
                current,
                (entry, AnalysisEdgeKind::ConditionalTrue),
                (next, AnalysisEdgeKind::ConditionalFalse),
            )?;

            let completion = self.build_result_branch(branch, entry, join, depth)?;

            if let Some(completion) = completion {
                let completion = scope.map_or(completion, |scope| {
                    self.push_scope_exit(completion, scope, branch.into())
                });

                self.push_edge(completion, join, AnalysisEdgeKind::Sequential, None);
            }

            current = if let Some(scope) = scope {
                self.scopes.pop();

                self.push_scope_exit(next, scope, condition.into())
            } else {
                next
            };
        }

        if let Some(branch) = branches.get(conditions.len()) {
            if let Some(completion) =
                self.build_result_branch(*branch, current, join, self.scope_depth())?
            {
                self.push_edge(completion, join, AnalysisEdgeKind::Sequential, None);
            }
        } else {
            self.push_edge(current, join, AnalysisEdgeKind::Sequential, None);
        }

        Some(Some(join))
    }

    pub(super) fn begin_condition_scope(
        &mut self,
        condition: BoundExpressionId,
        current: AnalysisBlockId,
    ) -> Option<BoundBlockId> {
        let bray_bound_tree::BoundExpression::Structured(test) =
            self.view().expression(condition)?
        else {
            return None;
        };

        if test.kind() != BoundStructuredExpressionKind::Condition {
            return None;
        }

        let [scope] = test.blocks() else {
            return None;
        };

        let scope = *scope;

        self.push_bound(current, scope.into());
        self.scopes.push(scope);

        Some(scope)
    }

    pub(super) fn build_condition(
        &mut self,
        id: BoundExpressionId,
        current: AnalysisBlockId,
        matched: (AnalysisBlockId, AnalysisEdgeKind),
        unmatched: (AnalysisBlockId, AnalysisEdgeKind),
    ) -> Option<()> {
        if let bray_bound_tree::BoundExpression::Structured(condition) =
            self.view().expression(id)?
            && condition.kind() == BoundStructuredExpressionKind::Condition
        {
            let operand = *condition.operands().first()?;

            self.push_bound(current, id.into());

            return self.build_condition(operand, current, matched, unmatched);
        }

        if let bray_bound_tree::BoundExpression::Unary(unary) = self.view().expression(id)?
            && unary.operator() == BoundOperator::LogicalNot
        {
            let operand = *unary.operands().first()?;

            self.push_bound(current, id.into());

            return self.build_condition(operand, current, unmatched, matched);
        }

        if let bray_bound_tree::BoundExpression::Binary(binary) = self.view().expression(id)?
            && matches!(
                binary.operator(),
                BoundOperator::LogicalAnd | BoundOperator::LogicalOr
            )
        {
            let [left, right] = binary.operands() else {
                return None;
            };

            let (left, right) = (*left, *right);

            let operator = binary.operator();

            let next = self.push_block();

            self.push_bound(current, id.into());

            if operator == BoundOperator::LogicalAnd {
                self.build_condition(
                    left,
                    current,
                    (next, AnalysisEdgeKind::ConditionalTrue),
                    unmatched,
                )?;
            } else {
                self.build_condition(
                    left,
                    current,
                    matched,
                    (next, AnalysisEdgeKind::ConditionalFalse),
                )?;
            }

            self.build_condition(right, next, matched, unmatched)?;

            return Some(());
        }

        let pattern = match self.view().expression(id)? {
            bray_bound_tree::BoundExpression::Structured(expression)
                if expression.kind() == BoundStructuredExpressionKind::PatternBinding =>
            {
                Some((
                    *expression.operands().first()?,
                    *expression.patterns().first()?,
                ))
            }
            _ => None,
        };

        let operand = pattern.map_or(id, |(subject, _)| subject);

        let Some(current) = self.build_expression(operand, current)? else {
            return Some(());
        };

        if pattern.is_some() {
            self.push_bound(current, id.into());
        }

        if pattern.is_none()
            && let Some(value) = self
                .completion_semantics
                .and_then(|(expressions, _, _)| expressions.literals().expression(id))
        {
            let value = match self.request().semantic_values().constant_value_data(value) {
                Ok(value) => value,
                Err(error) => {
                    self.record_infrastructure_failure(
                        crate::CheckerInfrastructureError::SemanticValueStore(error),
                    );

                    return None;
                }
            };

            if let bray_symbols::ConstantValueKind::Boolean(value) = value.kind() {
                let (target, kind) = if *value { matched } else { unmatched };

                self.push_edge(
                    current,
                    target,
                    kind,
                    Some(AnalysisRefinement::Condition {
                        expression: id,
                        value: *value,
                    }),
                );

                return Some(());
            }
        }

        let success = if pattern.is_some() {
            self.push_block()
        } else {
            matched.0
        };

        self.push_edge(
            current,
            success,
            matched.1,
            Some(AnalysisRefinement::Condition {
                expression: id,
                value: true,
            }),
        );

        self.push_edge(
            current,
            unmatched.0,
            unmatched.1,
            Some(AnalysisRefinement::Condition {
                expression: id,
                value: false,
            }),
        );

        if let Some((_, pattern)) = pattern
            && let Some(completion) = self.build_pattern(pattern, success)?
        {
            self.push_edge(completion, matched.0, AnalysisEdgeKind::Sequential, None);
        }

        Some(())
    }

    pub(super) fn build_result_branch(
        &mut self,
        branch: BoundBlockId,
        entry: AnalysisBlockId,
        completion: AnalysisBlockId,
        scope_depth: usize,
    ) -> Option<Option<AnalysisBlockId>> {
        let target = self.view().block(branch)?.origin().source_anchor().syntax();

        self.result_yields.push(ResultYieldContext {
            target,
            completion,
            scope_depth,
        });

        let result = self.build_block(branch, entry);

        self.result_yields.pop();

        result
    }
}
