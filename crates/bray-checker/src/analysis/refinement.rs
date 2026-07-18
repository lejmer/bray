use std::collections::BTreeSet;

use crate::{CheckerRequestContext, UnitCheckRequest};

use super::fixed_point::{FixedPointDomain, FixedPointOutcome, FlowDirection, solve_fixed_point};
use super::id::AnalysisBlockId;
use super::model::{
    AnalysisBlock, AnalysisEdge, AnalysisEdgeKind, AnalysisRefinement, ControlFlowGraph,
};
use super::reachability::ReachabilityResult;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct RefinementState {
    is_reachable: bool,
    facts: BTreeSet<AnalysisRefinement>,
}

pub(super) struct RefinementResult {
    result: super::fixed_point::FixedPointResult<RefinementState>,
}

impl RefinementResult {
    pub(super) fn facts_at(&self, block: AnalysisBlockId) -> Option<&BTreeSet<AnalysisRefinement>> {
        self.result
            .state(block)
            .filter(|state| state.is_reachable)
            .map(|state| &state.facts)
    }
}

pub(super) fn analyze_refinements<C>(
    graph: &ControlFlowGraph,
    reachability: &ReachabilityResult,
    request: UnitCheckRequest<'_, C>,
) -> Option<RefinementResult>
where
    C: CheckerRequestContext + ?Sized,
{
    let domain = RefinementDomain {
        graph,
        reachability,
    };

    match solve_fixed_point(graph, &domain, &request) {
        FixedPointOutcome::Complete(result) => {
            let result = RefinementResult { result };

            for block in graph.blocks() {
                if reachability.is_block_reachable(block.id())
                    && result.facts_at(block.id()).is_none()
                {
                    panic!("refinement state did not cover every reachable analysis block");
                }
            }

            Some(result)
        }
        FixedPointOutcome::Cancelled => None,
        FixedPointOutcome::ConvergenceInvariantViolated => {
            panic!("finite refinement analysis exceeded its convergence bound")
        }
    }
}

struct RefinementDomain<'graph> {
    graph: &'graph ControlFlowGraph,
    reachability: &'graph ReachabilityResult,
}

impl FixedPointDomain for RefinementDomain<'_> {
    type State = RefinementState;

    fn direction(&self) -> FlowDirection {
        FlowDirection::Forward
    }

    fn bottom(&self) -> Self::State {
        RefinementState::default()
    }

    fn boundary(&self) -> Self::State {
        RefinementState {
            is_reachable: true,
            facts: BTreeSet::new(),
        }
    }

    fn merge_boundary(&self, target: &mut Self::State, incoming: &Self::State) -> bool {
        merge_state(target, incoming, None, false)
    }

    fn propagate(
        &self,
        block: &AnalysisBlock,
        source: &Self::State,
        edge: &AnalysisEdge,
        target: &mut Self::State,
    ) -> bool {
        if !source.is_reachable || !self.reachability.is_edge_reachable(edge.id()) {
            return false;
        }

        let invalidates_facts =
            self.block_invalidates_facts(block) || edge.kind() == AnalysisEdgeKind::Recovery;

        merge_state(target, source, edge.refinement(), invalidates_facts)
    }

    fn propagate_self(
        &self,
        block: &AnalysisBlock,
        state: &mut Self::State,
        edge: &AnalysisEdge,
    ) -> bool {
        if self.block_invalidates_facts(block) || edge.kind() == AnalysisEdgeKind::Recovery {
            let changed = !state.facts.is_empty();

            state.facts.clear();

            return changed;
        }

        false
    }

    fn convergence_bound(&self, _: &ControlFlowGraph) -> usize {
        let refinements = self
            .graph
            .edges()
            .iter()
            .filter(|edge| edge.refinement().is_some())
            .count();

        self.graph
            .blocks()
            .len()
            .saturating_mul(refinements.saturating_add(2))
    }
}

