use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};

use crate::{
    AnyBoundNodeId, BorrowCapabilityId, BoundBlockId, BoundExpressionId, BoundUnitId,
    BoundUnitKind, CheckedMemoryOperation, CheckedMemoryOperations, MemoryOperationDecision,
    StorageAccessId, StorageAccessPurpose, StorageIdentityId, StoragePlan, StorageProjection,
    storage_identity_transfers_at_unit_exit,
};

/// The checker result for one evaluated storage operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageOperationStatus {
    /// Control flow cannot reach the operation.
    Unreachable,
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
    /// The operation requires ownership of storage reached only through a borrow.
    MissingOwnership,
    /// The selected nullable or union substorage is inactive on this control-flow path.
    InactiveProjection,
    /// The operation requires implicit copying for a non-copyable type.
    NotCopyable,
}

/// One source-correlated checked storage operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageOperationDecision {
    node: AnyBoundNodeId,
    expression: BoundExpressionId,
    purpose: StorageAccessPurpose,
    access: StorageAccessId,
    borrow: Option<BorrowCapabilityId>,
    status: StorageOperationStatus,
}

impl StorageOperationDecision {
    /// Creates one checked operation decision.
    pub const fn new(
        node: AnyBoundNodeId,
        expression: BoundExpressionId,
        purpose: StorageAccessPurpose,
        access: StorageAccessId,
        borrow: Option<BorrowCapabilityId>,
        status: StorageOperationStatus,
    ) -> Self {
        Self {
            node,
            expression,
            purpose,
            access,
            borrow,
            status,
        }
    }

    /// Returns the control-flow node where this operation takes effect.
    pub const fn node(self) -> AnyBoundNodeId {
        self.node
    }

    /// Returns the source expression supplying the accessed value.
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

/// Storage and borrow state immediately before one direct-await suspension.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageSuspensionState {
    expression: BoundExpressionId,
    live: Arc<[StorageIdentityId]>,
    initialized: Arc<[StorageIdentityId]>,
    moved: Arc<[StorageAccessId]>,
    active_borrows: Arc<[BorrowCapabilityId]>,
}

impl StorageSuspensionState {
    /// Creates one normalized suspension-state snapshot.
    pub fn new(
        expression: BoundExpressionId,
        live: impl IntoIterator<Item = StorageIdentityId>,
        initialized: impl IntoIterator<Item = StorageIdentityId>,
        moved: impl IntoIterator<Item = StorageAccessId>,
        active_borrows: impl IntoIterator<Item = BorrowCapabilityId>,
    ) -> Self {
        Self {
            expression,
            live: sorted_unique_shared_slice(live),
            initialized: sorted_unique_shared_slice(initialized),
            moved: sorted_unique_shared_slice(moved),
            active_borrows: sorted_unique_shared_slice(active_borrows),
        }
    }

    /// Returns the direct-await expression.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns storage known to be live before suspension.
    pub fn live(&self) -> &[StorageIdentityId] {
        &self.live
    }

    /// Returns storage known to be initialized before suspension.
    pub fn initialized(&self) -> &[StorageIdentityId] {
        &self.initialized
    }

    /// Returns storage accesses moved before suspension.
    pub fn moved(&self) -> &[StorageAccessId] {
        &self.moved
    }

    /// Returns borrow capabilities active before suspension.
    pub fn active_borrows(&self) -> &[BorrowCapabilityId] {
        &self.active_borrows
    }
}

/// One lexical scope exit proven reachable by checked control flow.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageExitPoint {
    scope: BoundBlockId,
    exit: AnyBoundNodeId,
}

impl StorageExitPoint {
    /// Creates one lowering-reachable scope exit.
    pub const fn new(scope: BoundBlockId, exit: AnyBoundNodeId) -> Self {
        Self { scope, exit }
    }

    /// Returns the lexical scope being exited.
    pub const fn scope(self) -> BoundBlockId {
        self.scope
    }

    /// Returns the bound node that exits the scope.
    pub const fn exit(self) -> AnyBoundNodeId {
        self.exit
    }
}

/// Storage and borrow state that remains relevant at one lexical scope exit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageExitDecision {
    scope: BoundBlockId,
    exit: AnyBoundNodeId,
    live: Arc<[StorageIdentityId]>,
    initialized: Arc<[StorageIdentityId]>,
    moved: Arc<[StorageAccessId]>,
    fully_moved: Arc<[StorageIdentityId]>,
    active_borrows: Arc<[BorrowCapabilityId]>,
    is_recovered: bool,
}

