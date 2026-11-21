use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_bound_tree::{
    AsyncStorageExitDecision, BorrowCapabilityId, CheckedDependencyContracts, StorageAbsence, StorageAccessId,
    StorageAccessPlan, StorageCleanupAction, StorageCleanupEntry, StorageCleanupSchedule, StorageExitDecision,
    StorageExitPoint, StorageIdentityId, StoragePath, StoragePlan, StorageTarget,
};

use crate::analysis::storage_flow::model::StorageFlowState;
use crate::analysis::id::{AnalysisBlockId, AnalysisOperationId};
use crate::analysis::guarantee::CleanupSelection;
use crate::analysis::model::{AnalysisOperation, AnalysisOperationKind, AnalysisScopeExitPhase};

pub(super) struct BlockRecords<Key, Value>(BTreeMap<AnalysisBlockId, Vec<(Key, Value)>>);

impl<Key, Value> Default for BlockRecords<Key, Value> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<Key: Ord, Value> BlockRecords<Key, Value> {
    pub(super) fn record(&mut self, block: AnalysisBlockId, key: Key, value: Value) {
        self.0.entry(block).or_default().push((key, value));
    }

    pub(super) fn extend(&mut self, incoming: Self) {
        self.0.extend(incoming.0);
    }

    pub(super) fn publish(self, merge: impl Fn(&mut Value, &Value)) -> BTreeMap<Key, Value> {
        let mut published = BTreeMap::new();

        for (key, value) in self.0.into_values().flatten() {
            match published.entry(key) {
                std::collections::btree_map::Entry::Vacant(entry) => { entry.insert(value); }
                std::collections::btree_map::Entry::Occupied(mut entry) => merge(entry.get_mut(), &value),
            }
        }

        published
    }
}

#[derive(Default)]
pub(super) struct CleanupSnapshots {
    exits: BTreeMap<StorageExitPoint, ScopeExitSnapshot>,
    entries: BTreeMap<(AnalysisOperationId, Option<StorageCleanupAction>), ReachedCleanup>,
}

#[derive(Default)]
struct ScopeExitSnapshot {
    broadcast: BTreeSet<StorageAccessId>,
    lifecycle_accesses: Option<ReachedLifecycle>,
    cleanup_recoveries: BTreeSet<AsyncStorageExitDecision>,
    phase_recovered: bool,
    lifecycle: Option<ExitState>,
}

struct ReachedCleanup {
    orders: Option<ReachedLifecycle>,
    absent: Arc<BTreeMap<StoragePath, StorageAbsence>>,
    replacement: Option<StorageTarget>,
}

enum ReachedLifecycle {
    Ordered(Arc<[StorageAccessId]>),
    Multiple(BTreeSet<Arc<[StorageAccessId]>>),
}

impl ReachedLifecycle {
    fn record(&mut self, incoming: Arc<[StorageAccessId]>) {
        match self {
            Self::Ordered(current) if is_subsequence(&incoming, current) => {}
            Self::Ordered(current) if is_subsequence(current, &incoming) => *current = incoming,
            Self::Ordered(current) => {
                *self = Self::Multiple([Arc::clone(current), incoming].into());
            }
            Self::Multiple(orders) => { orders.insert(incoming); }
        }
    }

    fn accesses(&self) -> impl Iterator<Item = StorageAccessId> + '_ {
        let (ordered, alternatives) = match self {
            Self::Ordered(accesses) => (Some(accesses.as_ref()), None),
            Self::Multiple(accesses) => (None, Some(accesses)),
        };

        ordered.into_iter().flatten()
            .chain(alternatives.into_iter().flatten().flat_map(|order| order.iter())).copied()
    }
}

fn is_subsequence(selection: &[StorageAccessId], ordered: &[StorageAccessId]) -> bool {
    let mut ordered = ordered.iter();

    selection.iter().all(|access| ordered.any(|candidate| access == candidate))
}

