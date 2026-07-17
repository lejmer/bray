use bray_bound_tree::{
    BoundBlockId, BoundExpressionId, BoundOperator, BoundStructuredExpressionKind,
    CheckedBlockResultRole, CheckedForIterationFacts, CheckedMatchFacts,
};

use crate::analysis::build::{CatchContext, ControlFlowGraphBuilder};
use crate::analysis::id::AnalysisBlockId;
use crate::analysis::model::{AnalysisEdgeKind, AnalysisExitKind, AnalysisRefinement};

use super::coverage::match_exhaustiveness_with_selection;

impl ControlFlowGraphBuilder<'_> {
    pub(in crate::analysis) fn build_structured(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundStructuredExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        match expression.kind() {
            BoundStructuredExpressionKind::Conditional => {
                let current = self.build_operands(expression.operands(), current)?;

                self.push_bound(current, id.into());

                self.build_branches(id, expression.blocks(), current)
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
            BoundStructuredExpressionKind::BooleanFold => {
                self.build_boolean_fold(id, expression.operands(), current)
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
                self.build_propagation(id, current, true)
            }
            BoundStructuredExpressionKind::Panic => {
                let current = self.build_operands(expression.operands(), current)?;

                self.push_bound(current, id.into());
                self.push_exit(current, AnalysisExitKind::Panic);

                Some(None)
            }
            _ => {
                let mut current = self.build_operands(expression.operands(), current)?;

                self.push_bound(current, id.into());

                for block in expression.blocks() {
                    current = self
                        .build_block_with_role(
                            *block,
                            current,
                            CheckedBlockResultRole::Expression(id),
                        )?
                        .unwrap_or_else(|| self.push_block());
                }

                Some(Some(current))
            }
        }
    }

    pub(in crate::analysis) fn build_short_circuit(
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

        self.push_edge(current, right_entry, right_kind, None);
        self.push_edge(current, join, bypass_kind, None);

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

        self.push_edge(current, success, AnalysisEdgeKind::ConditionalTrue, None);
        self.push_edge(current, failure, AnalysisEdgeKind::ConditionalFalse, None);

        let failure = self.build_operands(operands.get(1..).unwrap_or_default(), failure)?;

        self.push_exit(failure, AnalysisExitKind::Panic);

        Some(Some(success))
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
                .build_block_with_role(*block, completion, CheckedBlockResultRole::Expression(id))?
                .unwrap_or_else(|| self.push_block());
        }

        self.catches.pop();
        self.push_edge(completion, join, AnalysisEdgeKind::Sequential, None);

        Some(Some(join))
    }

    pub(in crate::analysis) fn build_for(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundForExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let current = self.build_expression(expression.source(), current)?;
        let current = current.unwrap_or_else(|| self.push_block());

        self.push_bound(current, id.into());
        if expression.is_recovered() {
            self.facts
                .push_for_iteration(CheckedForIterationFacts::recovered(id));
        } else if let Some(fact) = self
            .request()
            .control_fact_selections()
            .for_iteration(id)
            .cloned()
        {
            self.facts.push_for_iteration(fact);
        }

        self.build_iteration(
            id,
            expression.pattern(),
            expression.body(),
            expression.else_body(),
            expression.origin().source_anchor().syntax(),
            current,
        )
    }

    pub(in crate::analysis) fn build_match(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundMatchExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let current = self.build_expression(expression.subject(), current)?;
        let current = current.unwrap_or_else(|| self.push_block());

        self.push_bound(current, id.into());

        if let Some(exhaustiveness) = match_exhaustiveness_with_selection(
            self.view(),
            expression,
            self.request().control_fact_selections().match_facts(id),
        ) {
            self.facts
                .push_match(CheckedMatchFacts::new(id, exhaustiveness));
        }

        let join = self.push_block();
        let mut candidate = current;

        for arm in expression.arms() {
            let arm_entry = self.push_block();
            let next_candidate = self.push_block();
            let refinement = AnalysisRefinement::PatternSuccess(arm.pattern());

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
                    let guard = self
                        .build_expression(guard, arm_entry)?
                        .unwrap_or_else(|| self.push_block());

                    let body_entry = self.push_block();

                    self.push_edge(guard, body_entry, AnalysisEdgeKind::ConditionalTrue, None);
                    self.push_edge(
                        guard,
                        next_candidate,
                        AnalysisEdgeKind::ConditionalFalse,
                        None,
                    );

                    body_entry
                }
                None => arm_entry,
            };

            if let Some(completion) = self.build_block_with_role(
                arm.body(),
                body_entry,
                CheckedBlockResultRole::Expression(id),
            )? {
                self.push_edge(completion, join, AnalysisEdgeKind::Sequential, None);
            }

            candidate = next_candidate;
        }

        self.push_edge(candidate, join, AnalysisEdgeKind::MatchNoMatch, None);

        Some(Some(join))
    }

    fn build_branches(
        &mut self,
        expression: BoundExpressionId,
        branches: &[BoundBlockId],
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

            self.push_edge(current, entry, kind, None);

            if let Some(completion) = self.build_block_with_role(
                branch,
                entry,
                CheckedBlockResultRole::Expression(expression),
            )? {
                self.push_edge(completion, join, AnalysisEdgeKind::Sequential, None);
            }
        }

        if branches.len() < 2 {
            self.push_edge(current, join, AnalysisEdgeKind::ConditionalFalse, None);
        }

        Some(Some(join))
    }
}
