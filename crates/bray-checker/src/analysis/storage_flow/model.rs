use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BorrowCapabilityId, BoundBlockItem, BoundExpressionId,
    CheckedMemoryOperations, CheckedRefinementFacts, LivenessFacts, StorageAccessId,
    StorageAccessPlan, StorageAccessPurpose, StorageAccessRoot, StorageBinding,
    StorageBindingTarget, StorageIdentity, StorageIdentityId, StoragePlan,
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
    initialization_destinations: BTreeMap<BoundExpressionId, StorageIdentityId>,
    copyable_types: BTreeSet<TypeId>,
    mutable_storage: BTreeSet<StorageIdentityId>,
}

impl StorageFlowInput {
    pub(super) fn new<C>(
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
        copyable_types: BTreeSet<TypeId>,
        mutable_storage: BTreeSet<StorageIdentityId>,
    ) -> Self
    where
        C: CheckerRequestContext + ?Sized,
    {
        let initialization_destinations = local_initialization_destinations(request, storage);

        let mut input = Self {
            copyable_types,
            initialization_destinations,
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

    pub(super) fn initialization_destination(
        &self,
        expression: BoundExpressionId,
    ) -> Option<StorageIdentityId> {
        self.initialization_destinations.get(&expression).copied()
    }

    pub(super) fn type_is_copyable(&self, ty: TypeId) -> bool {
        self.copyable_types.contains(&ty)
    }

    pub(super) fn storage_is_mutable(&self, storage: StorageIdentityId) -> bool {
        self.mutable_storage.contains(&storage)
    }
}

fn local_initialization_destinations<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
) -> BTreeMap<BoundExpressionId, StorageIdentityId>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut result = BTreeMap::new();

    for (_, block) in request.unit().tree().blocks() {
        for item in block.items() {
            let BoundBlockItem::LocalBinding(binding) = item else {
                continue;
            };

            let destinations = binding
                .bindings()
                .iter()
                .filter_map(|binding| {
                    match storage.binding(StorageBindingTarget::Local(*binding)) {
                        Some(StorageBinding::Identity(destination)) => Some(destination),
                        Some(StorageBinding::Access(_)) | None => None,
                    }
                })
                .collect::<Vec<_>>();

            if let [destination] = destinations.as_slice() {
                result.insert(binding.initializer(), *destination);
            }
        }
    }

    result
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct StorageFlowState {
    pub(super) reachable: bool,
    pub(super) live: BTreeSet<StorageIdentityId>,
    pub(super) initialized: BTreeSet<StorageIdentityId>,
    pub(super) moved: BTreeSet<StorageAccessId>,
    pub(super) active_borrows: BTreeSet<BorrowCapabilityId>,
    pub(super) definitely_active_borrows: BTreeSet<BorrowCapabilityId>,
    pub(super) raw_initialized: BTreeMap<StorageIdentityId, BTreeSet<TypeId>>,
    pub(super) active_allocations: BTreeSet<StorageIdentityId>,
    pub(super) invalidated_allocations: BTreeSet<StorageIdentityId>,
    pub(super) recovered: bool,
}

impl StorageFlowState {
    fn entry(storage: &StoragePlan) -> Self {
        let initialized = storage
            .identity_entries()
            .filter_map(|(id, identity)| identity.is_initialized_at_entry().then_some(id))
            .collect::<BTreeSet<_>>();

        let active_borrows = storage
            .borrow_capability_entries()
            .filter_map(|(id, capability)| capability.entry_binding().is_some().then_some(id))
            .collect::<BTreeSet<_>>();

        Self {
            reachable: true,
            live: initialized.clone(),
            initialized,
            moved: BTreeSet::new(),
            definitely_active_borrows: active_borrows.clone(),
            active_borrows,
            raw_initialized: BTreeMap::new(),
            active_allocations: BTreeSet::new(),
            invalidated_allocations: BTreeSet::new(),
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

        let live_count = self.live.len();
        let initialized_count = self.initialized.len();
        let moved_count = self.moved.len();
        let borrow_count = self.active_borrows.len();
        let definite_borrow_count = self.definitely_active_borrows.len();
        let raw_storage_count = self.raw_initialized.len();

        let raw_initialized_count = self
            .raw_initialized
            .values()
            .map(BTreeSet::len)
            .sum::<usize>();

        let active_allocation_count = self.active_allocations.len();
        let invalidated_allocation_count = self.invalidated_allocations.len();
        let was_recovered = self.recovered;

        self.live.retain(|storage| incoming.live.contains(storage));

        self.initialized
            .retain(|storage| incoming.initialized.contains(storage));

        self.moved.extend(incoming.moved.iter().copied());

        self.active_borrows
            .extend(incoming.active_borrows.iter().copied());

        self.definitely_active_borrows
            .retain(|borrow| incoming.definitely_active_borrows.contains(borrow));

        self.raw_initialized.retain(|storage, initialized| {
            let Some(incoming) = incoming.raw_initialized.get(storage) else {
                return false;
            };

            initialized.retain(|ty| incoming.contains(ty));

            !initialized.is_empty()
        });

        self.active_allocations
            .retain(|storage| incoming.active_allocations.contains(storage));

        self.invalidated_allocations
            .extend(incoming.invalidated_allocations.iter().copied());

        self.recovered |= incoming.recovered;

        self.live.len() != live_count
            || self.initialized.len() != initialized_count
            || self.moved.len() != moved_count
            || self.active_borrows.len() != borrow_count
            || self.definitely_active_borrows.len() != definite_borrow_count
            || self.raw_initialized.len() != raw_storage_count
            || self
                .raw_initialized
                .values()
                .map(BTreeSet::len)
                .sum::<usize>()
                != raw_initialized_count
            || self.active_allocations.len() != active_allocation_count
            || self.invalidated_allocations.len() != invalidated_allocation_count
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
    memory: &'analysis CheckedMemoryOperations,
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
        memory: &'analysis CheckedMemoryOperations,
        input: &'analysis StorageFlowInput,
        request: CheckerUnitView<'analysis, C>,
    ) -> Self {
        Self {
            graph,
            reachability,
            storage,
            liveness,
            refinements,
            memory,
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
            self.memory,
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
            .saturating_add(self.storage.identities().len().saturating_mul(3))
            .saturating_add(2);

        graph.blocks().len().saturating_mul(domain_size)
    }
}

fn identity_definition_node(identity: StorageIdentity) -> Option<AnyBoundNodeId> {
    match identity {
        StorageIdentity::LocalOwned(definition) => Some(definition),
        StorageIdentity::Temporary(expression)
        | StorageIdentity::IterationCursor(expression)
        | StorageIdentity::IterationElement(expression)
        | StorageIdentity::Allocation(expression) => Some(AnyBoundNodeId::Expression(expression)),
        StorageIdentity::Alternative { pattern, .. } => Some(AnyBoundNodeId::Pattern(pattern)),
        StorageIdentity::Parameter(_)
        | StorageIdentity::Receiver(_)
        | StorageIdentity::AnonymousParameter(_)
        | StorageIdentity::PredicateParameter(_)
        | StorageIdentity::PostconditionResult(_)
        | StorageIdentity::Result(_)
        | StorageIdentity::CompilerCreated(_)
        | StorageIdentity::Error(_) => None,
    }
}
