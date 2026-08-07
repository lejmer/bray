use bray_bound_tree::{
    BoundBlockId, BoundExpressionId, BoundOperator, BoundPatternId, BoundStructuredExpressionKind,
};
use bray_declarations::SyntaxAnchor;

use crate::CheckerRequestContext;

use super::build::{CatchContext, ControlFlowGraphBuilder, LoopContext};
use super::id::AnalysisBlockId;
use super::model::{AnalysisEdgeKind, AnalysisExitKind, AnalysisRefinement};

impl<C> ControlFlowGraphBuilder<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn build_structured(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundStructuredExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        match expression.kind() {
            BoundStructuredExpressionKind::Conditional => {
                let current = self.build_operands(expression.operands(), current)?;
                let condition = expression.operands().first().copied();

                self.push_bound(current, id.into());

                self.build_branches(expression.blocks(), condition, current)
            }
            BoundStructuredExpressionKind::While => {
                let Some(body) = expression.blocks().first().copied() else {
                    self.push_recovery(current, id.into());

                    return Some(Some(current));
                };

                self.build_while(
                    id,
                    expression.operands(),
                    body,
                    expression.blocks().get(1).copied(),
                    expression.origin().source_anchor().syntax(),
                    current,
                )
            }
            BoundStructuredExpressionKind::Loop => {
                let Some(body) = expression.blocks().first().copied() else {
                    self.push_recovery(current, id.into());

                    return Some(Some(current));
                };

                self.build_unconditional_loop(
                    id,
                    body,
                    expression.origin().source_anchor().syntax(),
                    current,
                )
            }
            BoundStructuredExpressionKind::Assertion => {
                self.build_assertion(id, expression.operands(), current)
            }
            BoundStructuredExpressionKind::TrustBoundary => {
                self.build_trust_boundary(id, expression.operands(), current)
            }
            BoundStructuredExpressionKind::BooleanAllFold
            | BoundStructuredExpressionKind::BooleanAnyFold => {
                self.build_boolean_fold(id, expression.operands(), current)
            }
            BoundStructuredExpressionKind::ArrayGenerator
            | BoundStructuredExpressionKind::GeneralGenerator => {
                self.build_generator_region(id, expression, current)
            }
            BoundStructuredExpressionKind::Catch => {
                self.build_catch(id, expression.operands(), expression.blocks(), current)
            }
            BoundStructuredExpressionKind::ResultPropagation => {
                let current = self.build_operands(expression.operands(), current)?;

                self.push_bound(current, id.into());

                self.build_result_or_run_result_propagation(id, expression, current)
            }
            BoundStructuredExpressionKind::NullablePropagation => {
                let current = self.build_operands(expression.operands(), current)?;

                self.push_bound(current, id.into());

                let subject = expression.operands().first().copied().unwrap_or(id);

                self.build_propagation(subject, current, true)
            }
            BoundStructuredExpressionKind::Panic => {
                let current = self.build_operands(expression.operands(), current)?;

                self.push_bound(current, id.into());
                self.push_exit(current, AnalysisExitKind::Panic, id.into());

                Some(None)
            }
            _ => {
                let mut current = self.build_operands(expression.operands(), current)?;

                self.push_bound(current, id.into());

                for block in expression.blocks() {
                    current = self
                        .build_block(*block, current)?
                        .unwrap_or_else(|| self.push_block());
                }

                Some(Some(current))
            }
        }
    }

    fn build_generator_region(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundStructuredExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        self.yield_regions
            .push(expression.origin().source_anchor().syntax());

        let completion = self.build_operands(expression.operands(), current);

        self.yield_regions.pop();

        let mut current = completion?;

        self.push_bound(current, id.into());

        for block in expression.blocks() {
            current = self
                .build_block(*block, current)?
                .unwrap_or_else(|| self.push_block());
        }

        Some(Some(current))
    }

    pub(super) fn build_short_circuit(
        &mut self,
        id: BoundExpressionId,
        operands: &[BoundExpressionId],
        operator: BoundOperator,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let Some(left) = operands.first().copied() else {
            self.push_recovery(current, id.into());

            return Some(Some(current));
        };

        let current = self
            .build_expression(left, current)?
            .unwrap_or_else(|| self.push_block());

        self.push_bound(current, id.into());

        let join = self.push_block();
        let right_entry = self.push_block();

        let (right_kind, bypass_kind) = match operator {
            BoundOperator::LogicalAnd => (
                AnalysisEdgeKind::ConditionalTrue,
                AnalysisEdgeKind::ConditionalFalse,
            ),
            BoundOperator::LogicalOr => (
                AnalysisEdgeKind::ConditionalFalse,
                AnalysisEdgeKind::ConditionalTrue,
            ),
            _ => return Some(Some(current)),
        };

        self.push_edge(
            current,
            right_entry,
            right_kind,
            Some(AnalysisRefinement::Condition {
                expression: left,
                value: operator == BoundOperator::LogicalAnd,
            }),
        );

        self.push_edge(
            current,
            join,
            bypass_kind,
            Some(AnalysisRefinement::Condition {
                expression: left,
                value: operator == BoundOperator::LogicalOr,
            }),
        );

        if let Some(right) = operands.get(1).copied() {
            if let Some(completion) = self.build_expression(right, right_entry)? {
                self.push_edge(completion, join, AnalysisEdgeKind::Sequential, None);
            }
        } else {
            self.push_recovery(right_entry, id.into());
            self.push_edge(right_entry, join, AnalysisEdgeKind::Recovery, None);
        }

        Some(Some(join))
    }

    fn build_assertion(
        &mut self,
        id: BoundExpressionId,
        operands: &[BoundExpressionId],
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let Some(condition) = operands.first().copied() else {
            self.push_recovery(current, id.into());

            return Some(Some(current));
        };

        let current = self
            .build_expression(condition, current)?
            .unwrap_or_else(|| self.push_block());

        self.push_bound(current, id.into());

        let success = self.push_block();
        let failure = self.push_block();

        self.push_edge(
            current,
            success,
            AnalysisEdgeKind::ConditionalTrue,
            Some(AnalysisRefinement::Condition {
                expression: condition,
                value: true,
            }),
        );

        self.push_edge(
            current,
            failure,
            AnalysisEdgeKind::ConditionalFalse,
            Some(AnalysisRefinement::Condition {
                expression: condition,
                value: false,
            }),
        );

        let failure = self.build_operands(operands.get(1..).unwrap_or_default(), failure)?;

        self.push_exit(failure, AnalysisExitKind::Panic, id.into());

        Some(Some(success))
    }

    fn build_trust_boundary(
        &mut self,
        id: BoundExpressionId,
        operands: &[BoundExpressionId],
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let entry = self.push_block();

        self.push_edge(
            current,
            entry,
            AnalysisEdgeKind::Sequential,
            Some(AnalysisRefinement::TrustBoundary(id)),
        );

        let current = self.build_operands(operands, entry)?;

        self.push_bound(current, id.into());

        Some(Some(current))
    }

    fn build_boolean_fold(
        &mut self,
        id: BoundExpressionId,
        operands: &[BoundExpressionId],
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let current = self.build_operands(operands, current)?;

        self.push_bound(current, id.into());

        let iteration = self.push_block();
        let completion = self.push_block();

        self.push_edge(current, iteration, AnalysisEdgeKind::LoopEntry, None);
        self.push_edge(iteration, iteration, AnalysisEdgeKind::LoopBack, None);

        self.push_edge(
            iteration,
            completion,
            AnalysisEdgeKind::ConditionalFalse,
            None,
        );

        Some(Some(completion))
    }

    fn build_catch(
        &mut self,
        id: BoundExpressionId,
        operands: &[BoundExpressionId],
        blocks: &[BoundBlockId],
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        self.push_bound(current, id.into());

        let join = self.push_block();

        self.catches.push(CatchContext {
            target: join,
            scope_depth: self.scope_depth(),
        });

        let mut completion = self.build_operands(operands, current)?;

        for block in blocks {
            completion = self
                .build_block(*block, completion)?
                .unwrap_or_else(|| self.push_block());
        }

        self.catches.pop();
        self.push_edge(completion, join, AnalysisEdgeKind::Sequential, None);

        Some(Some(join))
    }

    pub(super) fn build_for(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundForExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let current = self.build_expression(expression.source(), current)?;
        let current = current.unwrap_or_else(|| self.push_block());

        self.push_bound(current, id.into());

        self.build_iteration(
            expression.pattern(),
            expression.body(),
            expression.else_body(),
            expression.origin().source_anchor().syntax(),
            current,
        )
    }

    pub(super) fn build_match(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundMatchExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let current = self.build_expression(expression.subject(), current)?;
        let current = current.unwrap_or_else(|| self.push_block());

        self.push_bound(current, id.into());

        let join = self.push_block();
        let mut candidate = current;

        for arm in expression.arms() {
            let arm_entry = self.push_block();
            let next_candidate = self.push_block();

            let refinement = AnalysisRefinement::PatternSuccess {
                subject: expression.subject(),
                pattern: arm.pattern(),
            };

            self.push_edge(
                candidate,
                arm_entry,
                AnalysisEdgeKind::MatchArm,
                Some(refinement),
            );

            self.push_edge(
                candidate,
                next_candidate,
                AnalysisEdgeKind::MatchNoMatch,
                None,
            );

            let arm_entry = self
                .build_pattern(arm.pattern(), arm_entry)?
                .unwrap_or_else(|| self.push_block());

            let body_entry = match arm.guard() {
                Some(guard) => {
                    let guard_expression = guard;

                    let guard = self
                        .build_expression(guard, arm_entry)?
                        .unwrap_or_else(|| self.push_block());

                    let body_entry = self.push_block();

                    self.push_edge(
                        guard,
                        body_entry,
                        AnalysisEdgeKind::ConditionalTrue,
                        Some(AnalysisRefinement::Condition {
                            expression: guard_expression,
                            value: true,
                        }),
                    );

                    self.push_edge(
                        guard,
                        next_candidate,
                        AnalysisEdgeKind::ConditionalFalse,
                        Some(AnalysisRefinement::Condition {
                            expression: guard_expression,
                            value: false,
                        }),
                    );

                    body_entry
                }
                None => arm_entry,
            };

            if let Some(completion) = self.build_block(arm.body(), body_entry)? {
                self.push_edge(completion, join, AnalysisEdgeKind::Sequential, None);
            }

            candidate = next_candidate;
        }

        self.push_edge(candidate, join, AnalysisEdgeKind::MatchNoMatch, None);

        Some(Some(join))
    }

    fn build_branches(
        &mut self,
        branches: &[BoundBlockId],
        condition: Option<BoundExpressionId>,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let join = self.push_block();

        for (index, branch) in branches.iter().copied().enumerate() {
            let entry = self.push_block();

            let kind = if index == 0 {
                AnalysisEdgeKind::ConditionalTrue
            } else {
                AnalysisEdgeKind::ConditionalFalse
            };

            self.push_edge(
                current,
                entry,
                kind,
                condition.map(|expression| AnalysisRefinement::Condition {
                    expression,
                    value: index == 0,
                }),
            );

            if let Some(completion) = self.build_block(branch, entry)? {
                self.push_edge(completion, join, AnalysisEdgeKind::Sequential, None);
            }
        }

        if branches.len() < 2 {
            self.push_edge(
                current,
                join,
                AnalysisEdgeKind::ConditionalFalse,
                condition.map(|expression| AnalysisRefinement::Condition {
                    expression,
                    value: false,
                }),
            );
        }

        Some(Some(join))
    }

    fn build_while(
        &mut self,
        id: BoundExpressionId,
        condition: &[BoundExpressionId],
        body: BoundBlockId,
        else_body: Option<BoundBlockId>,
        target: SyntaxAnchor,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let header = self.push_block();
        let body_entry = self.push_block();
        let exhausted = self.push_block();
        let join = self.push_block();

        self.push_edge(current, header, AnalysisEdgeKind::Sequential, None);

        let condition_expression = condition.last().copied();
        let condition = self.build_operands(condition, header)?;

        self.push_bound(condition, id.into());

        self.push_edge(
            condition,
            body_entry,
            AnalysisEdgeKind::LoopEntry,
            condition_expression.map(|expression| AnalysisRefinement::Condition {
                expression,
                value: true,
            }),
        );

        self.push_edge(
            condition,
            exhausted,
            AnalysisEdgeKind::ConditionalFalse,
            condition_expression.map(|expression| AnalysisRefinement::Condition {
                expression,
                value: false,
            }),
        );

        self.loops.push(LoopContext {
            target,
            continue_target: Some(header),
            completion: join,
            scope_depth: self.scope_depth(),
        });

        if let Some(body_exit) = self.build_block(body, body_entry)? {
            self.push_edge(body_exit, header, AnalysisEdgeKind::LoopBack, None);
        }

        if let Some(context) = self.loops.last_mut() {
            context.continue_target = None;
        }

        match else_body {
            Some(else_body) => {
                if let Some(else_exit) = self.build_block(else_body, exhausted)? {
                    self.push_edge(else_exit, join, AnalysisEdgeKind::Sequential, None);
                }
            }
            None => self.push_edge(exhausted, join, AnalysisEdgeKind::Sequential, None),
        }

        self.loops.pop();

        Some(Some(join))
    }

    fn build_unconditional_loop(
        &mut self,
        id: BoundExpressionId,
        body: BoundBlockId,
        target: SyntaxAnchor,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let header = self.push_block();
        let body_entry = self.push_block();
        let join = self.push_block();

        self.push_edge(current, header, AnalysisEdgeKind::Sequential, None);
        self.push_bound(header, id.into());
        self.push_edge(header, body_entry, AnalysisEdgeKind::LoopEntry, None);
        self.push_exit(header, AnalysisExitKind::Divergence, id.into());

        self.loops.push(LoopContext {
            target,
            continue_target: Some(header),
            completion: join,
            scope_depth: self.scope_depth(),
        });

        if let Some(body_exit) = self.build_block(body, body_entry)? {
            self.push_edge(body_exit, header, AnalysisEdgeKind::LoopBack, None);
        }

        self.loops.pop();

        Some(Some(join))
    }

    pub(super) fn build_iteration(
        &mut self,
        pattern: BoundPatternId,
        body: BoundBlockId,
        else_body: Option<BoundBlockId>,
        target: SyntaxAnchor,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let header = self.push_block();
        let item = self.push_block();
        let exhausted = self.push_block();
        let join = self.push_block();

        self.push_edge(current, header, AnalysisEdgeKind::Sequential, None);
        self.push_edge(header, item, AnalysisEdgeKind::LoopEntry, None);
        self.push_edge(header, exhausted, AnalysisEdgeKind::ConditionalFalse, None);

        self.loops.push(LoopContext {
            target,
            continue_target: Some(header),
            completion: join,
            scope_depth: self.scope_depth(),
        });

        let item = self
            .build_pattern(pattern, item)?
            .unwrap_or_else(|| self.push_block());

        if let Some(body_exit) = self.build_block(body, item)? {
            self.push_edge(body_exit, header, AnalysisEdgeKind::LoopBack, None);
        }

        if let Some(context) = self.loops.last_mut() {
            context.continue_target = None;
        }

        match else_body {
            Some(else_body) => {
                if let Some(else_exit) = self.build_block(else_body, exhausted)? {
                    self.push_edge(else_exit, join, AnalysisEdgeKind::Sequential, None);
                }
            }
            None => self.push_edge(exhausted, join, AnalysisEdgeKind::Sequential, None),
        }

        self.loops.pop();

        Some(Some(join))
    }
}