impl StorageExitDecision {
    /// Creates one normalized scope-exit decision.
    pub fn new(
        scope: BoundBlockId,
        exit: AnyBoundNodeId,
        live: impl IntoIterator<Item = StorageIdentityId>,
        initialized: impl IntoIterator<Item = StorageIdentityId>,
        moved: impl IntoIterator<Item = StorageAccessId>,
        fully_moved: impl IntoIterator<Item = StorageIdentityId>,
        active_borrows: impl IntoIterator<Item = BorrowCapabilityId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            scope,
            exit,
            live: sorted_unique_shared_slice(live),
            initialized: sorted_unique_shared_slice(initialized),
            moved: sorted_unique_shared_slice(moved),
            fully_moved: sorted_unique_shared_slice(fully_moved),
            active_borrows: sorted_unique_shared_slice(active_borrows),
            is_recovered,
        }
    }

    /// Returns the lexical scope being exited.
    pub const fn scope(&self) -> BoundBlockId {
        self.scope
    }

    /// Returns the bound node whose completion or transfer exits the scope.
    pub const fn exit(&self) -> AnyBoundNodeId {
        self.exit
    }

    /// Returns storage live on at least one path reaching this exit.
    pub fn live(&self) -> &[StorageIdentityId] {
        &self.live
    }

    /// Returns storage known to remain initialized.
    pub fn initialized(&self) -> &[StorageIdentityId] {
        &self.initialized
    }

    /// Returns storage accesses whose reached values were moved before this exit.
    pub fn moved(&self) -> &[StorageAccessId] {
        &self.moved
    }

    /// Returns storage identities known to be fully moved on every incoming path.
    pub fn fully_moved(&self) -> &[StorageIdentityId] {
        &self.fully_moved
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

/// A contract violation while constructing durable storage-flow analysis.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StorageFlowBuildError {
    /// A decision references an identity owned by another unit.
    ForeignUnit,
    /// The same direct-await expression has more than one state snapshot.
    DuplicateSuspension,
    /// The same expression has more than one memory-flow decision.
    DuplicateMemoryOperation,
    /// The same assignment has more than one old-storage decision.
    DuplicateReplacement,
}

/// Immutable storage, ownership, and borrow decisions for one bound unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StorageFlow {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    operations: Arc<[StorageOperationDecision]>,
    suspensions: Arc<[StorageSuspensionState]>,
    reachable_exits: Arc<[StorageExitPoint]>,
    exits: Arc<[StorageExitDecision]>,
    memory_operations: Arc<[MemoryOperationDecision]>,
    checked_memory_operations: Arc<[CheckedMemoryOperation]>,
    replacements: Arc<[crate::StorageReplacementDecision]>,
    is_recovered: bool,
}

