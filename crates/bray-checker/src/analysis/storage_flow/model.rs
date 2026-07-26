use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BorrowCapabilityId, CheckedRefinementFacts, LivenessFacts, StorageAccessId,
    StorageAccessPlan, StorageAccessPurpose, StorageAccessRoot, StorageIdentity, StorageIdentityId,
    StoragePlan,
};
use bray_symbols::TypeId;

use crate::{CheckerRequestContext, CheckerUnitView};

use super::super::fixed_point::{FixedPointDomain, FlowDirection};
use super::super::model::{AnalysisBlock, AnalysisEdge, AnalysisEdgeKind, ControlFlowGraph};
use super::super::reachability::ReachabilityResult;
use super::check::StorageFlowCollector;

#[derive(Debug, Default)]
pub(super) struct StorageFlowInput {
    plans: BTreeMap<AnyBoundNodeId, Vec<StorageAccessPlan>>,
    borrows: BTreeMap<StorageAccessPlan, BorrowCapabilityId>,
    definitions: BTreeMap<AnyBoundNodeId, Vec<StorageIdentityId>>,
    copyable_types: BTreeSet<TypeId>,
    mutable_storage: BTreeSet<StorageIdentityId>,
}

impl StorageFlowInput {
    pub(super) fn new(
        storage: &StoragePlan,
        copyable_types: BTreeSet<TypeId>,
        mutable_storage: BTreeSet<StorageIdentityId>,
    ) -> Self {
        let mut input = Self {
            copyable_types,
            mutable_storage,
            ..Self::default()
        };

        let planned_borrows = storage
            .borrow_capability_entries()
            .filter_map(|(id, capability)| {
                capability
                    .expression()
                    .map(|expression| ((expression, capability.kind(), capability.access()), id))
            })
            .collect::<BTreeMap<_, _>>();

        for plan in storage.access_plans().iter().copied() {
            input
                .plans
                .entry(AnyBoundNodeId::Expression(plan.expression()))
                .or_default()
                .push(plan);

            let StorageAccessPurpose::Borrow(kind) = plan.purpose() else {
                continue;
            };

            let direct = storage
                .access(plan.access())
                .and_then(|access| match access.root() {
                    StorageAccessRoot::Borrow(capability) => Some(capability),
                    StorageAccessRoot::Storage(_)
                    | StorageAccessRoot::OwnedIndirection { .. }
                    | StorageAccessRoot::Recovery(_) => None,
                });

            let capability = planned_borrows
                .get(&(plan.expression(), kind, plan.access()))
                .copied()
                .or_else(|| {
                    direct.filter(|capability| {
                        storage
                            .borrow_capability(*capability)
                            .is_some_and(|capability| capability.kind() == kind)
                    })
                });

            if let Some(capability) = capability {
                input.borrows.insert(plan, capability);
            }
        }

        for (storage, identity) in storage.identity_entries() {
            let Some(node) = identity_definition_node(identity) else {
                continue;
            };

            input.definitions.entry(node).or_default().push(storage);
        }

        input
    }

    pub(super) fn plans(&self, node: AnyBoundNodeId) -> &[StorageAccessPlan] {
        self.plans.get(&node).map(Vec::as_slice).unwrap_or_default()
    }

    pub(super) fn borrow(&self, plan: StorageAccessPlan) -> Option<BorrowCapabilityId> {
        self.borrows.get(&plan).copied()
    }

