use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{AnyBoundNodeId, BoundBlockId, BoundExpressionId, CallableProofDependency};
use bray_symbols::{ConstantTermId, ExecutionProperty, ProofOutcome, SemanticValueStore};

use crate::contract::{conditions_are_inconsistent, prove_condition};
use crate::{CheckerInfrastructureError, ExecutionGuaranteeInput};

use super::super::fixed_point::{FixedPointDomain, FlowDirection};
use super::super::model::{
    AnalysisBlock, AnalysisCallPhase, AnalysisEdge, AnalysisEdgeKind, AnalysisOperation,
    AnalysisOperationKind, AnalysisRefinement, ControlFlowGraph,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DomainState {
    pub(super) reachable: bool,
    pub(super) entry_observations_valid: bool,
    pub(super) invalidation: Option<super::super::id::AnalysisObservationSite>,
    pub(super) conditions: BTreeSet<(ConstantTermId, bool)>,
    pub(super) dependencies: BTreeSet<CallableProofDependency>,
    pub(super) total_calls: BTreeSet<BoundExpressionId>,
    pub(super) total_cleanup: BTreeSet<(BoundBlockId, AnyBoundNodeId)>,
    pub(super) observations: BTreeMap<ConstantTermId, ConstantTermId>,
    pub(super) evaluated_values: BTreeMap<BoundExpressionId, ConstantTermId>,
    pub(super) call_completions: BTreeMap<BoundExpressionId, super::call::CallCompletion>,
}

pub(super) struct GuaranteeDomain<'a> {
    pub(super) view: bray_bound_tree::BoundUnitView<'a>,
    pub(super) selections: &'a bray_bound_tree::CheckedSemanticSelections,
    pub(super) patterns: &'a bray_bound_tree::CheckedPatterns,
    pub(super) boolean: bray_symbols::TypeId,
    pub(super) result_representation: Option<bray_symbols::CompilerKnownResultRepresentation>,
    pub(super) available: &'a bray_symbols::AvailableCompilerKnownSymbols,
    pub(super) asynchronous: &'a bray_bound_tree::CheckedAsync,
    pub(super) graph: &'a ControlFlowGraph,
    pub(super) values: &'a SemanticValueStore,
    pub(super) input: &'a ExecutionGuaranteeInput,
    pub(super) assumptions: &'a [(ConstantTermId, bool)],
    pub(super) pure_operations: &'a BTreeSet<super::super::id::AnalysisOperationId>,
    pub(super) total_operations: &'a BTreeSet<super::super::id::AnalysisOperationId>,
    pub(super) storage_calls: &'a BTreeSet<AnyBoundNodeId>,
    pub(super) empty_cleanup: &'a BTreeSet<(BoundBlockId, AnyBoundNodeId)>,
    pub(super) assignments:
        &'a BTreeMap<super::super::id::AnalysisOperationId, (ConstantTermId, BoundExpressionId)>,
    pub(super) storage: &'a bray_bound_tree::StoragePlan,
    pub(super) storage_exits:
        &'a BTreeMap<bray_bound_tree::StorageExitPoint, &'a bray_bound_tree::StorageExitDecision>,
    pub(super) mutable_borrow_accesses: &'a BTreeSet<bray_bound_tree::StorageAccessId>,
    pub(super) observation_accesses:
        &'a BTreeMap<ConstantTermId, BTreeSet<bray_bound_tree::StorageAccessId>>,
    pub(super) mutations:
        &'a BTreeMap<bray_bound_tree::BoundOperationPoint, Box<[bray_bound_tree::StorageAccessId]>>,
    pub(super) retained_mutations: Option<&'a [bray_bound_tree::StorageAccessId]>,
    pub(super) local_initializers:
        &'a BTreeMap<bray_bound_tree::BoundPatternId, (ConstantTermId, BoundExpressionId)>,
}

