use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};

use crate::{
    BodyBehaviorCall, BoundDependencySubject,
    BoundExpressionId, BoundUnitId, BoundUnitKind, StorageAccessId, StorageIdentityId,
    StorageScope,
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
    /// Drive the selected asynchronous scoped lifecycle invocation.
    ScopedCall,
    /// Resolve pending owners under the enclosing frame's cleanup shield.
    Cleanup,
}

/// One suspension point and the semantic state it retains.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct AsyncSuspensionPoint {
    occurrence: crate::BoundExecutionSite,
    kind: AsyncSuspensionKind,
    deferred_calls: Arc<[BodyBehaviorCall]>,
    retained_subjects: Arc<[BoundDependencySubject]>,
    cancellation_broadcast: Arc<[StorageAccessId]>,
    is_recovered: bool,
}

impl AsyncSuspensionPoint {
    /// Combines retained subjects from control-flow paths reaching the same selected suspension.
    pub fn merge_path(&mut self, other: &Self) {
        assert_eq!(
            self.occurrence, other.occurrence,
            "merged suspension paths retain the same occurrence"
        );

        assert_eq!(
            self.kind, other.kind,
            "merged suspension paths retain the same execution kind"
        );

        *self = Self::new(
            self.occurrence,
            self.kind,
            self.deferred_calls
                .iter()
                .chain(other.deferred_calls.iter())
                .cloned(),
            self.retained_subjects
                .iter()
                .chain(other.retained_subjects.iter())
                .copied(),
            self.cancellation_broadcast
                .iter()
                .chain(other.cancellation_broadcast.iter())
                .copied(),
            self.is_recovered || other.is_recovered,
        );
    }

    /// Creates one normalized suspension point.
    pub fn new(
        occurrence: impl Into<crate::BoundExecutionSite>,
        kind: AsyncSuspensionKind,
        deferred_calls: impl IntoIterator<Item = BodyBehaviorCall>,
        retained_subjects: impl IntoIterator<Item = BoundDependencySubject>,
        cancellation_broadcast: impl IntoIterator<Item = StorageAccessId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            occurrence: occurrence.into(),
            kind,
            deferred_calls: sorted_unique_shared_slice(deferred_calls),
            retained_subjects: sorted_unique_shared_slice(retained_subjects),
            cancellation_broadcast: sorted_unique_shared_slice(cancellation_broadcast),
            is_recovered,
        }
    }

    /// Returns the exact source or implicit lifecycle phase that suspends.
    pub const fn occurrence(&self) -> crate::BoundExecutionSite {
        self.occurrence
    }

    /// Returns the language operation that causes suspension.
    pub const fn kind(&self) -> AsyncSuspensionKind {
        self.kind
    }

    /// Returns deferred callable bodies that can execute at this await.
    pub fn deferred_calls(&self) -> &[BodyBehaviorCall] {
        &self.deferred_calls
    }

    /// Returns semantic subjects retained while the current run is suspended.
    pub fn retained_subjects(&self) -> &[BoundDependencySubject] {
        &self.retained_subjects
    }

    /// Returns owned storage whose initialized parts require cancellation broadcast.
    pub fn cancellation_broadcast(&self) -> &[StorageAccessId] {
        &self.cancellation_broadcast
    }

    /// Returns whether recovery prevented a complete suspension decision.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

/// The ordered cleanup phases selected for one initialized storage identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AsyncCleanupPhases {
    /// Broadcast cancellation without a later lifecycle operation.
    Cancellation,
    /// Resolve lifecycle state without a preceding cancellation broadcast.
    Lifecycle,
    /// Broadcast cancellation before resolving lifecycle state.
    CancellationThenLifecycle,
}

impl AsyncCleanupPhases {
    /// Returns whether cleanup starts with task cancellation broadcast.
    pub const fn includes_cancellation(self) -> bool {
        matches!(self, Self::Cancellation | Self::CancellationThenLifecycle)
    }