impl CleanupSnapshots {
    pub(super) fn merge_block(&mut self, incoming: Self) {
        self.entries.extend(incoming.entries);

        super::core::merge_records(&mut self.exits, incoming.exits, |snapshot, reached| {
            snapshot.broadcast.extend(reached.broadcast);
            snapshot.cleanup_recoveries.extend(reached.cleanup_recoveries);
            snapshot.phase_recovered |= reached.phase_recovered;

            if let Some(incoming) = reached.lifecycle_accesses {
                match &mut snapshot.lifecycle_accesses {
                    Some(known) => match incoming {
                        ReachedLifecycle::Ordered(order) => known.record(order),
                        ReachedLifecycle::Multiple(orders) => {
                            for order in orders {
                                known.record(order);
                            }
                        }
                    },
                    None => snapshot.lifecycle_accesses = Some(incoming),
                }
            }

            if let Some(incoming) = reached.lifecycle {
                match &mut snapshot.lifecycle {
                    Some(known) => known.merge_snapshot(incoming),
                    None => snapshot.lifecycle = Some(incoming),
                }
            }
        });
    }

    pub(super) fn record_entry(
        &mut self,
        operation: AnalysisOperationId,
        selection: &CleanupSelection,
        absent: &Arc<BTreeMap<StoragePath, StorageAbsence>>,
    ) {
        use std::collections::btree_map::Entry;

        let order = selection.accesses.as_ref().filter(|order| !order.is_empty());

        match self.entries.entry((operation, selection.after)) {
            Entry::Vacant(entry) => {
                entry.insert(ReachedCleanup {
                    orders: order.map(|order| ReachedLifecycle::Ordered(Arc::clone(order))),
                    absent: Arc::clone(absent),
                    // The cleanup transition also uses this reached address.
                    replacement: selection.replacement.clone(),
                });
            }
            Entry::Occupied(mut entry) => {
                let reached = entry.get_mut();

                if let Some(order) = order {
                    match &mut reached.orders {
                        Some(previous) => previous.record(Arc::clone(order)),
                        None => reached.orders = Some(ReachedLifecycle::Ordered(Arc::clone(order))),
                    }
                }

                if !Arc::ptr_eq(&reached.absent, absent)
                    && reached.absent.keys().any(|path| !absent.contains_key(path))
                {
                    Arc::make_mut(&mut reached.absent).retain(|path, _| absent.contains_key(path));
                }

                if reached.replacement != selection.replacement {
                    reached.replacement = None;
                }
            }
        }
    }

    pub(super) fn record_phase(
        &mut self,
        point: &AnalysisOperation,
        state: &StorageFlowState,
        selection: CleanupSelection,
    ) {
        let AnalysisOperationKind::ScopeExit { scope, exit, phase, .. } = point.kind() else {
            unreachable!("cleanup phase selection belongs to a scope exit");
        };

        let snapshot = self.exits.entry(StorageExitPoint::new(scope, exit)).or_default();

        snapshot.cleanup_recoveries.extend(selection.recoveries);
        snapshot.phase_recovered |= state.recovered;

        let accesses = selection.accesses.filter(|accesses| !accesses.is_empty());

        match phase {
            AnalysisScopeExitPhase::TaskCancellationBroadcast => {
                if let Some(accesses) = accesses {
                    snapshot.broadcast.extend(accesses.iter().copied());
                }
            }
            AnalysisScopeExitPhase::LifecycleResolution => {
                if let Some(accesses) = accesses {
                    match &mut snapshot.lifecycle_accesses {
                        Some(previous) => previous.record(accesses),
                        None => snapshot.lifecycle_accesses = Some(ReachedLifecycle::Ordered(accesses)),
                    }
                }
            }
        }
    }

    pub(super) fn record_lifecycle(&mut self, point: StorageExitPoint, state: &StorageFlowState) {
        let snapshot = self.exits.entry(point).or_default();

        match &mut snapshot.lifecycle {
            Some(previous) => previous.merge(state),
            None => snapshot.lifecycle = Some(ExitState::new(state)),
        }
    }

