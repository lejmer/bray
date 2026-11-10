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
    const POINTER_PREDICATES: &[(&str, bool, &[usize])] = &[
        ("ValidRead", true, &[0]),
        ("ValidWrite", true, &[0]),
        ("AlignedFor", true, &[0]),
        ("CallableAddressValid", true, &[0]),
        ("NonOverlapping", true, &[0, 2]),
        ("DeviceValidRead", true, &[0]),
        ("DeviceValidWrite", true, &[0]),
        ("DeviceAlignedFor", true, &[0]),
        ("InitializedAs", false, &[0]),
        ("InitializedRangeAs", false, &[0]),
        ("DeviceInitializedAs", false, &[0]),
        ("SharedAliasValid", false, &[0]),
        ("ExclusiveAliasValid", false, &[0]),
        ("EpochCurrent", false, &[0]),
        ("SynchronizedAccess", false, &[0]),
        ("MovementStable", false, &[0]),
        ("FinalizationPending", false, &[0]),
    ];
    let (pointer_predicates, mut content_predicates): (BTreeMap<_, _>, BTreeSet<_>) = request
        .trusted_contracts()
        .into_iter()
        .flat_map(|_| POINTER_PREDICATES.iter().copied())
        .filter_map(|(name, preserves_contents, pointers)| {
            let key = bray_compiler_known::CompilerKnownDeclarationKey::try_new(name)
                .expect("closed pointer predicate key must be valid");

            request
                .context()
                .available_compiler_known_symbols()
                .provider()
                .declaration_symbol::<bray_symbols::PredicateSymbolId>(&key)
                .map(|predicate| (predicate.into(), preserves_contents, pointers))
        })
        .fold((BTreeMap::new(), BTreeSet::new()), |(mut pointers, mut contents), (predicate, preserves_contents, arguments)| {
            pointers.insert(predicate, arguments);

            // Initialization observes pointer values too, but writes must still
            // invalidate it independently of spatial permissions.
            if preserves_contents { contents.insert(predicate); }

            (pointers, contents)
        });

    let owned_allocation = request
        .context()
        .available_compiler_known_symbols()
        .provider()
        .declaration_symbol::<bray_symbols::PredicateSymbolId>(
            &bray_compiler_known::CompilerKnownDeclarationKey::try_new("OwnedAllocation")
                .expect("closed allocation predicate key must be valid"),
        )
        .map(Into::into);

    let initialized_range = request
        .context()
        .available_compiler_known_symbols()
        .provider()
        .declaration_symbol::<bray_symbols::PredicateSymbolId>(
            &bray_compiler_known::CompilerKnownDeclarationKey::try_new("InitializedRangeAs")
                .expect("closed initialized range predicate key must be valid"),
        )
        .map(Into::into);

    content_predicates.extend(owned_allocation);

    ExecutionFlowDomain {
        dependencies,
        sequence_length: request
            .context()
            .available_compiler_known_symbols()
            .provider()
            .declaration_symbols()
            .get(
                &bray_compiler_known::CompilerKnownDeclarationKey::try_new("SequenceLength")
                    .expect("closed sequence length key must be valid"),
            )
            .copied(),
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
        union_variants: &prepared.union_variants,
        literals: &prepared.literals,
        storage,
        contracts,
        disposal,
        refinements,
        memory,
        checked_body: None,
        pointer_predicates,
        content_predicates,
        owned_allocation,
        initialized_range,
    }
}