    pub(super) fn definitions(&self, node: AnyBoundNodeId) -> &[StorageIdentityId] {
        self.definitions
            .get(&node)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub(super) fn type_is_copyable(&self, ty: TypeId) -> bool {
        self.copyable_types.contains(&ty)
    }

    pub(super) fn storage_is_mutable(&self, storage: StorageIdentityId) -> bool {
        self.mutable_storage.contains(&storage)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct StorageFlowState {
    pub(super) reachable: bool,
    pub(super) initialized: BTreeSet<StorageIdentityId>,
    pub(super) moved: BTreeSet<StorageAccessId>,
    pub(super) active_borrows: BTreeSet<BorrowCapabilityId>,
    pub(super) recovered: bool,
}

impl StorageFlowState {
    fn entry(storage: &StoragePlan) -> Self {
        let initialized = storage
            .identity_entries()
            .filter_map(|(id, identity)| identity_is_initialized_at_entry(identity).then_some(id))
            .collect();

        let active_borrows = storage
            .borrow_capability_entries()
            .filter_map(|(id, capability)| capability.entry_binding().is_some().then_some(id))
            .collect();

        Self {
            reachable: true,
            initialized,
            moved: BTreeSet::new(),
            active_borrows,
            recovered: false,
        }
    }

    fn merge(&mut self, incoming: &Self) -> bool {
        if !incoming.reachable {
            return false;
        }

        if !self.reachable {
            // Each block owns its task-local state after propagation.
            *self = incoming.clone();

            return true;
        }

        let initialized_count = self.initialized.len();
        let moved_count = self.moved.len();
        let borrow_count = self.active_borrows.len();
        let was_recovered = self.recovered;

        self.initialized
            .retain(|storage| incoming.initialized.contains(storage));

        self.moved.extend(incoming.moved.iter().copied());

        self.active_borrows
            .extend(incoming.active_borrows.iter().copied());

        self.recovered |= incoming.recovered;

        self.initialized.len() != initialized_count
            || self.moved.len() != moved_count
            || self.active_borrows.len() != borrow_count
            || self.recovered != was_recovered
    }
}

pub(super) struct StorageFlowDomain<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    graph: &'analysis ControlFlowGraph,
    reachability: &'analysis ReachabilityResult,
    storage: &'analysis StoragePlan,
    liveness: &'analysis LivenessFacts,
    refinements: &'analysis CheckedRefinementFacts,
    input: &'analysis StorageFlowInput,
    request: CheckerUnitView<'analysis, C>,
}

impl<'analysis, C> StorageFlowDomain<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn new(
        graph: &'analysis ControlFlowGraph,
        reachability: &'analysis ReachabilityResult,
        storage: &'analysis StoragePlan,
        liveness: &'analysis LivenessFacts,
        refinements: &'analysis CheckedRefinementFacts,
        input: &'analysis StorageFlowInput,
        request: CheckerUnitView<'analysis, C>,
    ) -> Self {
        Self {
            graph,
            reachability,
            storage,
            liveness,
            refinements,
            input,
            request,
        }
    }

    fn transfer_block(&self, block: &AnalysisBlock, source: &StorageFlowState) -> StorageFlowState {
        // Transfer owns an independent task-local successor state.
        let mut state = source.clone();

        let mut collector = StorageFlowCollector::without_publication(
            self.request,
            self.storage,
            self.liveness,
            self.refinements,
            self.input,
        );

        for operation in block.operations() {
            let Some(operation) = self.graph.operation(*operation) else {
                continue;
            };

            collector.apply_operation(&mut state, operation);
        }

        state
    }
}

impl<C> FixedPointDomain for StorageFlowDomain<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    type State = StorageFlowState;

    fn direction(&self) -> FlowDirection {
        FlowDirection::Forward
    }

    fn bottom(&self) -> Self::State {
        StorageFlowState::default()
    }

    fn boundary(&self) -> Self::State {
        StorageFlowState::entry(self.storage)
    }

    fn merge_boundary(&self, target: &mut Self::State, boundary: &Self::State) -> bool {
        target.merge(boundary)
    }

    fn transfer(&self, block: &AnalysisBlock, source: &Self::State) -> Self::State {
        self.transfer_block(block, source)
    }

    fn propagate(
        &self,
        source: &Self::State,
        edge: &AnalysisEdge,
        target: &mut Self::State,
    ) -> bool {
        if !self.reachability.is_edge_reachable(edge.id()) {
            return false;
        }

        if edge.kind() == AnalysisEdgeKind::Recovery {
            let mut incoming = source.clone();
            incoming.recovered = true;

            return target.merge(&incoming);
        }

        target.merge(source)
    }

    fn convergence_bound(&self, graph: &ControlFlowGraph) -> usize {
        let domain_size = self
            .storage
            .identities()
            .len()
            .saturating_add(self.storage.accesses().len())
            .saturating_add(self.storage.borrow_capabilities().len())
            .saturating_add(2);

        graph.blocks().len().saturating_mul(domain_size)
    }
}

const fn identity_is_initialized_at_entry(identity: StorageIdentity) -> bool {
    matches!(
        identity,
        StorageIdentity::Parameter(_)
            | StorageIdentity::Receiver(_)
            | StorageIdentity::AnonymousParameter(_)
            | StorageIdentity::PredicateParameter(_)
    )
}

fn identity_definition_node(identity: StorageIdentity) -> Option<AnyBoundNodeId> {
    match identity {
        StorageIdentity::LocalOwned(definition) => Some(definition),
        StorageIdentity::Temporary(expression)
        | StorageIdentity::IterationCursor(expression)
        | StorageIdentity::IterationElement(expression)
        | StorageIdentity::Allocation(expression) => Some(AnyBoundNodeId::Expression(expression)),
        StorageIdentity::Parameter(_)
        | StorageIdentity::Receiver(_)
        | StorageIdentity::AnonymousParameter(_)
        | StorageIdentity::PredicateParameter(_)
        | StorageIdentity::Result(_)
        | StorageIdentity::CompilerCreated(_)
        | StorageIdentity::Error(_) => None,
    }
}
