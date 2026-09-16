use bray_bound_tree::BoundUnitIdentity;
use bray_runtime_interface::ExecutableHostContract;
use bray_symbols::TypeId;

use crate::{
    MirBlock, MirBlockId, MirBlockKind, MirFrameDescriptor, MirHelperReference, MirOperation,
    MirOperationCommit, MirOperationId, MirOperationKind, MirSourceAnchor, MirSourceOrigin,
    MirStorage, MirStorageId, MirStorageKind, MirTargetContract, MirTerminator, MirTerminatorKind,
    MirUnit, MirUnitId, MirUnitKey, MirUnitKind, MirValue, MirValueId, MirValueOrigin,
};

use super::MirCapacityError;

#[derive(Debug)]
pub(super) struct MirBlockBuilder {
    source: MirSourceAnchor,
    kind: MirBlockKind,
    parameters: Vec<MirValueId>,
    operations: Vec<MirOperationId>,
    terminator: Option<MirTerminator>,
}

/// Builder for one immutable MIR unit.
#[derive(Debug)]
pub struct MirUnitBuilder {
    key: MirUnitKey,
    unit: MirUnitId,
    source: MirSourceOrigin,
    target: MirTargetContract,
    kind: MirUnitKind,
    frame_descriptor: Option<MirFrameDescriptor>,
    blocks: Vec<MirBlockBuilder>,
    operations: Vec<MirOperation>,
    storages: Vec<MirStorage>,
    values: Vec<MirValue>,
}

impl MirUnitBuilder {
    /// Returns the target contract retained by the body under construction.
    pub const fn target(&self) -> &MirTargetContract {
        &self.target
    }

    /// Starts construction of a compiler-provided body with ordinary synchronous calling semantics.
    pub fn for_compiler_provided_callable(
        unit: MirUnitId,
        definition: bray_symbols::CallableDefinitionId,
        target: MirTargetContract,
    ) -> Self {
        Self {
            key: MirUnitKey::CompilerProvidedCallable(definition),
            unit,
            source: MirSourceOrigin::CompilerProvidedCallable(definition),
            target,
            kind: MirUnitKind::Synchronous,
            frame_descriptor: None,
            blocks: Vec::new(),
            operations: Vec::new(),
            storages: Vec::new(),
            values: Vec::new(),
        }
    }

    /// Starts MIR reconstruction for one checked executable template from a dependency.
    pub fn for_imported_executable(
        unit: MirUnitId,
        key: crate::MirImportedExecutableKey,
        kind: MirUnitKind,
        target: MirTargetContract,
    ) -> Self {
        Self {
            key: MirUnitKey::ImportedExecutable(key),
            unit,
            source: MirSourceOrigin::ImportedExecutable(key),
            target,
            kind,
            frame_descriptor: None,
            blocks: Vec::new(),
            operations: Vec::new(),
            storages: Vec::new(),
            values: Vec::new(),
        }
    }