impl StorageFlow {
    /// Validates and creates one durable storage-flow result.
    pub fn try_new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        operations: impl IntoIterator<Item = StorageOperationDecision>,
        suspensions: impl IntoIterator<Item = StorageSuspensionState>,
        reachable_exits: impl IntoIterator<Item = StorageExitPoint>,
        exits: impl IntoIterator<Item = StorageExitDecision>,
        is_recovered: bool,
    ) -> Result<Self, StorageFlowBuildError> {
        let operations = operations.into_iter().collect::<Vec<_>>();
        let mut suspensions = suspensions.into_iter().collect::<Vec<_>>();
        let reachable_exits = sorted_unique_shared_slice(reachable_exits);
        let exits = exits.into_iter().collect::<Vec<_>>();

        if operations.iter().any(|operation| {
            operation.node().unit() != unit
                || operation.expression().unit() != unit
                || operation.access().unit() != unit
                || operation
                    .borrow()
                    .is_some_and(|borrow| borrow.unit() != unit)
        }) || suspensions.iter().any(|suspension| {
            suspension.expression().unit() != unit
                || suspension
                    .live()
                    .iter()
                    .any(|storage| storage.unit() != unit)
                || suspension
                    .initialized()
                    .iter()
                    .any(|storage| storage.unit() != unit)
                || suspension
                    .moved()
                    .iter()
                    .any(|access| access.unit() != unit)
                || suspension
                    .active_borrows()
                    .iter()
                    .any(|borrow| borrow.unit() != unit)
        }) || reachable_exits
            .iter()
            .any(|exit| exit.scope().unit() != unit || exit.exit().unit() != unit)
            || exits.iter().any(|exit| {
                exit.scope().unit() != unit
                    || exit.exit().unit() != unit
                    || exit.live().iter().any(|storage| storage.unit() != unit)
                    || exit
                        .initialized()
                        .iter()
                        .any(|storage| storage.unit() != unit)
                    || exit.moved().iter().any(|access| access.unit() != unit)
                    || exit
                        .fully_moved()
                        .iter()
                        .any(|storage| storage.unit() != unit)
                    || exit
                        .active_borrows()
                        .iter()
                        .any(|borrow| borrow.unit() != unit)
            })
        {
            return Err(StorageFlowBuildError::ForeignUnit);
        }

        suspensions.sort_unstable_by_key(StorageSuspensionState::expression);

        if suspensions
            .windows(2)
            .any(|pair| pair[0].expression() == pair[1].expression())
        {
            return Err(StorageFlowBuildError::DuplicateSuspension);
        }

        Ok(Self {
            unit,
            kind,
            operations: shared_slice(operations),
            suspensions: shared_slice(suspensions),
            reachable_exits,
            exits: shared_slice(exits),
            memory_operations: Arc::from([]),
            checked_memory_operations: Arc::from([]),
            replacements: Arc::from([]),
            is_recovered,
        })
    }

    /// Adds source-ordered replacement decisions, rejecting foreign and duplicate identities.
    pub fn with_replacements(
        mut self,
        replacements: impl IntoIterator<Item = crate::StorageReplacementDecision>,
    ) -> Result<Self, StorageFlowBuildError> {
        let mut replacements = replacements.into_iter().collect::<Vec<_>>();

        if replacements.iter().any(|decision| {
            decision.expression().unit() != self.unit
                || decision.access().unit() != self.unit
                || decision
                    .moved()
                    .iter()
                    .any(|access| access.unit() != self.unit)
        }) {
            return Err(StorageFlowBuildError::ForeignUnit);
        }

        replacements.sort_unstable_by_key(crate::StorageReplacementDecision::expression);

        if replacements
            .windows(2)
            .any(|pair| pair[0].expression() == pair[1].expression())
        {
            return Err(StorageFlowBuildError::DuplicateReplacement);
        }

        self.replacements = shared_slice(replacements);

        Ok(self)
    }

    /// Returns old-storage decisions after replacement operands have been evaluated.
    pub fn replacements(&self) -> &[crate::StorageReplacementDecision] {
        &self.replacements
    }

    /// Adds validated memory-flow decisions without mutating the published analysis.
    pub fn with_memory_operations(
        mut self,
        checked: &CheckedMemoryOperations,
        decisions: impl IntoIterator<Item = MemoryOperationDecision>,
    ) -> Result<Self, StorageFlowBuildError> {
        if checked.unit() != self.unit || checked.kind() != self.kind {
            return Err(StorageFlowBuildError::ForeignUnit);
        }

        let mut operations = decisions.into_iter().collect::<Vec<_>>();

        if operations
            .iter()
            .any(|operation| operation.expression().unit() != self.unit)
        {
            return Err(StorageFlowBuildError::ForeignUnit);
        }

        operations.sort_unstable_by_key(|operation| operation.expression());

        if operations
            .windows(2)
            .any(|pair| pair[0].expression() == pair[1].expression())
        {
            return Err(StorageFlowBuildError::DuplicateMemoryOperation);
        }

        self.memory_operations = shared_slice(operations);
        self.checked_memory_operations = shared_slice(checked.operations().iter().cloned());

        Ok(self)
    }

    /// Returns the bound unit described by this analysis.
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

    /// Returns direct-await storage states in expression identity order.
    pub fn suspensions(&self) -> &[StorageSuspensionState] {
        &self.suspensions
    }

    /// Returns the exact scope exits proven reachable by checked control flow.
    pub fn reachable_exits(&self) -> &[StorageExitPoint] {
        &self.reachable_exits
    }

    /// Returns storage state immediately before one direct-await expression.
    pub fn suspension(&self, expression: BoundExpressionId) -> Option<&StorageSuspensionState> {
        self.suspensions
            .binary_search_by_key(&expression, StorageSuspensionState::expression)
            .ok()
            .map(|index| &self.suspensions[index])
    }

    /// Returns lexical scope-exit decisions in control-flow order.
    pub fn exits(&self) -> &[StorageExitDecision] {
        &self.exits
    }

    /// Returns moved represented-part paths needed by this owner's cleanup or replacement.
    ///
    /// Retained inner-scope exits and transferred-out storage do not contribute exit paths.
    pub fn cleanup_moved_projections<'a>(
        &'a self,
        storage: &'a StoragePlan,
        identity: StorageIdentityId,
        owner: Option<BoundBlockId>,
    ) -> impl Iterator<Item = &'a [StorageProjection]> {
        let transfers = storage_identity_transfers_at_unit_exit(storage, identity);

        self.exits
            .iter()
            .filter(move |exit| owner == Some(exit.scope()) && !transfers)
            .flat_map(|exit| exit.moved())
            .chain(
                self.replacements
                    .iter()
                    .flat_map(|replacement| replacement.moved()),
            )
            .filter(move |access| {
                storage.root_identity(**access) == Some(identity)
                    && !storage.is_root_access(**access)
            })
            .filter_map(move |access| storage.resolved_projections(*access))
    }

    /// Returns memory-operation decisions in bound-expression order.
    pub fn memory_operations(&self) -> &[MemoryOperationDecision] {
        &self.memory_operations
    }

    /// Returns the checked memory operation for one bound expression.
    pub fn checked_memory_operation(
        &self,
        expression: BoundExpressionId,
    ) -> Option<&CheckedMemoryOperation> {
        self.checked_memory_operations
            .binary_search_by_key(&expression, CheckedMemoryOperation::expression)
            .ok()
            .map(|index| &self.checked_memory_operations[index])
    }

    /// Returns whether recovery prevented complete storage decisions.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

