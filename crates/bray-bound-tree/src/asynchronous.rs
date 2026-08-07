use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};

use crate::{
    AnyBoundNodeId, BodyBehaviorCall, BoundBlockId, BoundDependencyContractId,
    BoundDependencySubject, BoundExpressionId, BoundUnitId, BoundUnitKind, StorageAccessId,
};

/// A language-defined operation on an owned future or task.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AsyncTaskOperationKind {
    /// Transfer an inactive future into an independently scheduled task.
    Start,
    /// Transfer a task into a lazy joining computation.
    Join,
    /// Transfer a task into a lazy cancellation computation.
    Cancel,
}

/// One checked future or task operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AsyncTaskOperation {
    expression: BoundExpressionId,
    kind: AsyncTaskOperationKind,
}

impl AsyncTaskOperation {
    /// Creates one checked operation.
    pub const fn new(expression: BoundExpressionId, kind: AsyncTaskOperationKind) -> Self {
        Self { expression, kind }
    }

    /// Returns the source-correlated operation expression.
    pub const fn expression(self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the semantic operation performed by the expression.
    pub const fn kind(self) -> AsyncTaskOperationKind {
        self.kind
    }
}

/// The language operation that suspends one protected frame.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AsyncSuspensionKind {
    /// Wait for one future expression to complete.
    Await { operand: BoundExpressionId },
    /// Yield execution so another ready task can run.
    Yield,
}

/// One suspension point and the semantic state it retains.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AsyncSuspensionPoint {
    expression: BoundExpressionId,
    kind: AsyncSuspensionKind,
    dependency_contract: Option<BoundDependencyContractId>,
    deferred_calls: Arc<[BodyBehaviorCall]>,
    retained_subjects: Arc<[BoundDependencySubject]>,
    is_recovered: bool,
}

impl AsyncSuspensionPoint {
    /// Creates one normalized suspension point.
    pub fn new(
        expression: BoundExpressionId,
        kind: AsyncSuspensionKind,
        dependency_contract: Option<BoundDependencyContractId>,
        deferred_calls: impl IntoIterator<Item = BodyBehaviorCall>,
        retained_subjects: impl IntoIterator<Item = BoundDependencySubject>,
        is_recovered: bool,
    ) -> Self {
        Self {
            expression,
            kind,
            dependency_contract,
            deferred_calls: sorted_unique_shared_slice(deferred_calls),
            retained_subjects: sorted_unique_shared_slice(retained_subjects),
            is_recovered,
        }
    }

    /// Returns the expression that suspends execution.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the language operation that causes suspension.
    pub const fn kind(&self) -> AsyncSuspensionKind {
        self.kind
    }

    /// Returns the dependency contract carried by the future operand.
    pub const fn dependency_contract(&self) -> Option<BoundDependencyContractId> {
        self.dependency_contract
    }

    /// Returns deferred callable bodies that can execute at this await.
    pub fn deferred_calls(&self) -> &[BodyBehaviorCall] {
        &self.deferred_calls
    }

    /// Returns semantic subjects retained while the current run is suspended.
    pub fn retained_subjects(&self) -> &[BoundDependencySubject] {
        &self.retained_subjects
    }

    /// Returns whether recovery prevented a complete suspension decision.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

/// The ordered semantic work required when one lexical task scope exits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AsyncScopeExitPlan {
    scope: BoundBlockId,
    exit: AnyBoundNodeId,
    cancellation_broadcast: Arc<[StorageAccessId]>,
    lifecycle_resolution: Arc<[StorageAccessId]>,
    moved: Arc<[StorageAccessId]>,
    is_recovered: bool,
}

impl AsyncScopeExitPlan {
    /// Creates a two-phase scope-exit plan.
    pub fn new(
        scope: BoundBlockId,
        exit: AnyBoundNodeId,
        cancellation_broadcast: impl IntoIterator<Item = StorageAccessId>,
        lifecycle_resolution: impl IntoIterator<Item = StorageAccessId>,
        moved: impl IntoIterator<Item = StorageAccessId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            scope,
            exit,
            cancellation_broadcast: shared_slice(cancellation_broadcast),
            lifecycle_resolution: shared_slice(lifecycle_resolution),
            moved: sorted_unique_shared_slice(moved),
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

    /// Returns owned storage visited during cancellation broadcast.
    pub fn cancellation_broadcast(&self) -> &[StorageAccessId] {
        &self.cancellation_broadcast
    }

    /// Returns owned storage resolved after cancellation broadcast completes.
    pub fn lifecycle_resolution(&self) -> &[StorageAccessId] {
        &self.lifecycle_resolution
    }

    /// Returns paths already moved at this scope exit.
    pub fn moved(&self) -> &[StorageAccessId] {
        &self.moved
    }

    /// Returns whether recovery prevented a complete cleanup plan.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

/// A malformed async fact table.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AsyncFactsBuildError {
    /// One fact references semantic state owned by another bound unit.
    ForeignUnit,
}

/// Durable async frame, suspension, task, and cleanup facts for one bound unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedAsyncFacts {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    frame_dependencies: Arc<[BoundDependencySubject]>,
    suspensions: Arc<[AsyncSuspensionPoint]>,
    task_operations: Arc<[AsyncTaskOperation]>,
    scope_exits: Arc<[AsyncScopeExitPlan]>,
    is_recovered: bool,
}

