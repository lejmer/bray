use super::model::{ExecutionFlow, ExecutionFlowDomain};
use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{BoundExpressionId, CheckedExpressionSemantics};

use crate::execution_guarantees::ExecutionCondition;
use crate::{CheckerOutcome, CheckerRequestContext, CheckerUnitView};

use super::super::super::fixed_point::{FixedPointOutcome, solve_fixed_point};

#[expect(
    clippy::too_many_arguments,
    reason = "flow analysis consumes tables from independent checker queries without hidden lookups"
)]
pub(in crate::analysis::guarantee) fn analyze_execution_flow<
    'a,
    'view,
    C: CheckerRequestContext + ?Sized,
>(
    prepared: &'a crate::PreparedBodyAnalysis,
    excluded_edges: BTreeSet<crate::analysis::id::AnalysisEdgeId>,
    request: CheckerUnitView<'view, C>,
    semantics: &'a CheckedExpressionSemantics,
    assumptions: &'a [ExecutionCondition],
    storage: &'a bray_bound_tree::StoragePlan,
    memory: &'a bray_bound_tree::CheckedMemoryOperations,
    checked_body: &'a bray_bound_tree::CheckedBodySemantics,
    refinements: &'a std::collections::BTreeMap<
        bray_bound_tree::BoundExecutionSite,
        bray_bound_tree::StorageInvalidation,
    >,
    contracts: &'a BTreeMap<BoundExpressionId, Vec<crate::ExecutionCompletionContract>>,
    disposal: &'a super::super::cleanup::Cleanup<'a>,
    dependencies: &'a bray_bound_tree::CheckedDependencyContracts,
) -> CheckerOutcome<ExecutionFlow<'a, 'view, C>, C::UpstreamError> {
    let mut domain = prepare_execution_flow(
        prepared,
        request,
        semantics,
        assumptions,
        storage,
        memory,
        refinements,
        contracts,
        disposal,
        dependencies,
    );

    domain.excluded_edges = excluded_edges;
    domain.checked_body = Some(checked_body);

    let states = match solve_fixed_point(&prepared.graph, &domain, &request) {
        FixedPointOutcome::Complete(states) => states,
        FixedPointOutcome::Cancelled => return CheckerOutcome::Cancelled,
        FixedPointOutcome::ConvergenceInvariantViolated => {
            panic!("execution condition flow exceeded its finite convergence bound");
        }
    };

    CheckerOutcome::without_diagnostics(ExecutionFlow { domain, states })
}

#[expect(
    clippy::too_many_arguments,
    reason = "flow preparation consumes tables from independent checker queries without hidden lookups"
)]
pub(crate) fn prepare_execution_flow<'a, 'view, C: CheckerRequestContext + ?Sized>(
    prepared: &'a crate::PreparedBodyAnalysis,
    request: CheckerUnitView<'view, C>,
    semantics: &'a CheckedExpressionSemantics,
    assumptions: &'a [ExecutionCondition],
    storage: &'a bray_bound_tree::StoragePlan,
    memory: &'a bray_bound_tree::CheckedMemoryOperations,
    refinements: &'a std::collections::BTreeMap<
        bray_bound_tree::BoundExecutionSite,
        bray_bound_tree::StorageInvalidation,
    >,
    contracts: &'a BTreeMap<BoundExpressionId, Vec<crate::ExecutionCompletionContract>>,
    disposal: &'a super::super::cleanup::Cleanup<'a>,
    dependencies: &'a bray_bound_tree::CheckedDependencyContracts,
) -> ExecutionFlowDomain<'a, 'view, C> {
    ExecutionFlowDomain {
        dependencies,
        sequence_lengths: &prepared.sequence_lengths,
        has_bounds_checks: prepared.has_bounds_checks,
        observed_types: &prepared.observed_types,
        pattern_conditions: &prepared.pattern_conditions,
        borrowed_inputs: &prepared.borrowed_inputs,
        values: &prepared.values,
        scope_places: &prepared.scope_places,
        yield_targets: &prepared.yield_targets,
        generator_observations: &prepared.generator_observations,
        graph: &prepared.graph,
        excluded_edges: BTreeSet::new(),
        request,
        semantics,
        assumptions,
        input_invariants: &prepared.input_invariants,
        literals: &prepared.literals,
        storage,
        contracts,
        disposal,
        refinements,
        memory,
        checked_body: None,
        spatial_predicates: request
            .trusted_contracts()
            .filter(|contracts| !contracts.requirements.is_empty() || !contracts.calls.is_empty())
            .into_iter()
            .flat_map(|_| {
                [
                    "ValidRead",
                    "ValidWrite",
                    "AlignedFor",
                    "DeviceValidRead",
                    "DeviceValidWrite",
                    "DeviceAlignedFor",
                ]
            })
            .filter_map(|name| {
                let key = bray_compiler_known::CompilerKnownDeclarationKey::try_new(name)
                    .expect("closed spatial predicate key must be valid");

                request
                    .context()
                    .available_compiler_known_symbols()
                    .provider()
                    .declaration_symbol::<bray_symbols::PredicateSymbolId>(&key)
                    .map(Into::into)
            })
            .collect(),
        owned_allocation: request
            .context()
            .available_compiler_known_symbols()
            .provider()
            .declaration_symbol::<bray_symbols::PredicateSymbolId>(
                &bray_compiler_known::CompilerKnownDeclarationKey::try_new("OwnedAllocation")
                    .expect("closed allocation predicate key must be valid"),
            )
            .map(Into::into),
        initialized_range: request
            .context()
            .available_compiler_known_symbols()
            .provider()
            .declaration_symbol::<bray_symbols::PredicateSymbolId>(
                &bray_compiler_known::CompilerKnownDeclarationKey::try_new("InitializedRangeAs")
                    .expect("closed initialized range predicate key must be valid"),
            )
            .map(Into::into),
    }
}
