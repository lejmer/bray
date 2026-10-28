use std::collections::BTreeSet;

use bray_bound_tree::{BorrowCapabilityId, StorageAccessPlan, StorageIdentityId, StoragePlan};

use crate::analysis::storage_flow::model::StorageFlowState;

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
        crate::analysis::storage_flow::model::merge_fully_moved(
            &self.live,
            &mut self.fully_moved,
            &incoming.live,
            &incoming.fully_moved,
        );

        self.live.extend(incoming.live.iter().copied());

        self.initialized
            .retain(|identity| incoming.initialized.contains(identity));

        self.moved.extend(incoming.moved.keys().cloned());

        self.definitely_absent
            .retain(|access| incoming.definitely_absent.contains_key(access));

        self.active_borrows
            .extend(incoming.active_borrows.iter().copied());

        self.recovered |= incoming.recovered;
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
    ) -> Self {
        let root = storage.root_identity(plan.access());

        Self {
            live: root.is_some_and(|root| state.live.contains(&root)),
            // A move through another logical alias has no path in this root's partition.
            initialized: root.is_some_and(|root| state.initialized.contains(&root))
                && !state.moved.keys().any(|access| {
                    Some(access.root) != root
                        && storage.relationship_to_path(plan.access(), access)
                            != bray_bound_tree::StorageRelationship::Disjoint
                }),
            fully_moved: root.is_some_and(|root| state.fully_moved.contains(&root)),
            overlapping_move: state.moved.keys().any(|access| {
                storage.relationship_to_path(plan.access(), access)
                    != bray_bound_tree::StorageRelationship::Disjoint
            }),
            moved: state
                .moved
                .keys()
                .filter(|&access| Some(access.root) == root)
                .cloned()
                .collect(),
            definitely_absent: state
                .definitely_absent
                .keys()
                .filter(|&access| Some(access.root) == root)
                .cloned()
                .collect(),
            recovered: state.recovered || root.is_none(),
        }
    }

    pub(in crate::analysis) fn state(
        &self,
        has_root: bool,
    ) -> bray_bound_tree::StorageReplacementState {
        if !has_root {
            bray_bound_tree::StorageReplacementState::Conditional
        } else if !self.live || self.fully_moved {
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

    use super::{ExitState, ReplacementState};
    use crate::analysis::storage_flow::model::StorageFlowState;
    use crate::test_support::{error_type, expression_unit, push_expression};

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

        let projected = ReplacementState::new(&state, plan, &storage);

        assert!(projected.live);
        assert!(!projected.initialized);
        assert!(projected.overlapping_move);
        assert!(projected.moved.is_empty());

        let mut absent = ReplacementState::new(&StorageFlowState::default(), plan, &storage);

        absent.merge(projected);

        assert!(absent.live);
        assert!(!absent.initialized);
        assert!(absent.overlapping_move);
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