    pub(super) fn decisions<'a>(
        &'a self,
        storage: &'a StoragePlan,
        dependencies: &'a CheckedDependencyContracts,
        graph: &'a crate::analysis::model::ControlFlowGraph,
    ) -> impl Iterator<Item = StorageExitDecision> + 'a {
        self.exits.iter().map(move |(point, snapshot)| {
            let mut broadcast = snapshot.broadcast.iter().copied().collect::<Vec<_>>();

            // Forward selection visits each requirement in reverse declaration order.
            broadcast.sort_unstable_by_key(|access| {
                Reverse(storage.root_identity(*access).expect("broadcast access retains its storage root"))
            });

            let state = snapshot.lifecycle.as_ref();

            let lifecycle = match &snapshot.lifecycle_accesses {
                None => StorageCleanupSchedule::Fixed(Arc::from([])),
                Some(ReachedLifecycle::Ordered(order)) => StorageCleanupSchedule::Fixed(Arc::clone(order)),
                Some(selection @ ReachedLifecycle::Multiple(_)) => {
                    let accesses = selection.accesses().collect::<BTreeSet<_>>();

                    dependencies.conditional_cleanup_schedule(storage, accesses.into_iter().collect())
                }
            };

            StorageExitDecision::new(
                point.scope(),
                point.exit(),
                state.into_iter().flat_map(|state| state.live.iter().copied()),
                state.into_iter().flat_map(|state| state.initialized.iter().copied()),
                state.into_iter().flat_map(|state| state.moved.iter().cloned()),
                state.into_iter().flat_map(|state| state.definitely_absent.iter().cloned()),
                state.into_iter().flat_map(|state| state.fully_moved.iter().copied()),
                state.into_iter().flat_map(|state| state.active_borrows.iter().copied()),
                snapshot.phase_recovered || !snapshot.cleanup_recoveries.is_empty() || state.is_some_and(|state| state.recovered),
            ).with_scoped_exit(graph.scoped_uses().get(&point.scope()).copied()).with_cleanup_selection(
                broadcast,
                lifecycle,
                snapshot.cleanup_recoveries.iter().copied(),
            )
        })
    }

    pub(super) fn into_entries(self) -> BTreeMap<(AnalysisOperationId, Option<StorageCleanupAction>), StorageCleanupEntry> {
        self.entries.into_iter().map(|(point, entry)| {
            let orders = match entry.orders {
                None => Arc::from([]),
                Some(ReachedLifecycle::Ordered(accesses)) => Arc::from([accesses]),
                Some(ReachedLifecycle::Multiple(orders)) => orders.into_iter().collect(),
            };

            (point, StorageCleanupEntry::new(orders, entry.absent, entry.replacement))
        }).collect()
    }
}

#[derive(Debug)]
pub(super) struct ExitState {
    pub(super) live: BTreeSet<StorageIdentityId>,
    pub(super) initialized: BTreeSet<StorageIdentityId>,
    pub(super) moved: BTreeSet<bray_bound_tree::StoragePath>,
    pub(super) definitely_absent: BTreeSet<bray_bound_tree::StoragePath>,
    pub(super) fully_moved: BTreeSet<StorageIdentityId>,
    pub(super) active_borrows: BTreeSet<BorrowCapabilityId>,
    pub(super) recovered: bool,
}

impl ExitState {
    pub(super) fn new(state: &StorageFlowState) -> Self {
        // Publication owns only cleanup decision facts while forward analysis continues independently.
        Self {
            live: state.live.clone(),
            initialized: state.initialized.clone(),
            moved: state.moved.keys().cloned().collect(),
            definitely_absent: state.definitely_absent.keys().cloned().collect(),
            fully_moved: state.fully_moved.clone(),
            active_borrows: state.active_borrows.clone(),
            recovered: state.recovered,
        }
    }

    pub(super) fn merge(&mut self, incoming: &StorageFlowState) {
        self.merge_availability(&incoming.live, &incoming.initialized, &incoming.fully_moved,
            &incoming.active_borrows, incoming.recovered);
        self.moved.extend(incoming.moved.keys().cloned());
        self.definitely_absent.retain(|access| incoming.definitely_absent.contains_key(access));
    }

    fn merge_snapshot(&mut self, incoming: Self) {
        self.merge_availability(&incoming.live, &incoming.initialized, &incoming.fully_moved,
            &incoming.active_borrows, incoming.recovered);
        self.moved.extend(incoming.moved);
        self.definitely_absent.retain(|access| incoming.definitely_absent.contains(access));
    }

