use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;

use crate::{
    AsyncStorageCleanupRequirement, BoundExpressionId, StorageAccessId, StorageCleanupPart,
};

/// Old destination availability after evaluating the replacement value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StorageReplacementState {
    /// No old initialized contents remain on any incoming path.
    Absent,
    /// The old destination is completely initialized on every incoming path.
    Present,
    /// Runtime initialization guards select the remaining initialized contents.
    Conditional,
}

/// Complete cleanup selection for one replacement destination.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StorageReplacementPlan {
    expression: BoundExpressionId,
    access: StorageAccessId,
    cleanup: AsyncStorageCleanupRequirement,
    parts: Option<Arc<[StorageCleanupPart]>>,
}

impl StorageReplacementPlan {
    /// Selects whole or represented-part cleanup before replacement installation.
    pub fn new(
        expression: BoundExpressionId,
        access: StorageAccessId,
        cleanup: AsyncStorageCleanupRequirement,
        parts: Option<Vec<StorageCleanupPart>>,
    ) -> Self {
        Self {
            expression,
            access,
            cleanup,
            parts: parts.map(Into::into),
        }
    }

    /// Returns the assignment whose old contents are resolved.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the destination established before evaluating the new value.
    pub const fn access(&self) -> StorageAccessId {
        self.access
    }

    /// Returns the required old-value cleanup phases or exact recovery cause.
    pub const fn cleanup(&self) -> AsyncStorageCleanupRequirement {
        self.cleanup
    }

    /// Returns the complete partial-value partition, or no partition for whole cleanup.
    pub fn parts(&self) -> Option<&[StorageCleanupPart]> {
        self.parts.as_deref()
    }
}

/// Checked destination state before old cleanup and replacement installation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StorageReplacementDecision {
    expression: BoundExpressionId,
    access: StorageAccessId,
    state: StorageReplacementState,
    moved: Arc<[StorageAccessId]>,
    is_recovered: bool,
}

impl StorageReplacementDecision {
    /// Records the old state after RHS transfers and before installing its value.
    pub fn new(
        expression: BoundExpressionId,
        access: StorageAccessId,
        state: StorageReplacementState,
        moved: impl IntoIterator<Item = StorageAccessId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            expression,
            access,
            state,
            moved: sorted_unique_shared_slice(moved),
            is_recovered,
        }
    }

    /// Returns the source assignment.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the established destination access.
    pub const fn access(&self) -> StorageAccessId {
        self.access
    }

    /// Returns the old destination's initialization state.
    pub const fn state(&self) -> StorageReplacementState {
        self.state
    }

    /// Returns moved accesses within the destination's storage root.
    pub fn moved(&self) -> &[StorageAccessId] {
        &self.moved
    }

    /// Returns whether recovery prevented a complete decision.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

#[cfg(test)]
mod tests {
    use super::{StorageReplacementDecision, StorageReplacementState};
    use crate::{
        BoundExpressionId, BoundUnitId, BoundUnitKind, StorageAccessId, StorageFlow,
        StorageFlowBuildError,
    };

    #[test]
    fn replacement_state_retains_normalized_moves_after_rhs_evaluation() {
        let unit = BoundUnitId::new(7);
        let expression = BoundExpressionId::from_slot(unit, 3);
        let destination = StorageAccessId::from_slot(unit, 1);
        let moved = StorageAccessId::from_slot(unit, 2);

        let decision = StorageReplacementDecision::new(
            expression,
            destination,
            StorageReplacementState::Conditional,
            [moved, moved],
            false,
        );

        assert_eq!(
            decision,
            StorageReplacementDecision::new(
                expression,
                destination,
                StorageReplacementState::Conditional,
                [moved],
                false,
            )
        );

        let flow = StorageFlow::try_new(unit, BoundUnitKind::CallableBody, [], [], [], [], false)
            .unwrap()
            .with_replacements([decision.clone()])
            .unwrap();

        assert_eq!(flow.replacements(), &[decision]);
    }

    #[test]
    fn replacement_table_rejects_duplicate_and_foreign_decisions() {
        let unit = BoundUnitId::new(7);
        let expression = BoundExpressionId::from_slot(unit, 3);
        let access = StorageAccessId::from_slot(unit, 1);

        let decision = StorageReplacementDecision::new(
            expression,
            access,
            StorageReplacementState::Present,
            [],
            false,
        );

        let flow = || {
            StorageFlow::try_new(unit, BoundUnitKind::CallableBody, [], [], [], [], false).unwrap()
        };

        assert_eq!(
            flow().with_replacements([decision.clone(), decision]),
            Err(StorageFlowBuildError::DuplicateReplacement)
        );

        for (expression, access, moved) in [
            (
                BoundExpressionId::from_slot(BoundUnitId::new(8), 3),
                access,
                Vec::new(),
            ),
            (
                expression,
                StorageAccessId::from_slot(BoundUnitId::new(8), 1),
                Vec::new(),
            ),
            (
                expression,
                access,
                vec![StorageAccessId::from_slot(BoundUnitId::new(8), 2)],
            ),
        ] {
            let decision = StorageReplacementDecision::new(
                expression,
                access,
                StorageReplacementState::Conditional,
                moved,
                false,
            );

            assert_eq!(
                flow().with_replacements([decision]),
                Err(StorageFlowBuildError::ForeignUnit)
            );
        }
    }
}
