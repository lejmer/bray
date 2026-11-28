use super::model::{ExecutionFlow, ExecutionFlowDomain};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{BoundExpressionId, CheckedExpressionSemantics};

use crate::execution_guarantees::ExecutionCondition;
use crate::{CheckerRequestContext, CheckerUnitView};

use super::super::super::fixed_point::{FixedPointOutcome, solve_fixed_point_with};
use super::super::super::model::AnalysisBlock;
use super::super::super::outcomes::FlowOutcomes;
use super::super::state::ExecutionState;

impl<'a, 'view, C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'a, 'view, C> {
    pub(in crate::analysis::guarantee) fn solve(
        mut self,
        assumptions: &[ExecutionCondition],
        excluded_edges: BTreeSet<crate::analysis::id::AnalysisEdgeId>,
    ) -> Option<ExecutionFlow<'a, 'view, C>> {
        let entry = self.assumptions.to_mut();

        entry.clear();
        entry.extend_from_slice(assumptions);
        self.excluded_edges = excluded_edges;
        self.candidate_evidence.get_mut().expect("candidate evidence lock is not poisoned").clear();
        self.consumed_completion_dependencies.get_mut().expect("completion dependency lock is not poisoned").clear();

        let mut reached_acyclic = BTreeSet::new();

        let transfer = |block: &AnalysisBlock, input: FlowOutcomes<ExecutionState>| {
            if input.states().next().is_some() {
                reached_acyclic.insert(block.id());
            }

            self.transfer_owned(block, input)
        };

        let states = match solve_fixed_point_with(self.graph, &self, &self.request, Some(transfer)) {
            FixedPointOutcome::Complete(states) => states,
            FixedPointOutcome::Cancelled => return None,
            FixedPointOutcome::ConvergenceInvariantViolated => {
                panic!("execution condition flow exceeded its finite convergence bound");
            }
        };

        Some(ExecutionFlow { domain: self, states, reached_acyclic })
    }
}

impl<'a, 'view, C: CheckerRequestContext + ?Sized> ExecutionFlow<'a, 'view, C> {
    pub(in crate::analysis::guarantee) fn into_domain(self) -> ExecutionFlowDomain<'a, 'view, C> {
        let ExecutionFlow { mut domain, states, .. } = self;

        drop(states);
        domain.proofs = Default::default();

        domain
    }

    pub(in crate::analysis::guarantee) fn matches_inputs(
        &self,
        assumptions: &[ExecutionCondition],
        excluded: &BTreeSet<crate::analysis::id::AnalysisEdgeId>,
    ) -> bool {
        self.domain.assumptions.as_ref() == assumptions && &self.domain.excluded_edges == excluded
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "flow preparation consumes tables from independent checker queries without hidden lookups"
)]
pub(crate) fn prepare_execution_flow<'a, 'view, C: CheckerRequestContext + ?Sized>(
    prepared: &'a crate::PreparedBodyAnalysis,
    request: CheckerUnitView<'view, C>,
    semantics: &'a CheckedExpressionSemantics,
    assumptions: Cow<'a, [ExecutionCondition]>,
    storage: &'a bray_bound_tree::StoragePlan,
    memory: &'a bray_bound_tree::CheckedMemoryOperations,
    contracts: &'a BTreeMap<BoundExpressionId, Vec<crate::ExecutionCompletionContract>>,
    dependencies: &'a bray_bound_tree::CheckedDependencyContracts,
    liveness: &'a bray_bound_tree::Liveness,
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

    let mut retained_storage = BTreeSet::new();

    for call in dependencies.calls().values() {
        collect_retained_storage(call.result().requirements(), &mut retained_storage);

        for report in call.report_contracts() {
            collect_retained_storage(report.requirements(), &mut retained_storage);
        }
    }

    ExecutionFlowDomain {
        candidate_evidence: Default::default(),
        storage_places: Default::default(),
        consumed_completion_dependencies: Default::default(),
        // Exact immutable query results survive cleanup-feasibility rounds within this body.
        proofs: Default::default(),
        dependencies,
        liveness,
        retained_storage,
        sequence_length: request
            .context()
            .available_compiler_known_symbols()
            .provider()
            .declaration_symbols()
            .get(
                &bray_compiler_known::CompilerKnownDeclarationKey::try_new("SequenceLength")
                    .expect("closed sequence length key must be valid"),
            )
            .map(|symbol| {
                let definition = bray_symbols::CallableDefinitionId::try_new(*symbol)
                    .expect("the sequence length declaration is callable");

                let owner = bray_symbols::GenericOwnerId::try_new(*symbol)
                    .expect("the sequence length declaration owns an empty substitution");

                let substitution = bray_symbols::GenericSubstitutionData::try_new(owner, [], [])
                    .expect("sequence length has no generic parameters");

                let substitution = request.semantic_values().intern_generic_substitution(substitution)
                    .expect("the sequence length substitution belongs to this semantic store");

                bray_symbols::CallableInstanceData::new(definition, substitution)
            }),
        has_bounds_checks: prepared.has_bounds_checks,
        certify_predicate: false,
        observed_types: &prepared.observed_types,
        absent_at_entry: &prepared.absent_at_entry,
        pattern_conditions: &prepared.pattern_conditions,
        borrowed_inputs: &prepared.borrowed_inputs,
        values: &prepared.values,
        scope_places: &prepared.scope_places,
        yield_targets: &prepared.yield_targets,
        generator_observations: &prepared.generator_observations,
        graph: &prepared.graph,
        completion_blocks: prepared.graph.completion_exits().map(|exit| exit.block()).collect(),
        excluded_edges: BTreeSet::new(),
        result_edges: &prepared.result_edges,
        invocation_failures: &prepared.invocation_failures,
        request,
        semantics,
        assumptions,
        input_invariants: &prepared.input_invariants,
        union_variants: &prepared.union_variants,
        literals: &prepared.literals,
        storage,
        contracts,
        disposal: &prepared.cleanup,
        memory,
        checked_body: None,
        pointer_predicates,
        content_predicates,
        owned_allocation,
        initialized_range,
    }
}

fn collect_retained_storage(
    requirements: &[bray_bound_tree::BoundDependencyRequirement],
    output: &mut BTreeSet<bray_bound_tree::StorageAccessId>,
) {
    for requirement in requirements {
        if let Some(access) = requirement.retained_storage_access() {
            output.insert(access);
        } else if let bray_bound_tree::BoundDependencyRequirement::Guarded(guard) = requirement {
            collect_retained_storage(guard.requirements(), output);
        }
    }
}
