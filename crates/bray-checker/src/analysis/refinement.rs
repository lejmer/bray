use std::collections::BTreeSet;

use crate::UnitCheckRequest;

use super::fixed_point::{FixedPointDomain, FixedPointOutcome, FlowDirection, solve_fixed_point};
use super::id::AnalysisBlockId;
use super::model::{AnalysisBlock, AnalysisEdge, AnalysisRefinement, ControlFlowGraph};
use super::reachability::ReachabilityConclusions;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct RefinementState {
    is_reachable: bool,
    facts: BTreeSet<AnalysisRefinement>,
}

pub(super) struct RefinementConclusions {
    result: super::fixed_point::FixedPointResult<RefinementState>,
}

impl RefinementConclusions {
    pub(super) fn facts_at(&self, block: AnalysisBlockId) -> Option<&BTreeSet<AnalysisRefinement>> {
        self.result
            .state(block)
            .filter(|state| state.is_reachable)
            .map(|state| &state.facts)
    }
}

pub(super) fn analyze_refinements(
    graph: &ControlFlowGraph,
    reachability: &ReachabilityConclusions,
    request: UnitCheckRequest<'_>,
) -> Option<RefinementConclusions> {
    let domain = RefinementDomain {
        graph,
        reachability,
    };

    match solve_fixed_point(graph, &domain, &request) {
        FixedPointOutcome::Complete(result) => {
            let conclusions = RefinementConclusions { result };

            for block in graph.blocks() {
                if reachability.is_block_reachable(block.id())
                    && conclusions.facts_at(block.id()).is_none()
                {
                    panic!("refinement state did not cover every reachable analysis block");
                }
            }

            Some(conclusions)
        }
        FixedPointOutcome::Cancelled => None,
        FixedPointOutcome::ConvergenceInvariantViolated => {
            panic!("finite refinement analysis exceeded its convergence bound")
        }
    }
}

struct RefinementDomain<'graph> {
    graph: &'graph ControlFlowGraph,
    reachability: &'graph ReachabilityConclusions,
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
        merge_state(target, incoming, None)
    }

    fn propagate(
        &self,
        _: &AnalysisBlock,
        source: &Self::State,
        edge: &AnalysisEdge,
        target: &mut Self::State,
    ) -> bool {
        if !source.is_reachable || !self.reachability.is_edge_reachable(edge.id()) {
            return false;
        }

        merge_state(target, source, edge.refinement())
    }

    fn propagate_self(&self, _: &AnalysisBlock, _: &mut Self::State, _: &AnalysisEdge) -> bool {
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

fn merge_state(
    target: &mut RefinementState,
    incoming: &RefinementState,
    edge_fact: Option<AnalysisRefinement>,
) -> bool {
    if !incoming.is_reachable {
        return false;
    }

    if !target.is_reachable {
        target.is_reachable = true;
        target.facts.extend(incoming.facts.iter().copied());

        if let Some(fact) = edge_fact {
            target.facts.insert(fact);
        }

        return true;
    }

    let previous_len = target.facts.len();

    target
        .facts
        .retain(|fact| incoming.facts.contains(fact) || edge_fact == Some(*fact));

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

        assert!(merge_state(&mut target, &incoming, Some(second)));
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

        assert!(merge_state(&mut target, &incoming, None));
        assert_eq!(target, state([common]));
    }

    #[test]
    fn identical_edge_facts_survive_a_branch_merge() {
        let expressions = expression_ids(2);
        let common = nullable_fact(expressions[0], true);
        let first_only = nullable_fact(expressions[1], true);

        let mut target = state([common, first_only]);
        let incoming = state([]);

        assert!(merge_state(&mut target, &incoming, Some(common)));
        assert_eq!(target, state([common]));
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