    /// Returns whether cleanup resolves lifecycle state.
    pub const fn includes_lifecycle(self) -> bool {
        matches!(self, Self::Lifecycle | Self::CancellationThenLifecycle)
    }
}

/// Cleanup selected for one storage identity at a scope exit.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AsyncStorageExitDisposition {
    /// The identity requires no cancellation or lifecycle operation.
    NoCleanup,
    /// The identity requires ordered cleanup through its root access.
    Cleanup {
        /// The root access used by both cleanup phases.
        access: StorageAccessId,
        /// The ordered phases required by the checked type representation.
        phases: AsyncCleanupPhases,
    },
    /// Earlier recovery prevented a complete disposition.
    Recovered(AsyncStorageExitRecoveryCause),
}

/// The exact missing analysis that prevented one storage disposition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AsyncStorageExitRecoveryCause {
    /// Storage planning did not publish the identity's root access.
    UnavailableRootAccess,
    /// Recovery prevented the checker from deciding the cleanup shape.
    UnavailableCleanupShape,
    /// Partial represented storage has no complete codegen-ready cleanup plan.
    UnavailablePartialCleanup,
    /// Cleanup dependencies do not admit a complete lifecycle order.
    UnavailableCleanupOrder,
}

/// Type-driven cleanup work required when an owned initialized identity leaves its scope.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AsyncStorageCleanupRequirement {
    /// The stored type requires no cleanup phase.
    None,
    /// The stored type requires these ordered cleanup phases.
    Cleanup(AsyncCleanupPhases),
    /// Recovery prevented a complete cleanup requirement.
    Recovered(AsyncStorageExitRecoveryCause),
}

/// Independently checked ownership, transfer, and cleanup facts for one live identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AsyncStorageRequirement {
    identity: StorageIdentityId,
    owner: Option<StorageScope>,
    transfers: bool,
    cleanup: AsyncStorageCleanupRequirement,
    parts: Option<Arc<[crate::StorageCleanupPart]>>,
}

impl AsyncStorageRequirement {
    /// Creates one checked storage requirement.
    pub const fn new(
        identity: StorageIdentityId,
        owner: Option<StorageScope>,
        transfers: bool,
        cleanup: AsyncStorageCleanupRequirement,
    ) -> Self {
        Self {
            identity,
            owner,
            transfers,
            cleanup,
            parts: None,
        }
    }

    /// Selects represented-part cleanup instead of whole-value cleanup for this identity.
    pub fn with_parts(
        mut self,
        parts: impl IntoIterator<Item = crate::StorageCleanupPart>,
    ) -> Self {
        self.parts = Some(shared_slice(parts));

        self
    }

    /// Returns the complete checked partition when this identity needs represented-part cleanup.
    pub fn parts(&self) -> Option<&[crate::StorageCleanupPart]> {
        self.parts.as_deref()
    }

    /// Selects the contiguous part range below a checked destination path.
    pub fn parts_below(&self, prefix: &[crate::StorageProjection])
        -> Option<(Arc<[crate::StorageCleanupPart]>, std::ops::Range<usize>)>
    {
        let parts = self.parts.as_ref()?;

        let contains = |part: &crate::StorageCleanupPart| {
            part.projections().get(..prefix.len()).is_some_and(|projections| {
                projections.iter().zip(prefix).all(|(part, projection)| part.projection().contains(*projection))
            })
        };

        let start = parts.iter().position(contains)?;
        let end = start + parts[start..].iter().take_while(|part| contains(part)).count();

        Some((Arc::clone(parts), start..end))
    }

    /// Returns the storage identity described by this requirement.
    pub const fn identity(&self) -> StorageIdentityId {
        self.identity
    }

    /// Returns the lexical scope that owns the identity.
    pub const fn owner(&self) -> Option<StorageScope> {
        self.owner
    }

    /// Returns whether this identity transfers through its unit boundary.
    pub const fn transfers(&self) -> bool {
        self.transfers
    }