    /// Starts MIR construction for one checked bound unit.
    pub fn for_bound(
        identity: BoundUnitIdentity<'_>,
        kind: MirUnitKind,
        target: MirTargetContract,
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
        target: MirTargetContract,
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

    /// Starts MIR construction for one type-specialized generated lifecycle definition.
    pub fn for_generated_lifecycle(
        unit: MirUnitId,
        key: MirUnitKey,
        reference: MirHelperReference,
        target: MirTargetContract,
    ) -> Self {
        Self {
            key,
            unit,
            source: MirSourceOrigin::GeneratedLifecycle(reference.clone()),
            target,
            kind: MirUnitKind::GeneratedLifecycle(reference),
            frame_descriptor: None,
            blocks: Vec::new(),
            operations: Vec::new(),
            storages: Vec::new(),
            values: Vec::new(),
        }
    }

    /// Attaches the hidden descriptor for this unit's protected async frame.
    pub fn set_frame_descriptor(&mut self, descriptor: MirFrameDescriptor) {
        assert!(
            self.frame_descriptor.is_none(),
            "MIR unit already has a frame descriptor"
        );

        match &self.kind {
            MirUnitKind::ProtectedAsyncFrame(frame) if *frame == descriptor.frame() => {}
            MirUnitKind::ProtectedAsyncFrame(_) => panic!("MIR frame descriptor has the wrong frame"),
            MirUnitKind::Synchronous
            | MirUnitKind::ExecutableHost(_)
            | MirUnitKind::GeneratedLifecycle(_) => panic!("MIR unit cannot carry a frame descriptor"),
        }

        self.frame_descriptor = Some(descriptor);
    }

    /// Adds one block in deterministic construction order.
    pub fn push_block(
        &mut self,
        source: MirSourceAnchor,
        kind: MirBlockKind,
    ) -> Result<MirBlockId, MirCapacityError> {
        self.assert_source(&source);

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

    /// Returns the category of a block already allocated by this builder.
    pub fn block_kind(&self, block: MirBlockId) -> MirBlockKind {
        self.blocks[self.block_index(block)].kind
    }

    /// Resolves the type of an operand while its unit is still being built.
    pub fn operand_type(&self, operand: &crate::MirOperand) -> TypeId {
        super::model::resolve_operand_type(self.unit, &self.values, operand)
            .expect("MIR operand must reference a value owned by this unit")
    }

    /// Adds one incoming block value in parameter order.
    pub fn push_block_parameter(
        &mut self,
        block: MirBlockId,
        source: MirSourceAnchor,
        ty: TypeId,
    ) -> Result<MirValueId, MirCapacityError> {
        self.assert_source(&source);

        let block_index = self.block_index(block);
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
    ) -> Result<MirStorageId, MirCapacityError> {
        self.assert_source(&source);

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
    ) -> Result<MirOperationCommit, MirCapacityError> {
        self.assert_source(&source);

        let block_index = self.block_index(block);
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
    ) {
        self.assert_source(&source);

        let block_index = self.block_index(block);

        assert!(
            self.blocks[block_index].terminator.is_none(),
            "MIR block already has a terminator"
        );

        self.blocks[block_index].terminator = Some(MirTerminator::new(source, kind));
    }

    /// Visits already terminated blocks while the unit is still being built.
    pub fn terminated_blocks(&self) -> impl Iterator<Item = (MirBlockId, &MirTerminator)> {
        self.blocks.iter().enumerate().filter_map(|(index, block)| {
            let terminal = block.terminator.as_ref()?;

            // push_block already checked that every retained block index fits its ID.
            let slot =
                u32::try_from(index).unwrap_or_else(|_| unreachable!("validated block index"));

            Some((MirBlockId::from_slot(self.unit, slot), terminal))
        })
    }

    /// Returns whether any completed block transfers control to the target block.
    pub fn has_incoming_edge(&self, target: MirBlockId) -> bool {
        self.block_index(target);

        self.blocks.iter().any(|block| {
            block.terminator.as_ref().is_some_and(|terminator| {
                let mut reaches_target = false;

                terminator.kind().for_each_successor(|successor| {
                    reaches_target |= successor == target;
                });

                reaches_target
            })
        })
    }

    /// Returns whether completed control flow can reach the target from the entry block.
    pub fn is_reachable(
        &self,
        entry: MirBlockId,
        target: MirBlockId,
    ) -> bool {
        let entry_index = self.block_index(entry);
        let target_index = self.block_index(target);
        let mut visited = vec![false; self.blocks.len()];
        let mut pending = vec![entry_index];

        while let Some(index) = pending.pop() {
            if visited[index] {
                continue;
            }

            if index == target_index {
                return true;
            }

            visited[index] = true;

            let Some(terminator) = self.blocks[index].terminator.as_ref() else {
                continue;
            };

            terminator
                .kind()
                .for_each_successor(|successor| pending.push(self.block_index(successor)));
        }

        false
    }

    /// Completes the MIR unit after its producers have established their contracts.
    pub fn finish(self, entry: MirBlockId) -> MirUnit {
        assert_eq!(
            entry.unit(),
            self.unit,
            "MIR entry block belongs to another unit"
        );

        assert_eq!(
            self.block_kind(entry),
            MirBlockKind::Ordinary,
            "MIR entry block must be ordinary"
        );

        let blocks = self
            .blocks
            .into_iter()
            .enumerate()
            .map(|(index, block)| {
                compact_slot(index).expect("MIR block identity capacity was checked at insertion");

                let terminator = block
                    .terminator
                    .expect("MIR blocks must be terminated before finishing");

                MirBlock::new(
                    block.source,
                    block.kind,
                    block.parameters,
                    block.operations,
                    terminator,
                )
            })
            .collect::<Vec<_>>();

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

        unit
    }

    fn assert_source(&self, source: &MirSourceAnchor) {
        assert!(
            source.belongs_to(&self.source),
            "MIR source anchor belongs to another unit"
        );
    }

    fn block_index(&self, block: MirBlockId) -> usize {
        assert_eq!(
            block.unit(),
            self.unit,
            "MIR block belongs to another unit"
        );

        let index = block.to_index().expect("MIR block has no valid slot");
        assert!(index < self.blocks.len(), "MIR block was not allocated");

        index
    }
}

fn compact_slot(index: usize) -> Result<u32, MirCapacityError> {
    crate::id::compact_slot(index).ok_or(MirCapacityError::IdentityCapacityExceeded)
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundNodeOrigin, BoundSourceAnchor, CheckedMemoryOperationKind};
    use bray_runtime_interface::{
        ProtectedAsyncFrameId, ProtectedFrameAbiVersions, RuntimeAbiRole, RuntimeAbiVersion,
    };
    use bray_source::SourceVersion;
    use bray_symbols::BorrowKind;
    use bray_testing::test_bound_unit;

    use super::MirUnitBuilder;
    use crate::{
        MirAggregate, MirAggregateKind, MirAsyncOperation, MirBlockKind, MirCleanupEdge,
        MirCleanupPhase, MirEdge, MirFrameDescriptor, MirFrameState, MirFrameStateId,
        MirImmediateValue, MirMemoryOperation, MirOperand, MirOperationKind, MirPlace,
        MirProjection, MirProjectionKind, MirRuntimeReference, MirSourceAnchor, MirStorageKind,
        MirTerminatorKind, MirUnitKind,
    };

    #[test]
    fn compiler_provided_bodies_reject_other_declarations_and_source_anchors() {
        let definition = |ordinal| {
            bray_symbols::CallableDefinitionId::try_new(
                bray_symbols::FunctionSymbolId::from_symbol_id(bray_symbols::SymbolId::new(
                    ordinal,
                ))
                .into(),
            )
            .unwrap()
        };

        let owner = definition(3);
        let source = MirSourceAnchor::CompilerProvidedCallable(owner);

        let mut builder = MirUnitBuilder::for_compiler_provided_callable(
            crate::MirUnitId::new(9),
            owner,
            crate::test_support::test_target(),
        );

        let bound = test_bound_unit(4);

        for foreign in [
            MirSourceAnchor::CompilerProvidedCallable(definition(4)),
            MirSourceAnchor::from(bound.key().source()),
        ] {
            assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                builder.push_block(foreign, MirBlockKind::Ordinary)
            }))
            .is_err());
        }

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap();

        builder.set_terminator(entry, source, MirTerminatorKind::Return(None));

        let unit = builder.finish(entry);

        assert_eq!(
            unit.key(),
            &crate::MirUnitKey::CompilerProvidedCallable(owner)
        );

        assert_eq!(
            unit.source(),
            &crate::MirSourceOrigin::CompilerProvidedCallable(owner)
        );
    }

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
            MirTerminatorKind::Goto(MirEdge::new(continuation, [MirOperand::Value(result)])),
        );

        set_terminator(
            &mut builder,
            continuation,
            source,
            MirTerminatorKind::Return(Some(MirOperand::Value(parameter))),
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
    fn builder_lookups_distinguish_foreign_and_missing_values() {
        let bound = test_bound_unit(96);
        let source = MirSourceAnchor::from(bound.key().source());
        let ty = crate::test_support::test_type();
        let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap();

        let value = builder.push_block_parameter(entry, source, ty).unwrap();
        assert_eq!(builder.block_kind(entry), MirBlockKind::Ordinary);
        assert_eq!(builder.operand_type(&MirOperand::Value(value)), ty);
        let foreign = crate::MirValueId::from_slot(crate::MirUnitId::new(97), 0);
        let missing = crate::MirValueId::from_slot(entry.unit(), 10);

        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            builder.operand_type(&MirOperand::Value(foreign))
        }))
        .is_err());

        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            builder.operand_type(&MirOperand::Value(missing))
        }))
        .is_err());

        for operand in [
            MirOperand::Immediate {
                value: MirImmediateValue::Unit,
                ty,
            },
            MirOperand::Copy(MirPlace::new(
                crate::MirStorageId::from_slot(entry.unit(), 0),
                [],
                ty,
            )),
        ] {
            assert_eq!(builder.operand_type(&operand), ty);
        }
    }

    #[test]
    fn builders_report_completed_incoming_edges() {
        let bound = test_bound_unit(28);
        let source = MirSourceAnchor::from(bound.key().source());

        let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);

        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let join = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);

        assert!(!builder.has_incoming_edge(join));

        set_terminator(
            &mut builder,
            entry,
            source,
            MirTerminatorKind::Goto(MirEdge::new(join, [])),
        );

        assert!(builder.has_incoming_edge(join));
    }

    #[test]
    fn builders_distinguish_reachable_edges_from_unreachable_predecessors() {
        let bound = test_bound_unit(29);
        let source = MirSourceAnchor::from(bound.key().source());

        let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);

        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let disconnected = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let join = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);

        set_terminator(
            &mut builder,
            entry,
            source.clone(),
            MirTerminatorKind::Return(None),
        );

        set_terminator(
            &mut builder,
            disconnected,
            source,
            MirTerminatorKind::Goto(MirEdge::new(join, [])),
        );

        assert!(builder.has_incoming_edge(join));
        assert!(!builder.is_reachable(entry, join));
        assert!(builder.is_reachable(disconnected, join));
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

        assert!(!builder.finish(entry).is_valid());
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

        assert!(!builder.finish(entry).is_valid());
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
            MirTerminatorKind::Return(Some(MirOperand::Value(foreign))),
        );

        set_terminator(&mut builder, other, source, MirTerminatorKind::Return(None));

        assert!(!builder.finish(entry).is_valid());
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

        let _operation = match builder.push_operation(
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

        assert!(!builder.finish(entry).is_valid());
    }

    #[test]
    fn repeated_arrays_require_one_value_operand_and_no_count_operand() {
        let bound = test_bound_unit(12);
        let source = MirSourceAnchor::from(bound.key().source());
        let ty = crate::test_support::test_type();

        let operand = MirOperand::Immediate {
            value: MirImmediateValue::Unit,
            ty,
        };

        for count in 0..=2 {
            let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);
            let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);

            let _operation = match builder.push_operation(
                entry,
                source.clone(),
                MirOperationKind::Aggregate(MirAggregate::new(
                    MirAggregateKind::RepeatedArray,
                    std::iter::repeat_n(operand.clone(), count),
                )),
                Some(ty),
            ) {
                Ok(operation) => operation.operation(),
                Err(error) => panic!("test MIR operation must commit before validation: {error:?}"),
            };

            set_terminator(
                &mut builder,
                entry,
                source.clone(),
                MirTerminatorKind::Return(None),
            );

            if count == 1 {
                assert!(builder.finish(entry).is_valid());
            } else {
                assert!(!builder.finish(entry).is_valid());
            }
        }
    }

    #[test]
    fn builders_reject_invalid_memory_operation_shapes() {
        let bound = test_bound_unit(14);
        let source = MirSourceAnchor::from(bound.key().source());
        let ty = crate::test_support::test_type();

        let operand = MirOperand::Immediate {
            value: MirImmediateValue::Unit,
            ty,
        };

        let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);
        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);

        let _operation = match builder.push_operation(
            entry,
            source.clone(),
            MirOperationKind::Memory(MirMemoryOperation::new(
                CheckedMemoryOperationKind::Write { pointee: ty },
                [operand],
                [ty],
                None,
            )),
            None,
        ) {
            Ok(operation) => operation.operation(),
            Err(error) => panic!("test MIR operation must commit before validation: {error:?}"),
        };

        set_terminator(&mut builder, entry, source, MirTerminatorKind::Return(None));

        assert!(!builder.finish(entry).is_valid());
    }

    #[test]
    fn builders_reject_memory_operand_type_mismatches() {
        let bound = test_bound_unit(15);
        let source = MirSourceAnchor::from(bound.key().source());
        let ty = crate::test_support::test_type();
        let other = crate::test_support::test_other_type();

        let operand = MirOperand::Immediate {
            value: MirImmediateValue::Unit,
            ty,
        };

        let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);
        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);

        let _operation = match builder.push_operation(
            entry,
            source.clone(),
            MirOperationKind::Memory(MirMemoryOperation::new(
                CheckedMemoryOperationKind::IsNull { pointee: ty },
                [operand],
                [other],
                Some(ty),
            )),
            Some(ty),
        ) {
            Ok(operation) => operation.operation(),
            Err(error) => panic!("test MIR operation must commit before validation: {error:?}"),
        };

        set_terminator(&mut builder, entry, source, MirTerminatorKind::Return(None));

        assert!(!builder.finish(entry).is_valid());
    }

    #[test]
    fn builders_reject_operation_illegal_atomic_orders() {
        let bound = test_bound_unit(16);
        let source = MirSourceAnchor::from(bound.key().source());
        let ty = crate::test_support::test_type();

        let operand = MirOperand::Immediate {
            value: MirImmediateValue::Unit,
            ty,
        };

        let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);
        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);

        let _operation = match builder.push_operation(
            entry,
            source.clone(),
            MirOperationKind::Memory(MirMemoryOperation::new(
                CheckedMemoryOperationKind::AtomicLoad {
                    value: ty,
                    order: bray_bound_tree::MemoryOrder::Release,
                },
                [operand],
                [ty],
                Some(ty),
            )),
            Some(ty),
        ) {
            Ok(operation) => operation.operation(),
            Err(error) => panic!("test MIR operation must commit before validation: {error:?}"),
        };

        set_terminator(&mut builder, entry, source, MirTerminatorKind::Return(None));

        assert!(!builder.finish(entry).is_valid());
    }

    #[test]
    fn builders_reject_incoherent_projected_place_types() {
        let bound = test_bound_unit(13);
        let source = MirSourceAnchor::from(bound.key().source());
        let storage_type = crate::test_support::test_type();
        let projected_type = crate::test_support::test_other_type();

        let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);
        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let storage = push_storage(&mut builder, source.clone(), storage_type);

        let projection = MirProjection::new(
            MirProjectionKind::TupleField(0),
            storage_type,
            projected_type,
        );

        let place = MirPlace::new(storage, [projection], storage_type);

        match builder.push_operation(
            entry,
            source.clone(),
            MirOperationKind::Borrow {
                kind: BorrowKind::Shared,
                place,
            },
            Some(storage_type),
        ) {
            Ok(_) => {}
            Err(error) => panic!("test MIR operation must commit before validation: {error:?}"),
        }

        set_terminator(&mut builder, entry, source, MirTerminatorKind::Return(None));

        assert!(!builder.finish(entry).is_valid());
    }

    #[test]
    fn checked_cleanup_outcomes_require_a_local_cancellation_edge_and_fallible_operation() {
        for (fallible, cancellation_kind) in [
            (true, MirBlockKind::Ordinary),
            (false, MirBlockKind::Ordinary),
            (true, MirBlockKind::LifecycleResolution),
        ] {
            let bound = test_bound_unit(6);
            let source = MirSourceAnchor::from(bound.key().source());
            let ty = crate::test_support::test_type();
            let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);
            let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
            let completed = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
            let panicked = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
            let cancelled = push_block(&mut builder, source.clone(), cancellation_kind);
            push_parameter(&mut builder, panicked, source.clone(), ty);
            let storage = push_storage(&mut builder, source.clone(), ty);
            let place = MirPlace::new(storage, [], ty);

            let operation = if fallible {
                MirOperationKind::Destroy(place)
            } else {
                MirOperationKind::Store {
                    kind: crate::MirStoreKind::Initialize,
                    destination: place,
                    value: MirOperand::Immediate {
                        value: MirImmediateValue::Unit,
                        ty,
                    },
                }
            };

            builder
                .push_operation(entry, source.clone(), operation, None)
                .unwrap();

            set_terminator(
                &mut builder,
                entry,
                source.clone(),
                MirTerminatorKind::CheckCallOutcome {
                    completed: MirEdge::new(completed, []),
                    panicked: crate::MirCallPanicEdge::new(panicked, ty),
                    cancelled: MirEdge::new(cancelled, []),
                },
            );

            for block in [completed, panicked, cancelled] {
                set_terminator(
                    &mut builder,
                    block,
                    source.clone(),
                    MirTerminatorKind::Return(None),
                );
            }

            let unit = builder.finish(entry);

            if !fallible {
                assert!(!unit.is_valid());
            } else if cancellation_kind != MirBlockKind::Ordinary {
                assert!(!unit.is_valid());
            } else {
                let mut successors = Vec::new();

                unit.block(entry)
                    .unwrap()
                    .terminator()
                    .kind()
                    .for_each_successor(|block| successors.push(block));

                assert_eq!(successors, [completed, panicked, cancelled]);
            }
        }
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

        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            builder.finish(entry)
        }))
        .is_err());

        let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);

        let foreign = BoundSourceAnchor::new(
            source.syntax(),
            SourceVersion::new(source.source_version().raw() + 1),
        );

        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            builder.push_block(
                MirSourceAnchor::source(BoundNodeOrigin::source(foreign)),
                MirBlockKind::Ordinary,
            )
        }))
        .is_err());
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

        let task = MirOperand::Move(MirPlace::new(storage, [], ty));

        let runtime =
            MirRuntimeReference::new(RuntimeAbiRole::TaskStart, RuntimeAbiVersion::new(1, 0));

        let operation = MirAsyncOperation::RequestTaskCancellation { task, runtime };

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

        builder.set_frame_descriptor(descriptor);

        assert!(!builder.finish(entry).is_valid());
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
        builder.set_terminator(block, source, terminator);
    }

    fn finish(builder: MirUnitBuilder, entry: crate::MirBlockId) -> crate::MirUnit {
        builder.finish(entry)
    }

    fn frame_descriptor(
        frame: ProtectedAsyncFrameId,
        entry: crate::MirBlockId,
        result_type: bray_symbols::TypeId,
    ) -> MirFrameDescriptor {
        let state = MirFrameState::new(MirFrameStateId::new(0), entry, [], []);

        let abi = RuntimeAbiVersion::new(1, 0);

        match MirFrameDescriptor::new(
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