impl GuaranteeDomain<'_> {
    pub(super) fn invalidate_observations(&self, state: &mut DomainState) {
        state.entry_observations_valid = false;
        state.invalidation = None;
        state.observations.clear();

        state
            .evaluated_values
            .retain(|expression, _| self.input.is_scalar_observation(*expression));
    }

    pub(super) fn operation_dependencies(
        &self,
        operation: &AnalysisOperation,
        property: ExecutionProperty,
        state: &DomainState,
    ) -> Result<Option<Vec<CallableProofDependency>>, CheckerInfrastructureError> {
        if self.storage_calls.contains(&operation.kind().node()) {
            return Ok(None);
        }

        let preserved = match property {
            ExecutionProperty::Pure => self.pure_operations,
            ExecutionProperty::Total => self.total_operations,
        };

        if preserved.contains(&operation.id()) {
            return Ok(Some(Vec::new()));
        }

        if let AnalysisOperationKind::Call {
            expression,
            phase: AnalysisCallPhase::Attempt,
        } = operation.kind()
        {
            return self
                .call_dependencies(expression, property, state)
                .map_err(Into::into);
        }

        if let AnalysisOperationKind::ScopeExit {
            block,
            exit,
            phase: super::super::model::AnalysisScopeExitPhase::LifecycleResolution,
            ..
        } = operation.kind()
        {
            return self.cleanup_dependencies(block, exit, property, state);
        }

        Ok(None)
    }

    pub(super) fn transfer_operation(
        &self,
        operation: &AnalysisOperation,
        state: &mut DomainState,
    ) -> Result<(), CheckerInfrastructureError> {
        if let AnalysisOperationKind::Call {
            expression,
            phase: AnalysisCallPhase::Attempt,
        } = operation.kind()
        {
            state.call_completions.remove(&expression);

            if let Some(completion) = self.prepare_call_completion(expression, state)? {
                state.call_completions.insert(expression, completion);
            }
        }

        if let AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression)) =
            operation.kind()
            && self.input.is_scalar_observation(expression)
        {
            state.evaluated_values.remove(&expression);

            if let Some(value) = self.current_expression(state, expression)? {
                state.evaluated_values.insert(expression, value);
            }
        }

        if let AnalysisOperationKind::Call {
            expression,
            phase: AnalysisCallPhase::Attempt,
        } = operation.kind()
            && let Some(dependencies) =
                self.operation_dependencies(operation, ExecutionProperty::Total, state)?
        {
            state.total_calls.insert(expression);
            state.dependencies.extend(dependencies);
        }

        if let AnalysisOperationKind::ScopeExit {
            block,
            exit,
            phase: super::super::model::AnalysisScopeExitPhase::LifecycleResolution,
            ..
        } = operation.kind()
            && let Some(dependencies) =
                self.operation_dependencies(operation, ExecutionProperty::Total, state)?
        {
            state.total_cleanup.insert((block, exit));
            state.dependencies.extend(dependencies);
        }

        let assigned = match self.assignments.get(&operation.id()) {
            Some((target, value)) if !self.storage_calls.contains(&operation.kind().node()) => self
                .current_expression(state, *value)?
                .map(|value| (*target, value)),
            _ => None,
        };

        match self.operation_dependencies(operation, ExecutionProperty::Pure, state)? {
            Some(dependencies) => state.dependencies.extend(dependencies),
            None => {
                let call_mutations = match operation.kind() {
                    AnalysisOperationKind::Call {
                        expression,
                        phase: AnalysisCallPhase::Attempt,
                    } if !self.storage_calls.contains(&operation.kind().node()) => {
                        self.call_mutations(expression)?
                    }
                    AnalysisOperationKind::Suspension { expression, .. } => {
                        self.suspension_mutations(expression)?
                    }
                    AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression))
                        if matches!(
                            self.view.expression(expression),
                            Some(bray_bound_tree::BoundExpression::Await(_))
                        ) =>
                    {
                        self.suspension_mutations(expression)?
                    }
                    _ => None,
                };

                let checked_mutations = !self.storage_calls.contains(&operation.kind().node())
                    && self.assignments.contains_key(&operation.id());

                if let Some(mut mutations) = call_mutations {
                    mutations.extend(
                        self.mutations
                            .get(&operation.kind().point())
                            .into_iter()
                            .flatten()
                            .copied(),
                    );

                    self.invalidate_storage_observations(state, &mutations)?;
                } else if checked_mutations {
                    self.invalidate_storage_observations(
                        state,
                        self.mutations
                            .get(&operation.kind().point())
                            .map_or(&[], |accesses| accesses.as_ref()),
                    )?;
                } else if let AnalysisOperationKind::ScopeExit {
                    block, exit, phase, ..
                } = operation.kind()
                {
                    if let Some(mutations) = self.scope_exit_mutations(block, exit, phase)? {
                        self.invalidate_storage_observations(state, &mutations)?;
                    } else {
                        self.invalidate_observations(state);
                    }
                } else {
                    self.invalidate_observations(state);
                }

                state.invalidation = Some(super::super::id::AnalysisObservationSite::Operation(
                    operation.id(),
                ));
            }
        }

        if let Some((target, value)) = assigned {
            state.observations.insert(target, value);
        }

        if let AnalysisOperationKind::Call {
            expression,
            phase: AnalysisCallPhase::Completion,
        } = operation.kind()
        {
            self.complete_call(expression, state)?;
        }

        if let AnalysisOperationKind::Bound(AnyBoundNodeId::Pattern(pattern)) = operation.kind()
            && let Some((target, initializer)) = self.local_initializers.get(&pattern)
        {
            state.observations.remove(target);

            if let Some(value) = self.current_expression(state, *initializer)? {
                state.observations.insert(*target, value);
            }

            self.transfer_value_observations(state, *initializer, *target)?;
        }

        Ok(())
    }

    pub(super) fn edge_state(
        &self,
        source: &DomainState,
        edge: &AnalysisEdge,
    ) -> Result<DomainState, CheckerInfrastructureError> {
        // Each successor owns its bounded set of copyable condition identities.
        let mut incoming = source.clone();

        if matches!(
            edge.kind(),
            AnalysisEdgeKind::LoopBack | AnalysisEdgeKind::LoopContinue
        ) {
            // Call observations name one evaluation, so a new iteration starts without its
            // predecessor's result identities or conditions derived from those results.
            incoming.evaluated_values.clear();
            incoming.call_completions.clear();
            incoming.observations.clear();

            incoming
                .conditions
                .retain(|condition| self.assumptions.contains(condition));
        }

        if let Some(AnalysisRefinement::CleanupFailure { scope, exit }) = edge.refinement()
            && (self.empty_cleanup.contains(&(scope, exit))
                || incoming.total_cleanup.contains(&(scope, exit)))
        {
            incoming.reachable = false;
        }

        if let Some(AnalysisRefinement::CallFailure(expression)) = edge.refinement()
            && incoming.total_calls.contains(&expression)
        {
            incoming.reachable = false;
        }

        if let Some(AnalysisRefinement::MatchExhaustion(expression)) = edge.refinement()
            && self
                .patterns
                .match_coverage(expression)
                .is_some_and(|coverage| !coverage.is_recovered() && coverage.is_exhaustive())
        {
            incoming.reachable = false;
        }

        if !incoming.reachable {
            return Ok(incoming);
        }

        let Some((condition, value)) = self.refinement_condition(&incoming, edge.refinement())?
        else {
            return Ok(incoming);
        };

        let assumptions = incoming.conditions.iter().copied().collect::<Vec<_>>();
        let proof = prove_condition(self.values, &assumptions, condition)?;

        if matches!(
            (proof, value),
            (ProofOutcome::Proven, false) | (ProofOutcome::Disproven, true)
        ) {
            incoming.reachable = false;
        } else if incoming.conditions.len() < crate::contract::MAX_CONDITION_STEPS {
            incoming.conditions.insert((condition, value));
        }

        Ok(incoming)
    }
}