    /// Returns the type-driven cleanup requirement.
    pub const fn cleanup(&self) -> AsyncStorageCleanupRequirement {
        self.cleanup
    }
}

/// One live storage identity and its scope-exit disposition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AsyncStorageExitDecision {
    identity: StorageIdentityId,
    disposition: AsyncStorageExitDisposition,
}

impl AsyncStorageExitDecision {
    /// Creates one source-correlated storage disposition.
    pub const fn new(
        identity: StorageIdentityId,
        disposition: AsyncStorageExitDisposition,
    ) -> Self {
        Self {
            identity,
            disposition,
        }
    }

    /// Returns the live storage identity being disposed.
    pub const fn identity(self) -> StorageIdentityId {
        self.identity
    }

    /// Returns the exact checked disposition.
    pub const fn disposition(self) -> AsyncStorageExitDisposition {
        self.disposition
    }
}

/// A malformed async analysis table.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AsyncAnalysisBuildError {
    /// One analysis references semantic state owned by another bound unit.
    ForeignUnit,
    /// One occurrence has more than one suspension record.
    DuplicateSuspension,
    /// One expression has more than one selected task operation.
    DuplicateTaskOperation,
    /// Storage requirement identities are duplicated or out of order.
    UnorderedStorageRequirements,
}

/// Durable async frame, suspension, task, and cleanup analysis for one bound unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedAsync {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    frame_dependencies: Arc<[BoundDependencySubject]>,
    suspensions: Arc<[AsyncSuspensionPoint]>,
    task_operations: Arc<[AsyncTaskOperation]>,
    storage_requirements: Arc<[AsyncStorageRequirement]>,
    cleanup_types: Arc<[crate::StorageCleanupType]>,
    is_recovered: bool,
}