impl RefinementDomain<'_> {
    fn block_invalidates_facts(&self, block: &AnalysisBlock) -> bool {
        !block.operations().is_empty()
    }
}

fn merge_state(
    target: &mut RefinementState,
    incoming: &RefinementState,
    edge_fact: Option<AnalysisRefinement>,
    invalidates_facts: bool,
) -> bool {
    if !incoming.is_reachable {
        return false;
    }

    if !target.is_reachable {
        target.is_reachable = true;

        if !invalidates_facts {
            target.facts.extend(incoming.facts.iter().copied());
        }

        if let Some(fact) = edge_fact {
            target.facts.insert(fact);
        }

        return true;
    }

    let previous_len = target.facts.len();

    target.facts.retain(|fact| {
        (!invalidates_facts && incoming.facts.contains(fact)) || edge_fact == Some(*fact)
    });

    target.facts.len() != previous_len
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundErrorExpression, BoundExpression, BoundExpressionId, BoundNodeOrigin,
        BoundTreeBuilder, BoundUnitId,
    };

    use super::{RefinementState, merge_state};
    use crate::analysis::model::AnalysisRefinement;

    #[test]
    fn first_reachable_predecessor_establishes_its_facts() {
        let expressions = expression_ids(2);
        let first = nullable_fact(expressions[0], true);
        let second = nullable_fact(expressions[1], true);

        let incoming = state([first]);
        let mut target = RefinementState::default();

        assert!(merge_state(&mut target, &incoming, Some(second), false));
        assert_eq!(target, state([first, second]));
    }

    #[test]
    fn branch_merge_keeps_only_facts_proven_by_every_predecessor() {
        let expressions = expression_ids(3);
        let common = nullable_fact(expressions[0], true);
        let first_only = nullable_fact(expressions[1], true);
        let second_only = nullable_fact(expressions[2], true);

        let mut target = state([common, first_only]);
        let incoming = state([common, second_only]);

        assert!(merge_state(&mut target, &incoming, None, false));
        assert_eq!(target, state([common]));
    }

    #[test]
    fn identical_edge_facts_survive_a_branch_merge() {
        let expressions = expression_ids(2);
        let common = nullable_fact(expressions[0], true);
        let first_only = nullable_fact(expressions[1], true);

        let mut target = state([common, first_only]);
        let incoming = state([]);

        assert!(merge_state(&mut target, &incoming, Some(common), false));
        assert_eq!(target, state([common]));
    }

    #[test]
    fn uncertain_operations_invalidate_every_incoming_fact() {
        let expressions = expression_ids(2);
        let first = nullable_fact(expressions[0], true);
        let edge_fact = nullable_fact(expressions[1], true);

        let mut target = RefinementState::default();
        let incoming = state([first]);

        assert!(merge_state(&mut target, &incoming, Some(edge_fact), true,));
        assert_eq!(target, state([edge_fact]));
    }

    fn state(facts: impl IntoIterator<Item = AnalysisRefinement>) -> RefinementState {
        RefinementState {
            is_reachable: true,
            facts: facts.into_iter().collect(),
        }
    }

    fn nullable_fact(expression: BoundExpressionId, is_present: bool) -> AnalysisRefinement {
        AnalysisRefinement::NullablePresence {
            expression,
            is_present,
        }
    }

    fn expression_ids(count: usize) -> Vec<BoundExpressionId> {
        let key = crate::test_support::callable_key();
        let mut builder = BoundTreeBuilder::new(BoundUnitId::new(1));
        let mut expressions = Vec::new();
        let ty = crate::test_support::error_type();

        for _ in 0..count {
            let expression = BoundExpression::Error(BoundErrorExpression::new(
                BoundNodeOrigin::source(key.source()),
                ty,
            ));

            let Ok(expression) = builder.push_expression(expression) else {
                panic!("test expressions must fit in the empty bound tree");
            };

            expressions.push(expression);
        }

        expressions
    }
}