impl FixedPointDomain for GuaranteeDomain<'_> {
    type State = Result<DomainState, CheckerInfrastructureError>;

    fn direction(&self) -> FlowDirection {
        FlowDirection::Forward
    }

    fn bottom(&self) -> Self::State {
        Ok(DomainState {
            reachable: false,
            entry_observations_valid: true,
            invalidation: None,
            conditions: BTreeSet::new(),
            dependencies: BTreeSet::new(),
            total_calls: BTreeSet::new(),
            total_cleanup: BTreeSet::new(),
            observations: BTreeMap::new(),
            evaluated_values: BTreeMap::new(),
            call_completions: BTreeMap::new(),
        })
    }

    fn boundary(&self) -> Self::State {
        Ok(DomainState {
            reachable: !conditions_are_inconsistent(self.values, self.assumptions)?,
            entry_observations_valid: true,
            invalidation: None,
            conditions: self.assumptions.iter().copied().collect(),
            dependencies: self
                .input
                .projection_dependencies()
                .iter()
                .copied()
                .collect(),
            total_calls: BTreeSet::new(),
            total_cleanup: BTreeSet::new(),
            observations: BTreeMap::new(),
            evaluated_values: BTreeMap::new(),
            call_completions: BTreeMap::new(),
        })
    }

    fn merge_boundary(&self, target: &mut Self::State, boundary: &Self::State) -> bool {
        merge(target, boundary, None)
    }

    fn transfer(&self, block: &AnalysisBlock, source: &Self::State) -> Self::State {
        // Transfer owns a bounded map independently of the fixed-point entry state.
        let mut state = source.clone()?;

        state.total_calls.clear();
        state.total_cleanup.clear();

        if state.reachable {
            for operation in block
                .operations()
                .iter()
                .filter_map(|operation| self.graph.operation(*operation))
            {
                self.transfer_operation(operation, &mut state)?;
            }
        }

        Ok(state)
    }

    fn propagate(
        &self,
        source: &Self::State,
        edge: &AnalysisEdge,
        target: &mut Self::State,
    ) -> bool {
        let incoming = source
            .as_ref()
            .map_err(|error| *error)
            .and_then(|state| self.edge_state(state, edge));

        merge(target, &incoming, Some(edge.target()))
    }

    fn convergence_bound(&self, _: &ControlFlowGraph) -> usize {
        self.graph.blocks().len().saturating_mul(
            self.graph
                .edges()
                .len()
                .saturating_add(self.assumptions.len())
                .saturating_add(self.input.dependency_count())
                .saturating_add(3),
        )
    }
}