impl CheckedAsyncFacts {
    /// Validates and creates one immutable async fact table.
    pub fn try_new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        frame_dependencies: impl IntoIterator<Item = BoundDependencySubject>,
        suspensions: impl IntoIterator<Item = AsyncSuspensionPoint>,
        task_operations: impl IntoIterator<Item = AsyncTaskOperation>,
        scope_exits: impl IntoIterator<Item = AsyncScopeExitPlan>,
        is_recovered: bool,
    ) -> Result<Self, AsyncFactsBuildError> {
        let frame_dependencies = sorted_unique_shared_slice(frame_dependencies);
        let suspensions = shared_slice(suspensions);
        let task_operations = shared_slice(task_operations);
        let scope_exits = shared_slice(scope_exits);

        if frame_dependencies
            .iter()
            .any(|subject| !subject.is_valid_for(unit))
            || suspensions.iter().any(|suspension| {
                suspension.expression().unit() != unit
                    || match suspension.kind() {
                        AsyncSuspensionKind::Await { operand } => operand.unit() != unit,
                        AsyncSuspensionKind::Yield => false,
                    }
                    || suspension
                        .dependency_contract()
                        .is_some_and(|contract| contract.unit() != unit)
                    || suspension
                        .retained_subjects()
                        .iter()
                        .any(|subject| !subject.is_valid_for(unit))
            })
            || task_operations
                .iter()
                .any(|operation| operation.expression().unit() != unit)
            || scope_exits.iter().any(|exit| {
                exit.scope().unit() != unit
                    || exit.exit().unit() != unit
                    || exit
                        .cancellation_broadcast()
                        .iter()
                        .chain(exit.lifecycle_resolution())
                        .chain(exit.moved())
                        .any(|access| access.unit() != unit)
            })
        {
            return Err(AsyncFactsBuildError::ForeignUnit);
        }

        Ok(Self {
            unit,
            kind,
            frame_dependencies,
            suspensions,
            task_operations,
            scope_exits,
            is_recovered,
        })
    }

    /// Returns the checked bound unit.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the checked unit category.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns every semantic subject retained by the async frame.
    pub fn frame_dependencies(&self) -> &[BoundDependencySubject] {
        &self.frame_dependencies
    }

    /// Returns suspension points in evaluation order.
    pub fn suspensions(&self) -> &[AsyncSuspensionPoint] {
        &self.suspensions
    }

    /// Returns task operations in evaluation order.
    pub fn task_operations(&self) -> &[AsyncTaskOperation] {
        &self.task_operations
    }

    /// Returns two-phase cleanup plans in control-flow order.
    pub fn scope_exits(&self) -> &[AsyncScopeExitPlan] {
        &self.scope_exits
    }

    /// Returns whether recovery prevented complete async checking.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AsyncScopeExitPlan, AsyncSuspensionKind, AsyncSuspensionPoint, AsyncTaskOperation,
        AsyncTaskOperationKind, CheckedAsyncFacts,
    };
    use crate::{
        BoundBlockId, BoundDependencySubject, BoundExpressionId, BoundUnitId, BoundUnitKind,
        StorageAccessId,
    };

    #[test]
    fn async_facts_normalize_frame_dependencies_and_preserve_operation_order() {
        let unit = BoundUnitId::new(7);
        let await_expression = BoundExpressionId::from_slot(unit, 2);
        let operand = BoundExpressionId::from_slot(unit, 1);
        let storage = StorageAccessId::from_slot(unit, 3);
        let dependency = BoundDependencySubject::StorageAccess(storage);

        let suspension = AsyncSuspensionPoint::new(
            await_expression,
            AsyncSuspensionKind::Await { operand },
            None,
            [],
            [dependency],
            false,
        );

        let operation = AsyncTaskOperation::new(operand, AsyncTaskOperationKind::Start);

        let cleanup = AsyncScopeExitPlan::new(
            BoundBlockId::from_slot(unit, 4),
            BoundBlockId::from_slot(unit, 4).into(),
            [storage],
            [storage],
            [storage],
            false,
        );

        let facts = CheckedAsyncFacts::try_new(
            unit,
            BoundUnitKind::CallableBody,
            [dependency, dependency],
            [suspension],
            [operation],
            [cleanup],
            false,
        )
        .unwrap_or_else(|error| panic!("unit-local async facts must build: {error:?}"));

        assert_eq!(facts.frame_dependencies(), &[dependency]);
        assert_eq!(facts.task_operations(), &[operation]);
        assert_eq!(facts.suspensions().len(), 1);
        assert_eq!(facts.scope_exits().len(), 1);
        assert_eq!(facts.scope_exits()[0].moved(), &[storage]);
    }

    #[test]
    fn async_facts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CheckedAsyncFacts>();
    }
}
