use std::collections::BTreeSet;

use bray_bound_tree::{
    BorrowCapabilityId, StorageAccessId, StorageAccessPlan, StorageIdentityId, StoragePlan,
};

use crate::analysis::storage_flow::model::StorageFlowState;

#[derive(Debug)]
pub(super) struct ExitState {
    pub(super) live: BTreeSet<StorageIdentityId>,
    pub(super) initialized: BTreeSet<StorageIdentityId>,
    pub(super) moved: BTreeSet<StorageAccessId>,
    pub(super) definitely_moved: BTreeSet<StorageAccessId>,
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
            moved: state.moved.keys().copied().collect(),
            definitely_moved: state.definitely_moved.clone(),
            fully_moved: state.fully_moved.clone(),
            active_borrows: state.active_borrows.clone(),
            recovered: state.recovered,
        }
    }

    pub(super) fn merge(&mut self, incoming: &StorageFlowState) {
        self.live.extend(incoming.live.iter().copied());

        self.initialized
            .retain(|identity| incoming.initialized.contains(identity));

        self.moved.extend(incoming.moved.keys().copied());

        self.definitely_moved
            .retain(|access| incoming.definitely_moved.contains(access));

        self.fully_moved
            .retain(|identity| incoming.fully_moved.contains(identity));

        self.active_borrows
            .extend(incoming.active_borrows.iter().copied());

        self.recovered |= incoming.recovered;
    }
}

#[derive(Debug)]
pub(super) struct ReplacementState {
    pub(super) live: bool,
    pub(super) initialized: bool,
    pub(super) fully_moved: bool,
    pub(super) overlapping_move: bool,
    pub(super) moved: BTreeSet<StorageAccessId>,
    pub(super) recovered: bool,
}

impl ReplacementState {
    pub(super) fn new(
        state: &StorageFlowState,
        plan: StorageAccessPlan,
        storage: &StoragePlan,
    ) -> Self {
        let root = storage.root_identity(plan.access());

        Self {
            live: root.is_some_and(|root| state.live.contains(&root)),
            initialized: root.is_some_and(|root| state.initialized.contains(&root)),
            fully_moved: root.is_some_and(|root| state.fully_moved.contains(&root)),
            overlapping_move: state.moved.keys().any(|access| {
                storage.access_contains(plan.access(), *access)
                    || storage.access_contains(*access, plan.access())
            }),
            moved: state
                .moved
                .keys()
                .copied()
                .filter(|access| storage.root_identity(*access) == root)
                .collect(),
            recovered: state.recovered || root.is_none(),
        }
    }

    pub(super) fn merge(&mut self, incoming: Self) {
        self.live |= incoming.live;
        self.initialized &= incoming.initialized;
        self.fully_moved &= incoming.fully_moved;
        self.overlapping_move |= incoming.overlapping_move;
        self.moved.extend(incoming.moved);
        self.recovered |= incoming.recovered;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

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
                BorrowCapabilityOrigin::Expression(expressions[1]),
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

        let mut state = StorageFlowState {
            reachable: true,
            ..StorageFlowState::default()
        };

        state.live.extend([owner, alias]);
        state.initialized.extend([owner, alias]);
        state.moved.insert(moved, expressions[1]);

        assert_ne!(
            storage.root_identity(moved),
            storage.root_identity(destination)
        );

        assert!(storage.access_contains(destination, moved));

        let projected = ReplacementState::new(&state, plan, &storage);

        assert!(projected.live && projected.initialized);
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

        let mut left = StorageFlowState {
            reachable: true,
            ..StorageFlowState::default()
        };

        left.live.extend([identity, moved_identity]);
        left.initialized.insert(identity);
        left.fully_moved.insert(moved_identity);

        left.allocation_origins
            .insert(identity, BTreeSet::from([expressions[0]]));

        let mut snapshot = ExitState::new(&left);

        let mut right = StorageFlowState {
            reachable: true,
            recovered: true,
            ..StorageFlowState::default()
        };

        right.live.extend([identity, moved_identity]);
        right.initialized.insert(moved_identity);
        right.fully_moved.insert(identity);

        snapshot.merge(&right);
        left.merge(&right);

        assert_eq!(snapshot.live, left.live);
        assert_eq!(snapshot.initialized, left.initialized);
        assert_eq!(snapshot.fully_moved, left.fully_moved);
        assert_eq!(snapshot.recovered, left.recovered);
    }
}