fn merge(
    target: &mut Result<DomainState, CheckerInfrastructureError>,
    incoming: &Result<DomainState, CheckerInfrastructureError>,
    join: Option<super::super::id::AnalysisBlockId>,
) -> bool {
    let incoming = match incoming {
        Err(error) => {
            if target.is_ok() {
                *target = Err(*error);

                return true;
            }

            return false;
        }
        Ok(incoming) if incoming.reachable => incoming,
        Ok(_) => return false,
    };

    let Ok(target) = target else {
        return false;
    };

    if !target.reachable {
        // A newly reached block independently owns its first incoming condition set.
        *target = incoming.clone();

        return true;
    }

    let previous_count = target.conditions.len();
    let previous_validity = target.entry_observations_valid;
    let previous_invalidation = target.invalidation;
    let previous_dependencies = target.dependencies.len();
    let previous_calls = target.total_calls.len();
    let previous_cleanup = target.total_cleanup.len();
    let previous_observations = target.observations.len();
    let previous_values = target.evaluated_values.len();
    let previous_completions = target.call_completions.len();

    target
        .conditions
        .retain(|condition| incoming.conditions.contains(condition));

    target.entry_observations_valid &= incoming.entry_observations_valid;

    if target.invalidation != incoming.invalidation {
        target.invalidation = join.map(super::super::id::AnalysisObservationSite::BlockEntry);
    }

    target
        .dependencies
        .extend(incoming.dependencies.iter().copied());

    target
        .total_calls
        .retain(|call| incoming.total_calls.contains(call));

    target
        .total_cleanup
        .retain(|cleanup| incoming.total_cleanup.contains(cleanup));

    target
        .observations
        .retain(|term, value| incoming.observations.get(term) == Some(value));

    target
        .evaluated_values
        .retain(|expression, value| incoming.evaluated_values.get(expression) == Some(value));

    target.call_completions.retain(|expression, completion| {
        incoming.call_completions.get(expression) == Some(completion)
    });

    previous_count != target.conditions.len()
        || previous_validity != target.entry_observations_valid
        || previous_invalidation != target.invalidation
        || previous_dependencies != target.dependencies.len()
        || previous_calls != target.total_calls.len()
        || previous_cleanup != target.total_cleanup.len()
        || previous_observations != target.observations.len()
        || previous_values != target.evaluated_values.len()
        || previous_completions != target.call_completions.len()
}

