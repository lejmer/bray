use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};

use crate::{
    BorrowCapabilityId, BoundBlockId, BoundExpressionId, BoundUnitId, BoundUnitKind,
    StorageAccessId, StorageAccessPurpose, StorageIdentityId,
};

/// The checker result for one evaluated storage operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageOperationStatus {
    /// The operation satisfies its storage requirements.
    Valid,
    /// Earlier recovery prevents a complete decision.
    Recovered,
    /// The reached storage is not initialized on every incoming path.
    Uninitialized,
    /// The reached storage or substorage was previously moved.
    Moved,
    /// An active overlapping borrow forbids the operation.
    ConflictingBorrow,
    /// The operation requires mutation authority that is not available.
    MissingMutationAuthority,
    /// The operation requires implicit copying for a non-copyable type.
    NotCopyable,
}

/// One source-correlated checked storage operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageOperationDecision {
    expression: BoundExpressionId,
    purpose: StorageAccessPurpose,
    access: StorageAccessId,
    borrow: Option<BorrowCapabilityId>,
    status: StorageOperationStatus,
}

impl StorageOperationDecision {
    /// Creates one checked operation decision.
    pub const fn new(
        expression: BoundExpressionId,
        purpose: StorageAccessPurpose,
        access: StorageAccessId,
        borrow: Option<BorrowCapabilityId>,
        status: StorageOperationStatus,
    ) -> Self {
        Self {
            expression,
            purpose,
            access,
            borrow,
            status,
        }
    }

    /// Returns the evaluated expression occurrence.
    pub const fn expression(self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the semantic storage operation.
    pub const fn purpose(self) -> StorageAccessPurpose {
        self.purpose
    }

    /// Returns the reached storage access.
    pub const fn access(self) -> StorageAccessId {
        self.access
    }

    /// Returns the capability established by a borrow operation.
    pub const fn borrow(self) -> Option<BorrowCapabilityId> {
        self.borrow
    }

    /// Returns the checker result.
    pub const fn status(self) -> StorageOperationStatus {
        self.status
    }
}

/// Storage and borrow state that remains relevant at one lexical scope exit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageExitDecision {
    scope: BoundBlockId,
    initialized: Arc<[StorageIdentityId]>,
    active_borrows: Arc<[BorrowCapabilityId]>,
    is_recovered: bool,
}

impl StorageExitDecision {
    /// Creates one normalized scope-exit decision.
    pub fn new(
        scope: BoundBlockId,
        initialized: impl IntoIterator<Item = StorageIdentityId>,
        active_borrows: impl IntoIterator<Item = BorrowCapabilityId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            scope,
            initialized: sorted_unique_shared_slice(initialized),
            active_borrows: sorted_unique_shared_slice(active_borrows),
            is_recovered,
        }
    }

    /// Returns the lexical scope being exited.
    pub const fn scope(&self) -> BoundBlockId {
        self.scope
    }

    /// Returns storage known to remain initialized.
    pub fn initialized(&self) -> &[StorageIdentityId] {
        &self.initialized
    }

    /// Returns borrows still active at the exit.
    pub fn active_borrows(&self) -> &[BorrowCapabilityId] {
        &self.active_borrows
    }

    /// Returns whether recovery affected the exit state.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

/// A contract violation while constructing durable storage-flow facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageFlowFactsBuildError {
    /// A decision references an identity owned by another unit.
    ForeignUnit,
}

/// Immutable storage, ownership, and borrow decisions for one bound unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageFlowFacts {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    operations: Arc<[StorageOperationDecision]>,
    exits: Arc<[StorageExitDecision]>,
    is_recovered: bool,
}

impl StorageFlowFacts {
    /// Validates and creates one durable storage-flow result.
    pub fn try_new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        operations: impl IntoIterator<Item = StorageOperationDecision>,
        exits: impl IntoIterator<Item = StorageExitDecision>,
        is_recovered: bool,
    ) -> Result<Self, StorageFlowFactsBuildError> {
        let operations = operations.into_iter().collect::<Vec<_>>();
        let exits = exits.into_iter().collect::<Vec<_>>();

        if operations.iter().any(|operation| {
            operation.expression().unit() != unit
                || operation.access().unit() != unit
                || operation
                    .borrow()
                    .is_some_and(|borrow| borrow.unit() != unit)
        }) || exits.iter().any(|exit| {
            exit.scope().unit() != unit
                || exit
                    .initialized()
                    .iter()
                    .any(|storage| storage.unit() != unit)
                || exit
                    .active_borrows()
                    .iter()
                    .any(|borrow| borrow.unit() != unit)
        }) {
            return Err(StorageFlowFactsBuildError::ForeignUnit);
        }

        Ok(Self {
            unit,
            kind,
            operations: shared_slice(operations),
            exits: shared_slice(exits),
            is_recovered,
        })
    }

    /// Returns the bound unit described by these facts.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the semantic category of the bound unit.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns checked operations in deterministic evaluation order.
    pub fn operations(&self) -> &[StorageOperationDecision] {
        &self.operations
    }

    /// Returns lexical scope-exit decisions in control-flow order.
    pub fn exits(&self) -> &[StorageExitDecision] {
        &self.exits
    }

    /// Returns whether recovery prevented complete storage decisions.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

#[cfg(test)]
mod tests {
    use super::{
        StorageExitDecision, StorageFlowFacts, StorageFlowFactsBuildError,
        StorageOperationDecision, StorageOperationStatus,
    };
    use crate::{
        BoundBlockId, BoundExpressionId, BoundUnitId, BoundUnitKind, StorageAccessId,
        StorageAccessPurpose, StorageIdentityId,
    };

    #[test]
    fn storage_flow_facts_retain_operations_and_normalized_exit_state() {
        let unit = BoundUnitId::new(3);
        let expression = BoundExpressionId::from_slot(unit, 0);
        let access = StorageAccessId::from_slot(unit, 0);
        let storage = StorageIdentityId::from_slot(unit, 0);
        let scope = BoundBlockId::from_slot(unit, 0);

        let operation = StorageOperationDecision::new(
            expression,
            StorageAccessPurpose::Read,
            access,
            None,
            StorageOperationStatus::Valid,
        );

        let exit = StorageExitDecision::new(scope, [storage, storage], [], false);

        let facts = StorageFlowFacts::try_new(
            unit,
            BoundUnitKind::CallableBody,
            [operation],
            [exit],
            false,
        )
        .unwrap_or_else(|error| panic!("unit-local storage facts must build: {error:?}"));

        assert_eq!(facts.operations(), &[operation]);
        assert_eq!(facts.exits()[0].initialized(), &[storage]);
        assert!(!facts.is_recovered());
    }

    #[test]
    fn storage_flow_facts_reject_foreign_ids() {
        let unit = BoundUnitId::new(4);
        let foreign = BoundUnitId::new(5);

        let decision = StorageOperationDecision::new(
            BoundExpressionId::from_slot(unit, 0),
            StorageAccessPurpose::Read,
            StorageAccessId::from_slot(foreign, 0),
            None,
            StorageOperationStatus::Recovered,
        );

        assert_eq!(
            StorageFlowFacts::try_new(unit, BoundUnitKind::CallableBody, [decision], [], true,),
            Err(StorageFlowFactsBuildError::ForeignUnit)
        );
    }

    #[test]
    fn storage_flow_facts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<StorageFlowFacts>();
    }
}
