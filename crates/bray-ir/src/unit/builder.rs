use bray_bound_tree::BoundUnitIdentity;
use bray_runtime_interface::ExecutableHostContract;
use bray_symbols::TypeId;

use crate::{
    MirBlock, MirBlockId, MirBlockKind, MirFrameDescriptor, MirOperation, MirOperationCommit,
    MirOperationId, MirOperationKind, MirSourceAnchor, MirSourceOrigin, MirStorage, MirStorageId,
    MirStorageKind, MirTargetFacts, MirTerminator, MirTerminatorKind, MirUnit, MirUnitId,
    MirUnitKey, MirUnitKind, MirValue, MirValueId, MirValueOrigin,
};

use super::MirUnitBuildError;
use super::validation::validate_unit;

#[derive(Debug)]
pub(super) struct MirBlockBuilder {
    source: MirSourceAnchor,
    kind: MirBlockKind,
    parameters: Vec<MirValueId>,
    operations: Vec<MirOperationId>,
    terminator: Option<MirTerminator>,
}

/// Builder for one validated immutable MIR unit.
#[derive(Debug)]
pub struct MirUnitBuilder {
    key: MirUnitKey,
    unit: MirUnitId,
    source: MirSourceOrigin,
    target: MirTargetFacts,
    kind: MirUnitKind,
    frame_descriptor: Option<MirFrameDescriptor>,
    blocks: Vec<MirBlockBuilder>,
    operations: Vec<MirOperation>,
    storages: Vec<MirStorage>,
    values: Vec<MirValue>,
}

impl MirUnitBuilder {
    /// Starts MIR construction for one checked bound unit.
    pub fn for_bound(
        identity: BoundUnitIdentity<'_>,
        kind: MirUnitKind,
        target: MirTargetFacts,
    ) -> Self {
        let source = identity.key().source();

        // The stable key is Arc-backed and must outlive the borrowed bound-unit view.
        let key = identity.key().clone();

        Self {
            key: MirUnitKey::Bound(key),
            unit: MirUnitId::new(identity.unit().raw()),
            source: MirSourceOrigin::Source(source),
            target,
            kind,
            frame_descriptor: None,
            blocks: Vec::new(),
            operations: Vec::new(),
            storages: Vec::new(),
            values: Vec::new(),
        }
    }

    /// Starts MIR construction for one compiler-generated executable host.
    pub fn for_executable_host(
        unit: MirUnitId,
        host: ExecutableHostContract,
        target: MirTargetFacts,
    ) -> Self {
        // Product identities are Arc-backed and the generated unit owns its provenance.
        let product = host.product().clone();

        Self {
            key: MirUnitKey::ExecutableHost(product.clone()),
            unit,
            source: MirSourceOrigin::ExecutableHost(product),
            target,
            kind: MirUnitKind::ExecutableHost(host),
            frame_descriptor: None,
            blocks: Vec::new(),
            operations: Vec::new(),
            storages: Vec::new(),
            values: Vec::new(),
        }
    }

    /// Attaches the hidden descriptor for this unit's protected async frame.
    pub fn set_frame_descriptor(
        &mut self,
        descriptor: MirFrameDescriptor,
    ) -> Result<(), MirUnitBuildError> {
        if self.frame_descriptor.is_some() {
            return Err(MirUnitBuildError::DuplicateFrameDescriptor);
        }

        match &self.kind {
            MirUnitKind::ProtectedAsyncFrame(frame) if *frame == descriptor.frame() => {}
            MirUnitKind::ProtectedAsyncFrame(_) => {
                return Err(MirUnitBuildError::ProtectedFrameMismatch);
            }
            MirUnitKind::Synchronous | MirUnitKind::ExecutableHost(_) => {
                return Err(MirUnitBuildError::UnexpectedFrameDescriptor);
            }
        }

        self.frame_descriptor = Some(descriptor);

        Ok(())
    }

    /// Adds one block in deterministic construction order.
    pub fn push_block(
        &mut self,
        source: MirSourceAnchor,
        kind: MirBlockKind,
    ) -> Result<MirBlockId, MirUnitBuildError> {
        self.validate_source(&source)?;

        let id = MirBlockId::from_slot(self.unit, compact_slot(self.blocks.len())?);

        self.blocks.push(MirBlockBuilder {
            source,
            kind,
            parameters: Vec::new(),
            operations: Vec::new(),
            terminator: None,
        });

        Ok(id)
    }