impl CheckedAsync {
    /// Validates and creates one immutable async analysis table.
    /// Storage requirements must be in identity order.
    #[expect(
        clippy::too_many_arguments,
        reason = "async analysis validates independent tables against one source unit identity"
    )]
    pub fn try_new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        frame_dependencies: impl IntoIterator<Item = BoundDependencySubject>,
        suspensions: impl IntoIterator<Item = AsyncSuspensionPoint>,
        task_operations: impl IntoIterator<Item = AsyncTaskOperation>,
        storage_requirements: impl Into<Arc<[AsyncStorageRequirement]>>,
        cleanup_types: impl IntoIterator<Item = crate::StorageCleanupType>,
        is_recovered: bool,
    ) -> Result<Self, AsyncAnalysisBuildError> {
        let frame_dependencies = sorted_unique_shared_slice(frame_dependencies);
        let mut suspensions = suspensions.into_iter().collect::<Vec<_>>();
        let mut task_operations = task_operations.into_iter().collect::<Vec<_>>();
        let storage_requirements = storage_requirements.into();
        let cleanup_types = shared_slice(cleanup_types);

        if frame_dependencies
            .iter()
            .any(|subject| !subject.is_valid_for(unit))
            || suspensions.iter().any(|suspension| {
                suspension.occurrence().node().unit() != unit
                    || suspension
                        .cancellation_broadcast()
                        .iter()
                        .any(|access| access.unit() != unit)
                    || match suspension.kind() {
                        AsyncSuspensionKind::Await { operand } => operand.unit() != unit,
                        AsyncSuspensionKind::Yield
                        | AsyncSuspensionKind::ScopedCall
                        | AsyncSuspensionKind::Cleanup => false,
                    }
                    || suspension
                        .retained_subjects()
                        .iter()
                        .any(|subject| !subject.is_valid_for(unit))
            })
            || task_operations
                .iter()
                .any(|operation| operation.expression().unit() != unit)
            || storage_requirements.iter().any(|requirement| {
                requirement.identity().unit() != unit
                    || requirement
                        .owner()
                        .is_some_and(|owner| owner.unit() != unit)
                    || requirement
                        .parts()
                        .is_some_and(|parts| parts.iter().any(|part| !part.is_valid_for(unit)))
            })
            || cleanup_types.iter().any(|shape| {
                shape
                    .components()
                    .into_iter()
                    .flatten()
                    .any(|projection| !projection.is_valid_for(unit))
            })
        {
            return Err(AsyncAnalysisBuildError::ForeignUnit);
        }

        suspensions.sort_unstable_by_key(AsyncSuspensionPoint::occurrence);
        task_operations.sort_unstable_by_key(|operation| operation.expression());

        if suspensions.windows(2).any(|pair| pair[0].occurrence() == pair[1].occurrence()) {
            return Err(AsyncAnalysisBuildError::DuplicateSuspension);
        }

        if task_operations.windows(2).any(|pair| pair[0].expression() == pair[1].expression()) {
            return Err(AsyncAnalysisBuildError::DuplicateTaskOperation);
        }

        if !storage_requirements.is_sorted_by(|left, right| left.identity() < right.identity()) {
            return Err(AsyncAnalysisBuildError::UnorderedStorageRequirements);
        }

        Ok(Self {
            unit,
            kind,
            frame_dependencies,
            suspensions: suspensions.into(),
            task_operations: task_operations.into(),
            storage_requirements,
            cleanup_types,
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

    /// Returns suspension points in occurrence order.
    pub fn suspensions(&self) -> &[AsyncSuspensionPoint] {
        &self.suspensions
    }

    /// Returns the checked suspension at the given occurrence.
    pub fn suspension(&self, occurrence: crate::BoundExecutionSite) -> Option<&AsyncSuspensionPoint> {
        self.suspensions.binary_search_by_key(&occurrence, AsyncSuspensionPoint::occurrence)
            .ok().map(|index| &self.suspensions[index])
    }

    /// Returns task operations in expression order.
    pub fn task_operations(&self) -> &[AsyncTaskOperation] {
        &self.task_operations
    }

    /// Returns the task operation selected for one expression.
    pub fn task_operation(&self, expression: BoundExpressionId) -> Option<AsyncTaskOperationKind> {
        self.task_operations.binary_search_by_key(&expression, |operation| operation.expression())
            .ok().map(|index| self.task_operations[index].kind())
    }

    /// Returns independently checked storage requirements in identity order.
    pub fn storage_requirements(&self) -> &[AsyncStorageRequirement] {
        &self.storage_requirements
    }

    /// Returns the checked cleanup requirement for one storage identity.
    pub fn storage_requirement(
        &self,
        identity: StorageIdentityId,
    ) -> Option<&AsyncStorageRequirement> {
        self.storage_requirements.binary_search_by_key(&identity, AsyncStorageRequirement::identity)
            .ok()
            .map(|index| &self.storage_requirements[index])
    }

    /// Returns independently checked cleanup types, including hidden represented members.
    pub fn cleanup_types(&self) -> &[crate::StorageCleanupType] {
        &self.cleanup_types
    }

    /// Returns whether recovery prevented complete async checking.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AsyncStorageRequirement, AsyncSuspensionKind,
        AsyncSuspensionPoint, AsyncTaskOperation, AsyncTaskOperationKind, CheckedAsync,
    };
    use crate::{
        AsyncStorageCleanupRequirement, BoundDependencySubject, BoundExpressionId,
        BoundUnitId, BoundUnitKind, StorageAccessId, StorageIdentityId,
    };

    #[test]
    fn checked_async_normalizes_frame_dependencies_and_indexes_operations() {
        let unit = BoundUnitId::new(7);
        let await_expression = BoundExpressionId::from_slot(unit, 2);
        let operand = BoundExpressionId::from_slot(unit, 1);
        let join_expression = BoundExpressionId::from_slot(unit, 3);
        let storage = StorageAccessId::from_slot(unit, 3);
        let dependency = BoundDependencySubject::StorageAccess(storage);

        let suspension = AsyncSuspensionPoint::new(
            await_expression,
            AsyncSuspensionKind::Await { operand },
            [],
            [dependency],
            [],
            false,
        );

        let start = AsyncTaskOperation::new(operand, AsyncTaskOperationKind::Start);
        let join = AsyncTaskOperation::new(join_expression, AsyncTaskOperationKind::Join);

        let analysis = CheckedAsync::try_new(
            unit,
            BoundUnitKind::CallableBody,
            [dependency, dependency],
            [suspension],
            [join, start],
            [],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("unit-local analysis must build: {error:?}"));

        assert_eq!(analysis.frame_dependencies(), &[dependency]);
        assert_eq!(analysis.task_operations(), &[start, join]);
        assert_eq!(analysis.task_operation(operand), Some(AsyncTaskOperationKind::Start));
        assert_eq!(analysis.suspension(await_expression.into()), analysis.suspensions().first());
    }

    #[test]
    fn checked_async_rejects_duplicate_suspension_occurrences() {
        let unit = BoundUnitId::new(7);
        let occurrence = BoundExpressionId::from_slot(unit, 1);
        let point = AsyncSuspensionPoint::new(occurrence, AsyncSuspensionKind::Yield, [], [], [], false);

        let analysis = CheckedAsync::try_new(
            unit,
            BoundUnitKind::CallableBody,
            [],
            [point.clone(), point],
            [],
            [],
            [],
            false,
        );

        assert_eq!(analysis, Err(super::AsyncAnalysisBuildError::DuplicateSuspension));
    }

    #[test]
    fn async_analysis_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CheckedAsync>();
    }

    #[test]
    fn cleanup_queries_borrow_checked_storage_in_identity_order() {
        let unit = BoundUnitId::new(7);
        let first_identity = StorageIdentityId::from_slot(unit, 1);
        let second_identity = StorageIdentityId::from_slot(unit, 2);

        let requirements = [first_identity, second_identity].map(|identity| {
            AsyncStorageRequirement::new(
                identity,
                None,
                false,
                AsyncStorageCleanupRequirement::None,
            )
        });

        let analysis = CheckedAsync::try_new(
            unit,
            BoundUnitKind::CallableBody,
            [],
            [],
            [],
            requirements.clone(),
            [],
            false,
        )
        .expect("fixture checked async must be valid");

        assert_eq!(analysis.storage_requirements(), &requirements);

        assert_eq!(
            analysis.storage_requirement(first_identity),
            Some(&requirements[0])
        );

        assert_eq!(
            analysis.storage_requirement(second_identity),
            Some(&requirements[1])
        );

        for missing_unit in [unit, BoundUnitId::new(8)] {
            assert!(
                analysis
                    .storage_requirement(StorageIdentityId::from_slot(missing_unit, 3))
                    .is_none()
            );

        }
    }

    #[test]
    fn cleanup_type_components_reject_foreign_expression_identities() {
        let unit = BoundUnitId::new(7);

        let values = bray_symbols::SemanticValueStore::try_new()
            .expect("fixture semantic value store must be valid");

        let ty = values
            .intern_type(bray_symbols::TypeData::tuple([]))
            .expect("fixture type must be interned");

        for component_unit in [unit, BoundUnitId::new(8)] {
            let projection = crate::StorageCleanupProjection::new(
                crate::StorageProjection::Element(BoundExpressionId::from_slot(component_unit, 1)),
                ty,
                ty,
            );

            let shape =
                crate::StorageCleanupType::new(ty, crate::AsyncStorageCleanupRequirement::None)
                    .with_components([projection], None, false);

            let result = CheckedAsync::try_new(
                unit,
                BoundUnitKind::CallableBody,
                [],
                [],
                [],
                [],
                [shape],
                false,
            );

            if component_unit == unit {
                assert_eq!(
                    result
                        .expect("fixture `result` call must succeed")
                        .cleanup_types()[0]
                        .components(),
                    Some([projection].as_slice())
                );
            } else {
                assert!(matches!(
                    result,
                    Err(super::AsyncAnalysisBuildError::ForeignUnit)
                ));
            }
        }
    }
}