    fn merge_availability(
        &mut self,
        live: &BTreeSet<StorageIdentityId>,
        initialized: &BTreeSet<StorageIdentityId>,
        fully_moved: &BTreeSet<StorageIdentityId>,
        active_borrows: &BTreeSet<BorrowCapabilityId>,
        recovered: bool,
    ) {
        crate::analysis::storage_flow::model::merge_fully_moved(
            &self.live,
            &mut self.fully_moved,
            live,
            fully_moved,
        );

        self.live.extend(live.iter().copied());
        self.initialized.retain(|identity| initialized.contains(identity));
        self.active_borrows.extend(active_borrows.iter().copied());
        self.recovered |= recovered;
    }
}

#[derive(Debug)]
pub(in crate::analysis) struct ReplacementState {
    pub(in crate::analysis) live: bool,
    pub(in crate::analysis) initialized: bool,
    pub(in crate::analysis) fully_moved: bool,
    pub(in crate::analysis) overlapping_move: bool,
    pub(in crate::analysis) moved: BTreeSet<bray_bound_tree::StoragePath>,
    pub(in crate::analysis) definitely_absent: BTreeSet<bray_bound_tree::StoragePath>,
    pub(in crate::analysis) recovered: bool,
}

impl ReplacementState {
    pub(in crate::analysis) fn new(
        state: &StorageFlowState,
        plan: StorageAccessPlan,
        storage: &StoragePlan,
        destination: Option<&bray_bound_tree::StorageTarget>,
    ) -> Self {
        use bray_bound_tree::{StorageRelationship, StorageTarget};

        let planned = storage.logical_path(plan.access());
        let reached = destination.and_then(StorageTarget::value_observation_path);

        let (Some(planned), Some(reached)) = (planned, reached.as_ref()) else {
            return Self {
                live: true,
                initialized: false,
                fully_moved: false,
                overlapping_move: false,
                moved: BTreeSet::new(),
                definitely_absent: BTreeSet::new(),
                recovered: state.recovered || planned.is_none(),
            };
        };

        let aliases_move = |path: &bray_bound_tree::StoragePath| {
            if path.root == reached.root {
                reached.relationship(path) != StorageRelationship::Disjoint
            } else {
                // A move without a resolved address can still have a planned loan path.
                storage.relationship_to_path(plan.access(), path) != StorageRelationship::Disjoint
            }
        };

        let fully_moved = state.fully_moved.contains(&reached.root) || state.definitely_absent.iter()
            .any(|(path, absence)| *absence == StorageAbsence::Moved && path.contains_storage(reached));

        Self {
            live: state.live.contains(&reached.root),
            initialized: state.initialized.contains(&reached.root)
                && !state.moved.keys().any(|path| path.root != reached.root && aliases_move(path)),
            fully_moved,
            overlapping_move: state.moved.keys().any(aliases_move),
            // Lowering's part table is indexed in the destination's planned coordinates.
            moved: state
                .moved
                .keys()
                .filter_map(|path| path.rebase(reached, planned))
                .collect(),
            definitely_absent: state
                .definitely_absent
                .keys()
                .filter(|path| reached.contains(path) || path.contains(reached))
                .cloned()
                .collect(),
            recovered: state.recovered,
        }
    }

    pub(in crate::analysis) fn state(&self) -> bray_bound_tree::StorageReplacementState {
        if !self.live || self.fully_moved {
            bray_bound_tree::StorageReplacementState::Absent
        } else if self.initialized && !self.overlapping_move {
            bray_bound_tree::StorageReplacementState::Present
        } else {
            bray_bound_tree::StorageReplacementState::Conditional
        }
    }

    pub(in crate::analysis) fn merge(&mut self, incoming: Self) {
        let owned = (self.live && !self.fully_moved) || (incoming.live && !incoming.fully_moved);

        self.live |= incoming.live;
        self.initialized &= incoming.initialized;
        self.fully_moved = self.live && !owned;
        self.overlapping_move |= incoming.overlapping_move;
        self.moved.extend(incoming.moved);

        self.definitely_absent
            .retain(|access| incoming.definitely_absent.contains(access));

        self.recovered |= incoming.recovered;
    }
}