    /// Adds one incoming block value in parameter order.
    pub fn push_block_parameter(
        &mut self,
        block: MirBlockId,
        source: MirSourceAnchor,
        ty: TypeId,
    ) -> Result<MirValueId, MirUnitBuildError> {
        self.validate_source(&source)?;

        let block_index = self.block_index(block)?;
        let value = MirValueId::from_slot(self.unit, compact_slot(self.values.len())?);

        self.values.push(MirValue::new(
            source,
            ty,
            MirValueOrigin::BlockParameter(block),
        ));

        self.blocks[block_index].parameters.push(value);

        Ok(value)
    }

    /// Adds one typed storage allocation in deterministic construction order.
    pub fn push_storage(
        &mut self,
        source: MirSourceAnchor,
        kind: MirStorageKind,
        ty: TypeId,
    ) -> Result<MirStorageId, MirUnitBuildError> {
        self.validate_source(&source)?;

        let id = MirStorageId::from_slot(self.unit, compact_slot(self.storages.len())?);

        self.storages.push(MirStorage::new(source, kind, ty));

        Ok(id)
    }

    /// Adds one operation and its optional result value to a block.
    pub fn push_operation(
        &mut self,
        block: MirBlockId,
        source: MirSourceAnchor,
        kind: MirOperationKind,
        result_type: Option<TypeId>,
    ) -> Result<MirOperationCommit, MirUnitBuildError> {
        self.validate_source(&source)?;

        let block_index = self.block_index(block)?;
        let operation = MirOperationId::from_slot(self.unit, compact_slot(self.operations.len())?);

        let result = match result_type {
            Some(ty) => {
                let value = MirValueId::from_slot(self.unit, compact_slot(self.values.len())?);

                // The result and operation independently retain the same source provenance.
                self.values.push(MirValue::new(
                    source.clone(),
                    ty,
                    MirValueOrigin::Operation(operation),
                ));

                Some(value)
            }
            None => None,
        };

        self.operations
            .push(MirOperation::new(source, kind, result));

        self.blocks[block_index].operations.push(operation);

        Ok(MirOperationCommit::new(operation, result))
    }

    /// Sets the single terminator for one block.
    pub fn set_terminator(
        &mut self,
        block: MirBlockId,
        source: MirSourceAnchor,
        kind: MirTerminatorKind,
    ) -> Result<(), MirUnitBuildError> {
        self.validate_source(&source)?;

        let block_index = self.block_index(block)?;

        if self.blocks[block_index].terminator.is_some() {
            return Err(MirUnitBuildError::DuplicateTerminator(block));
        }

        self.blocks[block_index].terminator = Some(MirTerminator::new(source, kind));

        Ok(())
    }