#[cfg(test)]
mod tests {
    use super::{
        StorageExitDecision, StorageExitPoint, StorageFlow, StorageFlowBuildError,
        StorageOperationDecision, StorageOperationStatus, StorageSuspensionState,
    };
    use crate::{
        BoundBlockId, BoundExpressionId, BoundUnitId, BoundUnitKind, StorageAccessId,
        StorageAccessPurpose, StorageIdentityId,
    };

    #[test]
    fn storage_flow_retains_operations_and_normalized_exit_state() {
        let unit = BoundUnitId::new(3);
        let expression = BoundExpressionId::from_slot(unit, 0);
        let access = StorageAccessId::from_slot(unit, 0);
        let storage = StorageIdentityId::from_slot(unit, 0);
        let scope = BoundBlockId::from_slot(unit, 0);

        let operation = StorageOperationDecision::new(
            expression.into(),
            expression,
            StorageAccessPurpose::Read,
            access,
            None,
            StorageOperationStatus::Valid,
        );

        let exit = StorageExitDecision::new(
            scope,
            scope.into(),
            [storage, storage],
            [storage, storage],
            [access, access],
            [storage, storage],
            [],
            false,
        );

        let suspension =
            StorageSuspensionState::new(expression, [storage], [storage], [access], []);

        let storage_flow = StorageFlow::try_new(
            unit,
            BoundUnitKind::CallableBody,
            [operation],
            [suspension],
            [StorageExitPoint::new(scope, scope.into())],
            [exit],
            false,
        )
        .unwrap_or_else(|error| panic!("unit-local storage-flow analysis must build: {error:?}"));

        assert_eq!(storage_flow.operations(), &[operation]);

        assert_eq!(
            storage_flow
                .suspension(expression)
                .map(StorageSuspensionState::initialized),
            Some(&[storage][..])
        );

        assert_eq!(storage_flow.exits()[0].initialized(), &[storage]);
        assert_eq!(storage_flow.exits()[0].moved(), &[access]);
        assert_eq!(storage_flow.exits()[0].fully_moved(), &[storage]);
        assert!(!storage_flow.is_recovered());
    }

    #[test]
    fn storage_flow_rejects_foreign_ids() {
        let unit = BoundUnitId::new(4);
        let foreign = BoundUnitId::new(5);

        let decision = StorageOperationDecision::new(
            BoundExpressionId::from_slot(unit, 0).into(),
            BoundExpressionId::from_slot(unit, 0),
            StorageAccessPurpose::Read,
            StorageAccessId::from_slot(foreign, 0),
            None,
            StorageOperationStatus::Recovered,
        );

        assert_eq!(
            StorageFlow::try_new(
                unit,
                BoundUnitKind::CallableBody,
                [decision],
                [],
                [],
                [],
                true,
            ),
            Err(StorageFlowBuildError::ForeignUnit)
        );
    }

    #[test]
    fn storage_flow_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<StorageFlow>();
    }
}