#[cfg(test)]
mod tests {

    use bray_bound_tree::{
        BorrowCapabilityOrigin, BoundErrorExpression, BoundExpression, BoundUnitId,
        PlannedBorrowCapability, StorageAccess, StorageAccessPurpose, StorageAccessRoot,
        StorageIdentity, StoragePlanBuilder,
    };
    use bray_symbols::BorrowKind;

    use super::{ExitState, ReachedLifecycle, ReplacementState};
    use crate::analysis::storage_flow::model::StorageFlowState;
    use crate::test_support::{error_type, expression_unit, push_expression};

    #[test]
    fn reached_lifecycle_selection_keeps_only_witnessed_orders() {
        let (unit, expressions) = expression_unit(BoundUnitId::new(91), |tree, origin| {
            vec![push_expression(tree, BoundExpression::Error(BoundErrorExpression::new(origin, error_type())))]
        });

        let expression = expressions[0];
        let source = unit.tree().expression(expression).expect("test expression exists").origin().source_anchor();
        let mut builder = StoragePlanBuilder::new(unit.unit(), unit.key().kind());
        let owner = builder.push_identity(StorageIdentity::Temporary(expression)).expect("test owner builds");
        let access = StorageAccess::new(StorageAccessRoot::Storage(owner), [], error_type(), source, false);

        let [first, second, third] = std::array::from_fn(|_| builder.push_access(access.clone()).expect("test access builds"));

        let mut selection = ReachedLifecycle::Ordered([third, first].into());

        selection.record([third, second, first].into());
        selection.record([second].into());

        assert!(matches!(&selection, ReachedLifecycle::Ordered(order) if order.as_ref() == &[third, second, first]));

        selection.record([first, third].into());

        assert!(matches!(&selection, ReachedLifecycle::Multiple(orders) if orders.len() == 2));
    }