    /// Completes the MIR unit after validating all identities and control-flow contracts.
    pub fn finish(self, entry: MirBlockId) -> Result<MirUnit, MirUnitBuildError> {
        if entry.unit() != self.unit {
            return Err(MirUnitBuildError::ForeignBlock {
                expected: self.unit,
                actual: entry.unit(),
            });
        }

        let blocks = self
            .blocks
            .into_iter()
            .enumerate()
            .map(|(index, block)| {
                let id = MirBlockId::from_slot(self.unit, compact_slot(index)?);

                let Some(terminator) = block.terminator else {
                    return Err(MirUnitBuildError::MissingTerminator(id));
                };

                Ok(MirBlock::new(
                    block.source,
                    block.kind,
                    block.parameters,
                    block.operations,
                    terminator,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let unit = MirUnit {
            key: self.key,
            unit: self.unit,
            source: self.source,
            target: self.target,
            kind: self.kind,
            frame_descriptor: self.frame_descriptor,
            entry,
            blocks: blocks.into(),
            operations: self.operations.into(),
            storages: self.storages.into(),
            values: self.values.into(),
        };

        validate_unit(&unit)?;

        Ok(unit)
    }

    fn validate_source(&self, source: &MirSourceAnchor) -> Result<(), MirUnitBuildError> {
        if !source.belongs_to(&self.source) {
            return Err(MirUnitBuildError::SourceOriginMismatch);
        }

        Ok(())
    }

    fn block_index(&self, block: MirBlockId) -> Result<usize, MirUnitBuildError> {
        if block.unit() != self.unit {
            return Err(MirUnitBuildError::ForeignBlock {
                expected: self.unit,
                actual: block.unit(),
            });
        }

        let Some(index) = block.to_index() else {
            return Err(MirUnitBuildError::MissingBlock(block));
        };

        if index >= self.blocks.len() {
            return Err(MirUnitBuildError::MissingBlock(block));
        }

        Ok(index)
    }
}

fn compact_slot(index: usize) -> Result<u32, MirUnitBuildError> {
    crate::id::compact_slot(index).ok_or(MirUnitBuildError::IdentityCapacityExceeded)
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundNodeOrigin, BoundSourceAnchor};
    use bray_runtime_interface::{
        ProtectedAsyncFrameId, ProtectedFrameAbiVersions, RuntimeAbiRole, RuntimeAbiVersion,
    };
    use bray_source::SourceVersion;
    use bray_symbols::BorrowKind;
    use bray_testing::test_bound_unit;

    use super::MirUnitBuilder;
    use crate::{
        MirAsyncOperation, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge,
        MirFrameDescriptor, MirFrameStateFacts, MirFrameStateId, MirOperationKind, MirPlace,
        MirRuntimeReference, MirSourceAnchor, MirStorageKind, MirTerminatorKind, MirUnitBuildError,
        MirUnitKind,
    };

    #[test]
    fn builders_publish_typed_storage_values_operations_and_edges() {
        let bound = test_bound_unit(4);
        let source = MirSourceAnchor::from(bound.key().source());
        let ty = crate::test_support::test_type();

        let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);

        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let continuation = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let parameter = push_parameter(&mut builder, continuation, source.clone(), ty);
        let storage = push_storage(&mut builder, source.clone(), ty);
        let place = MirPlace::new(storage, [], ty);

        let operation = match builder.push_operation(
            entry,
            source.clone(),
            MirOperationKind::Borrow {
                kind: BorrowKind::Shared,
                place,
            },
            Some(ty),
        ) {
            Ok(operation) => operation,
            Err(error) => panic!("test MIR operation must be valid: {error:?}"),
        };

        let Some(result) = operation.result() else {
            panic!("borrow operation must produce a value");
        };

        set_terminator(
            &mut builder,
            entry,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(
                continuation,
                [crate::MirOperand::Value(result)],
            )),
        );

        set_terminator(
            &mut builder,
            continuation,
            source,
            MirTerminatorKind::Return(Some(crate::MirOperand::Value(parameter))),
        );

        let unit = finish(builder, entry);

        assert_eq!(unit.blocks().len(), 2);
        assert_eq!(unit.operations().len(), 1);
        assert_eq!(unit.storages().len(), 1);
        assert_eq!(unit.values().len(), 2);

        assert_eq!(
            unit.operation(operation.operation())
                .and_then(|item| item.result()),
            Some(result)
        );
    }

    #[test]
    fn builders_enforce_two_phase_cleanup_order() {
        let bound = test_bound_unit(5);
        let source = MirSourceAnchor::from(bound.key().source());

        let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);

        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let cancellation = push_block(&mut builder, source.clone(), MirBlockKind::CleanupBroadcast);

        let lifecycle = push_block(
            &mut builder,
            source.clone(),
            MirBlockKind::LifecycleResolution,
        );

        set_terminator(
            &mut builder,
            entry,
            source.clone(),
            MirTerminatorKind::BeginCleanup(MirCleanupEdge::new(
                MirCleanupPhase::TaskCancellation,
                MirEdge::new(cancellation, []),
            )),
        );

        set_terminator(
            &mut builder,
            cancellation,
            source.clone(),
            MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                MirCleanupPhase::LifecycleResolution,
                MirEdge::new(lifecycle, []),
            )),
        );

        set_terminator(
            &mut builder,
            lifecycle,
            source,
            MirTerminatorKind::Return(None),
        );

        let unit = finish(builder, entry);

        assert_eq!(
            unit.block(cancellation).map(|block| block.kind()),
            Some(MirBlockKind::CleanupBroadcast)
        );

        assert_eq!(
            unit.block(lifecycle).map(|block| block.kind()),
            Some(MirBlockKind::LifecycleResolution)
        );
    }

    #[test]
    fn builders_reject_lifecycle_cleanup_before_task_cancellation() {
        let bound = test_bound_unit(8);
        let source = MirSourceAnchor::from(bound.key().source());

        let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);

        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);

        let lifecycle = push_block(
            &mut builder,
            source.clone(),
            MirBlockKind::LifecycleResolution,
        );

        set_terminator(
            &mut builder,
            entry,
            source.clone(),
            MirTerminatorKind::BeginCleanup(MirCleanupEdge::new(
                MirCleanupPhase::LifecycleResolution,
                MirEdge::new(lifecycle, []),
            )),
        );

        set_terminator(
            &mut builder,
            lifecycle,
            source,
            MirTerminatorKind::Return(None),
        );

        assert_eq!(
            builder.finish(entry),
            Err(MirUnitBuildError::CleanupPhaseOrderViolation(entry))
        );
    }

    #[test]
    fn ordinary_edges_cannot_enter_cleanup_blocks() {
        let bound = test_bound_unit(10);
        let source = MirSourceAnchor::from(bound.key().source());

        let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);

        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);

        let lifecycle = push_block(
            &mut builder,
            source.clone(),
            MirBlockKind::LifecycleResolution,
        );

        set_terminator(
            &mut builder,
            entry,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(lifecycle, [])),
        );

        set_terminator(
            &mut builder,
            lifecycle,
            source,
            MirTerminatorKind::Return(None),
        );

        assert_eq!(
            builder.finish(entry),
            Err(MirUnitBuildError::CleanupPhaseOrderViolation(lifecycle))
        );
    }

    #[test]
    fn values_cross_blocks_only_through_block_arguments() {
        let bound = test_bound_unit(11);
        let source = MirSourceAnchor::from(bound.key().source());
        let ty = crate::test_support::test_type();

        let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);

        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let other = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let foreign = push_parameter(&mut builder, other, source.clone(), ty);

        set_terminator(
            &mut builder,
            entry,
            source.clone(),
            MirTerminatorKind::Return(Some(crate::MirOperand::Value(foreign))),
        );

        set_terminator(&mut builder, other, source, MirTerminatorKind::Return(None));

        assert_eq!(
            builder.finish(entry),
            Err(MirUnitBuildError::ValueDoesNotDominateUse(foreign))
        );
    }

    #[test]
    fn builders_reject_value_operations_without_results() {
        let bound = test_bound_unit(9);
        let source = MirSourceAnchor::from(bound.key().source());
        let ty = crate::test_support::test_type();

        let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);

        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let storage = push_storage(&mut builder, source.clone(), ty);
        let place = MirPlace::new(storage, [], ty);

        let operation = match builder.push_operation(
            entry,
            source.clone(),
            MirOperationKind::Borrow {
                kind: BorrowKind::Shared,
                place,
            },
            None,
        ) {
            Ok(operation) => operation.operation(),
            Err(error) => panic!("test MIR operation must commit before validation: {error:?}"),
        };

        set_terminator(&mut builder, entry, source, MirTerminatorKind::Return(None));

        assert_eq!(
            builder.finish(entry),
            Err(MirUnitBuildError::MissingOperationResult(operation))
        );
    }

    #[test]
    fn builders_reject_missing_terminators_and_foreign_sources() {
        let bound = test_bound_unit(6);
        let source = bound.key().source();

        let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);

        let entry = push_block(
            &mut builder,
            MirSourceAnchor::from(source),
            MirBlockKind::Ordinary,
        );

        assert_eq!(
            builder.finish(entry),
            Err(MirUnitBuildError::MissingTerminator(entry))
        );

        let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);

        let foreign = BoundSourceAnchor::new(
            source.syntax(),
            SourceVersion::new(source.source_version().raw() + 1),
        );

        assert_eq!(
            builder.push_block(
                MirSourceAnchor::source(BoundNodeOrigin::source(foreign)),
                MirBlockKind::Ordinary,
            ),
            Err(MirUnitBuildError::SourceOriginMismatch)
        );
    }

    #[test]
    fn protected_frames_require_matching_descriptors_and_runtime_roles() {
        let bound = test_bound_unit(7);
        let source = MirSourceAnchor::from(bound.key().source());
        let frame = ProtectedAsyncFrameId::new([7; 32]);
        let ty = crate::test_support::test_type();

        let mut builder = unit_builder(&bound, MirUnitKind::ProtectedAsyncFrame(frame));

        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);

        let storage = match builder.push_storage(source.clone(), MirStorageKind::ChildTask, ty) {
            Ok(storage) => storage,
            Err(error) => panic!("test task storage must be valid: {error:?}"),
        };

        let runtime =
            MirRuntimeReference::new(RuntimeAbiRole::TaskStart, RuntimeAbiVersion::new(1, 0));

        let operation = MirAsyncOperation::RequestTaskCancellation {
            task: storage,
            runtime,
        };

        match builder.push_operation(
            entry,
            source.clone(),
            MirOperationKind::Async(operation),
            None,
        ) {
            Ok(_) => {}
            Err(error) => panic!("test MIR operation must commit before validation: {error:?}"),
        }

        set_terminator(&mut builder, entry, source, MirTerminatorKind::Return(None));

        let descriptor = frame_descriptor(frame, entry, ty);

        if let Err(error) = builder.set_frame_descriptor(descriptor) {
            panic!("test frame descriptor must commit: {error:?}");
        }

        assert_eq!(
            builder.finish(entry),
            Err(MirUnitBuildError::RuntimeRoleMismatch {
                expected: RuntimeAbiRole::TaskCancellationRequest,
                actual: RuntimeAbiRole::TaskStart,
            })
        );
    }

    #[test]
    fn mir_units_are_safe_to_share_between_workers() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<crate::MirUnit>();
        assert_send_sync::<crate::MirBlockId>();
        assert_send_sync::<crate::MirOperationId>();
        assert_send_sync::<crate::MirStorageId>();
        assert_send_sync::<crate::MirValueId>();
    }

    fn unit_builder(bound: &bray_bound_tree::BoundUnit, kind: MirUnitKind) -> MirUnitBuilder {
        MirUnitBuilder::for_bound(bound.identity(), kind, crate::test_support::test_target())
    }

    fn push_block(
        builder: &mut MirUnitBuilder,
        source: MirSourceAnchor,
        kind: MirBlockKind,
    ) -> crate::MirBlockId {
        match builder.push_block(source, kind) {
            Ok(block) => block,
            Err(error) => panic!("test MIR block must be valid: {error:?}"),
        }
    }

    fn push_parameter(
        builder: &mut MirUnitBuilder,
        block: crate::MirBlockId,
        source: MirSourceAnchor,
        ty: bray_symbols::TypeId,
    ) -> crate::MirValueId {
        match builder.push_block_parameter(block, source, ty) {
            Ok(value) => value,
            Err(error) => panic!("test block parameter must be valid: {error:?}"),
        }
    }

    fn push_storage(
        builder: &mut MirUnitBuilder,
        source: MirSourceAnchor,
        ty: bray_symbols::TypeId,
    ) -> crate::MirStorageId {
        match builder.push_storage(source, MirStorageKind::Temporary, ty) {
            Ok(storage) => storage,
            Err(error) => panic!("test MIR storage must be valid: {error:?}"),
        }
    }

    fn set_terminator(
        builder: &mut MirUnitBuilder,
        block: crate::MirBlockId,
        source: MirSourceAnchor,
        terminator: MirTerminatorKind,
    ) {
        if let Err(error) = builder.set_terminator(block, source, terminator) {
            panic!("test MIR terminator must be valid: {error:?}");
        }
    }

    fn finish(builder: MirUnitBuilder, entry: crate::MirBlockId) -> crate::MirUnit {
        match builder.finish(entry) {
            Ok(unit) => unit,
            Err(error) => panic!("test MIR unit must be valid: {error:?}"),
        }
    }

    fn frame_descriptor(
        frame: ProtectedAsyncFrameId,
        entry: crate::MirBlockId,
        result_type: bray_symbols::TypeId,
    ) -> MirFrameDescriptor {
        let state = MirFrameStateFacts::new(MirFrameStateId::new(0), entry, [], None, []);

        let abi = RuntimeAbiVersion::new(1, 0);

        match MirFrameDescriptor::try_new(
            frame,
            abi,
            ProtectedFrameAbiVersions::uniform(abi),
            result_type,
            [state],
        ) {
            Ok(descriptor) => descriptor,
            Err(error) => panic!("test frame descriptor must be valid: {error:?}"),
        }
    }
}