#[cfg(test)]
mod tests {
    use super::{DomainState, merge};
    use bray_symbols::{ConstantTermData, SemanticValueStore, SymbolOrdinal};

    #[test]
    fn joins_keep_only_common_evidence_and_preserve_invalidation() {
        let values = SemanticValueStore::try_new().unwrap();

        let input = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let mut target = Ok(DomainState {
            reachable: true,
            entry_observations_valid: true,
            invalidation: None,
            conditions: [(input, true)].into_iter().collect(),
            dependencies: Default::default(),
            total_calls: Default::default(),
            total_cleanup: Default::default(),
            observations: Default::default(),
            evaluated_values: Default::default(),
            call_completions: Default::default(),
        });

        let incoming = Ok(DomainState {
            reachable: true,
            entry_observations_valid: false,
            invalidation: None,
            conditions: [(input, false)].into_iter().collect(),
            dependencies: Default::default(),
            total_calls: Default::default(),
            total_cleanup: Default::default(),
            observations: Default::default(),
            evaluated_values: Default::default(),
            call_completions: Default::default(),
        });

        assert!(merge(&mut target, &incoming, None));

        assert_eq!(
            target,
            Ok(DomainState {
                reachable: true,
                entry_observations_valid: false,
                invalidation: None,
                conditions: Default::default(),
                dependencies: Default::default(),
                total_calls: Default::default(),
                total_cleanup: Default::default(),
                observations: Default::default(),
                evaluated_values: Default::default(),
                call_completions: Default::default(),
            })
        );

        assert!(!merge(&mut target, &incoming, None));

        let unit = bray_bound_tree::BoundUnitId::new(1);
        let join = super::super::super::id::AnalysisBlockId::from_slot(unit, 3);

        let first = super::super::super::id::AnalysisObservationSite::Operation(
            super::super::super::id::AnalysisOperationId::from_slot(unit, 1),
        );

        let second = super::super::super::id::AnalysisObservationSite::Operation(
            super::super::super::id::AnalysisOperationId::from_slot(unit, 2),
        );

        let mut incoming = incoming;

        target.as_mut().unwrap().invalidation = Some(first);
        incoming.as_mut().unwrap().invalidation = Some(second);

        assert!(merge(&mut target, &incoming, Some(join)));

        assert_eq!(
            target.as_ref().unwrap().invalidation,
            Some(super::super::super::id::AnalysisObservationSite::BlockEntry(join))
        );

        assert!(!merge(&mut target, &incoming, Some(join)));

        let bound = bray_testing::test_bound_unit(1);
        let origin = bray_bound_tree::BoundNodeOrigin::source(bound.key().source());
        let mut tree = bray_bound_tree::BoundTreeBuilder::new(unit);

        let first = tree
            .push_block(bray_bound_tree::BoundBlock::new(origin, [], false))
            .unwrap();

        let second = tree
            .push_block(bray_bound_tree::BoundBlock::new(origin, [], false))
            .unwrap();

        let common = (first, bray_bound_tree::AnyBoundNodeId::Block(first));
        let branch_only = (second, bray_bound_tree::AnyBoundNodeId::Block(second));

        target
            .as_mut()
            .unwrap()
            .total_cleanup
            .extend([common, branch_only]);

        incoming.as_mut().unwrap().total_cleanup.insert(common);

        assert!(merge(&mut target, &incoming, Some(join)));

        assert_eq!(
            target.as_ref().unwrap().total_cleanup,
            [common].into_iter().collect()
        );

        assert!(!merge(&mut target, &incoming, Some(join)));

        incoming.as_mut().unwrap().total_cleanup.clear();

        assert!(merge(&mut target, &incoming, Some(join)));
        assert!(target.as_ref().unwrap().total_cleanup.is_empty());
        assert!(!merge(&mut target, &incoming, Some(join)));
    }
}