    #[test]
    fn replacement_projection_preserves_moves_through_a_distinct_logical_alias() {
        let (unit, expressions) = expression_unit(BoundUnitId::new(90), |tree, origin| {
            (0..2)
                .map(|_| {
                    push_expression(
                        tree,
                        BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
                    )
                })
                .collect()
        });

        let source = unit
            .tree()
            .expression(expressions[0])
            .expect("test expression must exist")
            .origin()
            .source_anchor();

        let mut builder = StoragePlanBuilder::new(unit.unit(), unit.key().kind());

        let owner = builder
            .push_identity(StorageIdentity::Temporary(expressions[0]))
            .expect("owner must build");

        let alias = builder
            .push_identity(StorageIdentity::Temporary(expressions[1]))
            .expect("alias must build");

        let destination = builder
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(owner),
                [],
                error_type(),
                source,
                false,
            ))
            .expect("destination must build");

        let capability = builder
            .push_borrow_capability(PlannedBorrowCapability::new(
                BorrowCapabilityOrigin::Operation((expressions[1]).into()),
                BorrowKind::Shared,
                destination,
                None,
                source,
                false,
            ))
            .expect("alias capability must build");

        let moved = builder
            .push_access(StorageAccess::new(
                StorageAccessRoot::BorrowedStorage {
                    capability,
                    storage: alias,
                },
                [],
                error_type(),
                source,
                false,
            ))
            .expect("alias access must build");

        builder
            .plan_access(
                expressions[0].into(),
                expressions[0],
                StorageAccessPurpose::Assignment,
                destination,
            )
            .expect("replacement plan must build");

        let storage = builder.finish();
        let plan = storage.access_plans()[0];

        let mut state = StorageFlowState::default();

        state.reachable = true;

        state.live.extend([owner, alias]);
        state.initialized.extend([owner, alias]);

        state.record_move(
            storage.logical_path(moved).cloned().expect("alias path"),
            expressions[1],
            crate::test_support::error_type(),
            false,
        );

        assert_ne!(
            storage.root_identity(moved),
            storage.root_identity(destination)
        );

        assert!(storage.access_contains(destination, moved));

        let target = bray_bound_tree::StorageTarget::storage(bray_bound_tree::StoragePath::root(owner));
        let projected = ReplacementState::new(&state, plan, &storage, Some(&target));

        assert!(projected.live);
        assert!(!projected.initialized);
        assert!(projected.overlapping_move);
        assert!(projected.moved.is_empty());

        let mut absent = ReplacementState::new(&StorageFlowState::default(), plan, &storage, Some(&target));

        absent.merge(projected);

        assert!(absent.live);
        assert!(!absent.initialized);
        assert!(absent.overlapping_move);

        let reached = bray_bound_tree::StorageTarget::storage(bray_bound_tree::StoragePath::root(alias));

        state.record_move(reached.storage_path().expect("alias owns its slot").clone(), expressions[1], error_type(), true);

        let replacement = ReplacementState::new(&state, plan, &storage, Some(&reached));

        assert!(state.initialized.contains(&owner));
        assert_eq!(replacement.state(), bray_bound_tree::StorageReplacementState::Absent);
    }

    #[test]
    fn exit_projection_uses_the_same_join_as_forward_availability() {
        let (unit, expressions) = expression_unit(BoundUnitId::new(91), |tree, origin| {
            (0..2)
                .map(|_| {
                    push_expression(
                        tree,
                        BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
                    )
                })
                .collect()
        });

        let mut builder = StoragePlanBuilder::new(unit.unit(), unit.key().kind());

        let identity = builder
            .push_identity(StorageIdentity::Temporary(expressions[0]))
            .expect("identity must build");

        let moved_identity = builder
            .push_identity(StorageIdentity::Temporary(expressions[1]))
            .expect("moved identity must build");

        let mut left = StorageFlowState::default();

        left.reachable = true;

        left.live.extend([identity, moved_identity]);
        left.initialized.insert(identity);
        left.fully_moved.insert(moved_identity);

        left.memory.fresh(
            crate::analysis::storage_flow::memory::Place::root(identity),
            expressions[0],
        );

        left.memory.take_retirements();

        let mut snapshot = ExitState::new(&left);

        let mut right = StorageFlowState::default();

        right.reachable = true;
        right.recovered = true;

        right.live.extend([identity, moved_identity]);
        right.initialized.insert(moved_identity);
        right.fully_moved.insert(identity);

        snapshot.merge(&right);
        left.merge(&right, None);

        assert_eq!(snapshot.live, left.live);
        assert_eq!(snapshot.initialized, left.initialized);
        assert_eq!(snapshot.fully_moved, left.fully_moved);
        assert_eq!(snapshot.recovered, left.recovered);

        let make = |status: Option<bool>| {
            let mut state = StorageFlowState::default();

            state.reachable = true;

            if let Some(moved) = status {
                state.live.insert(identity);

                if moved {
                    state.move_complete_storage(identity);
                } else {
                    state.initialized.insert(identity);
                }
            }

            state
        };

        let replacement = |status: Option<bool>| ReplacementState {
            live: status.is_some(),
            initialized: status == Some(false),
            fully_moved: status == Some(true),
            overlapping_move: false,
            moved: Default::default(),
            definitely_absent: Default::default(),
            recovered: false,
        };

        for first in [None, Some(false), Some(true)] {
            for second in [None, Some(false), Some(true)] {
                let mut forward = make(first);
                let incoming = make(second);
                let mut exit = ExitState::new(&forward);
                let mut replaced = replacement(first);

                exit.merge(&incoming);
                replaced.merge(replacement(second));
                forward.merge(&incoming, None);

                assert_eq!(
                    (&exit.live, &exit.initialized, &exit.fully_moved),
                    (&forward.live, &forward.initialized, &forward.fully_moved),
                    "{first:?}, {second:?}"
                );

                assert_eq!(
                    (replaced.live, replaced.initialized, replaced.fully_moved),
                    (
                        forward.live.contains(&identity),
                        forward.initialized.contains(&identity),
                        forward.fully_moved.contains(&identity)
                    ),
                    "{first:?}, {second:?}"
                );
            }
        }
    }
}
