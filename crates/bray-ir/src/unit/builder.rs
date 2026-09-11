use bray_bound_tree::BoundUnitIdentity;
use bray_runtime_interface::ExecutableHostContract;
use bray_symbols::TypeId;

use crate::{
    MirBlock, MirBlockId, MirBlockKind, MirFrameDescriptor, MirHelperReference, MirOperation,
    MirOperationCommit, MirOperationId, MirOperationKind, MirSourceAnchor, MirSourceOrigin,
    MirStorage, MirStorageId, MirStorageKind, MirTargetContract, MirTerminator, MirTerminatorKind,
    MirUnit, MirUnitId, MirUnitKey, MirUnitKind, MirValue, MirValueId, MirValueOrigin,
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
    target: MirTargetContract,
    kind: MirUnitKind,
    frame_descriptor: Option<MirFrameDescriptor>,
    blocks: Vec<MirBlockBuilder>,
    operations: Vec<MirOperation>,
    storages: Vec<MirStorage>,
    values: Vec<MirValue>,
}

impl MirUnitBuilder {
    /// Reopens a template under a specialization identity while preserving source provenance and local IDs.
    pub fn for_specialization(unit: MirUnit, key: MirUnitKey, kind: MirUnitKind) -> Self {
        let mut builder = Self::from_unit(unit);

        builder.key = key;
        builder.kind = kind;

        builder
    }

    /// Reopens an immutable template for a transformation that preserves existing identities.
    /// The transformed unit is validated again by `finish`.
    pub fn from_unit(unit: MirUnit) -> Self {
        // Templates remain shared by other concrete instances. Each transformation owns its tables.
        let blocks = unit
            .blocks
            .iter()
            .map(|block| MirBlockBuilder {
                source: block.source().clone(),
                kind: block.kind(),
                parameters: block.parameters().to_vec(),
                operations: block.operations().to_vec(),
                terminator: Some(block.terminator().clone()),
            })
            .collect();

        Self {
            key: unit.key,
            unit: unit.unit,
            source: unit.source,
            target: unit.target,
            kind: unit.kind,
            frame_descriptor: unit.frame_descriptor,
            blocks,
            operations: unit.operations.to_vec(),
            storages: unit.storages.to_vec(),
            values: unit.values.to_vec(),
        }
    }

    /// Removes a block's terminator so lowering can extend its control flow.
    pub fn take_terminator(
        &mut self,
        block: MirBlockId,
    ) -> Result<MirTerminator, MirUnitBuildError> {
        let index = self.block_index(block)?;

        self.blocks[index]
            .terminator
            .take()
            .ok_or(MirUnitBuildError::MissingTerminator(block))
    }

    /// Detaches the frame descriptor while a transformation adds suspension states.
    pub fn take_frame_descriptor(&mut self) -> Option<MirFrameDescriptor> {
        self.frame_descriptor.take()
    }

    /// Sets checked execution context for a lifecycle operation.
    /// None clears the context. Present empty requirements represent a checked empty context.
    pub fn set_cleanup_execution(
        &mut self,
        operation: MirOperationId,
        execution: Option<crate::MirFrameExecutionState>,
    ) -> Result<(), MirUnitBuildError> {
        let index = self.operation_index(operation)?;
        let existing = &mut self.operations[index];

        existing.set_cleanup_execution(execution);

        Ok(())
    }

    /// Replaces a resultless operation without changing its identity or position in its block.
    /// A newly declared result receives a fresh value identity.
    pub fn replace_effect(
        &mut self,
        operation: MirOperationId,
        kind: MirOperationKind,
        result_type: Option<TypeId>,
    ) -> Result<MirOperationCommit, MirUnitBuildError> {
        let index = self.operation_index(operation)?;
        let existing = &mut self.operations[index];

        if existing.result().is_some() {
            return Err(MirUnitBuildError::UnexpectedOperationResult(operation));
        }

        let result = match result_type {
            Some(ty) => {
                let value = MirValueId::from_slot(self.unit, compact_slot(self.values.len())?);

                // The replacement operation and its result independently retain source provenance.
                self.values.push(MirValue::new(
                    existing.source().clone(),
                    ty,
                    MirValueOrigin::Operation(operation),
                ));

                Some(value)
            }
            None => None,
        };

        // Replacing an operation preserves the original template's source correlation.
        existing.replace(kind, result);

        Ok(MirOperationCommit::new(operation, result))
    }

    /// Returns the target contract retained by the body under construction.
    pub const fn target(&self) -> &MirTargetContract {
        &self.target
    }

    /// Returns the protected frame owned by the body under construction.
    pub const fn protected_frame(&self) -> Option<bray_runtime_interface::ProtectedAsyncFrameId> {
        self.kind.protected_frame()
    }

    /// Iterates committed storage identities in allocation order.
    pub fn storage_ids(&self) -> impl ExactSizeIterator<Item = MirStorageId> + '_ {
        self.storages.iter().enumerate().map(|(index, _)| {
            // Storage insertion checks that every allocated slot fits the compact identity.
            let slot = index as u32;

            MirStorageId::from_slot(self.unit, slot)
        })
    }

    /// Returns a fresh suspension identity after all descriptor and committed suspension states.
    /// State zero remains reserved for initial frame entry.
    pub fn next_frame_state(&self) -> Result<crate::MirFrameStateId, MirUnitBuildError> {
        let descriptor_states = self
            .frame_descriptor
            .iter()
            .flat_map(|descriptor| descriptor.states())
            .map(|state| state.state());

        let next = descriptor_states
            .chain(self.suspension_states().map(|(state, _)| state))
            .map(|state| state.raw())
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(MirUnitBuildError::IdentityCapacityExceeded)?;

        Ok(crate::MirFrameStateId::new(next))
    }

    /// Enumerates committed suspension state identities and their resumption blocks.
    pub fn suspension_states(
        &self,
    ) -> impl Iterator<Item = (crate::MirFrameStateId, MirBlockId)> + '_ {
        self.blocks
            .iter()
            .filter_map(|block| match block.terminator.as_ref()?.kind() {
                MirTerminatorKind::Suspend {
                    resume_state,
                    resume,
                    ..
                } => Some((*resume_state, resume.target())),
                _ => None,
            })
    }

    /// Starts construction of a compiler-provided body with its selected execution mode.
    pub fn for_compiler_provided_callable(
        unit: MirUnitId,
        definition: bray_symbols::CallableDefinitionId,
        kind: MirUnitKind,
        target: MirTargetContract,
    ) -> Self {
        Self {
            key: MirUnitKey::CompilerProvidedCallable(definition),
            unit,
            source: MirSourceOrigin::CompilerProvidedCallable(definition),
            target,
            kind,
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
        frame: Option<bray_runtime_interface::ProtectedAsyncFrameId>,
        target: MirTargetContract,
    ) -> Self {
        Self {
            key,
            unit,
            source: MirSourceOrigin::GeneratedLifecycle(reference),
            target,
            kind: frame.map_or(MirUnitKind::Synchronous, MirUnitKind::ProtectedAsyncFrame),
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

    /// Returns the category of a block already allocated by this builder.
    pub fn block_kind(&self, block: MirBlockId) -> Result<MirBlockKind, MirUnitBuildError> {
        Ok(self.blocks[self.block_index(block)?].kind)
    }

    /// Resolves the type of an operand while its unit is still being built.
    pub fn operand_type(&self, operand: &crate::MirOperand) -> Result<TypeId, MirUnitBuildError> {
        super::model::resolve_operand_type(self.unit, &self.values, operand)
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

    /// Changes a storage allocation's role while retaining its identity, type, and provenance.
    pub fn replace_storage_kind(
        &mut self,
        storage: MirStorageId,
        kind: MirStorageKind,
    ) -> Result<(), MirUnitBuildError> {
        if storage.unit() != self.unit {
            return Err(MirUnitBuildError::ForeignStorage(storage));
        }

        let existing = storage
            .to_index()
            .and_then(|index| self.storages.get_mut(index))
            .ok_or(MirUnitBuildError::MissingStorage(storage))?;

        *existing = MirStorage::new(existing.source().clone(), kind, existing.ty());

        Ok(())
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

    /// Returns whether any completed block transfers control to the target block.
    pub fn has_incoming_edge(&self, target: MirBlockId) -> Result<bool, MirUnitBuildError> {
        self.block_index(target)?;

        Ok(self.blocks.iter().any(|block| {
            block.terminator.as_ref().is_some_and(|terminator| {
                let mut reaches_target = false;

                terminator.kind().for_each_successor(|successor| {
                    reaches_target |= successor == target;
                });

                reaches_target
            })
        }))
    }

    /// Returns whether completed control flow can reach the target from the entry block.
    pub fn is_reachable(
        &self,
        entry: MirBlockId,
        target: MirBlockId,
    ) -> Result<bool, MirUnitBuildError> {
        let entry_index = self.block_index(entry)?;
        let target_index = self.block_index(target)?;
        let mut visited = vec![false; self.blocks.len()];
        let mut pending = vec![entry_index];

        while let Some(index) = pending.pop() {
            if visited[index] {
                continue;
            }

            if index == target_index {
                return Ok(true);
            }

            visited[index] = true;

            let Some(terminator) = self.blocks[index].terminator.as_ref() else {
                continue;
            };

            let mut invalid_successor = None;

            terminator
                .kind()
                .for_each_successor(|successor| match self.block_index(successor) {
                    Ok(index) => pending.push(index),
                    Err(error) => invalid_successor = Some(error),
                });

            if let Some(error) = invalid_successor {
                return Err(error);
            }
        }

        Ok(false)
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

    fn operation_index(&self, operation: MirOperationId) -> Result<usize, MirUnitBuildError> {
        if operation.unit() != self.unit {
            return Err(MirUnitBuildError::ForeignOperation(operation));
        }

        operation
            .to_index()
            .filter(|index| *index < self.operations.len())
            .ok_or(MirUnitBuildError::MissingOperation(operation))
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
        MirTerminatorKind, MirUnitBuildError, MirUnitKind,
    };

    #[test]
    fn cleanup_execution_survives_reopening_and_effect_replacement() {
        let bound = test_bound_unit(19);
        let source = MirSourceAnchor::from(bound.key().source());
        let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);
        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let ty = crate::test_support::test_type();
        let storage = push_storage(&mut builder, source.clone(), ty);
        let place = MirPlace::new(storage, [], ty);

        let operation = builder
            .push_operation(
                entry,
                source.clone(),
                MirOperationKind::Destroy(place.clone()),
                None,
            )
            .unwrap()
            .operation();

        let execution = crate::MirFrameExecutionState::new([], [storage, storage])
            .with_affinity(bray_runtime_interface::ProtectedFrameAffinity::OriginThread);

        let foreign = crate::MirOperationId::from_slot(crate::MirUnitId::new(u32::MAX), 0);
        let missing = crate::MirOperationId::from_slot(operation.unit(), u32::MAX);

        assert_eq!(
            builder.set_cleanup_execution(foreign, None),
            Err(MirUnitBuildError::ForeignOperation(foreign))
        );

        assert_eq!(
            builder.set_cleanup_execution(missing, None),
            Err(MirUnitBuildError::MissingOperation(missing))
        );

        assert_eq!(builder.storage_ids().collect::<Vec<_>>(), [storage]);

        builder
            .set_cleanup_execution(operation, Some(execution.clone()))
            .unwrap();

        set_terminator(&mut builder, entry, source, MirTerminatorKind::Return(None));

        let original = builder.finish(entry).unwrap();
        let mut builder = MirUnitBuilder::from_unit(original.clone());

        builder
            .replace_effect(operation, MirOperationKind::Finalize(place), None)
            .unwrap();

        let updated = builder.finish(entry).unwrap();
        let effect = updated.operation(operation).unwrap();

        assert_eq!(effect.cleanup_execution(), Some(&execution));

        assert_eq!(
            effect.source(),
            original.operation(operation).unwrap().source()
        );

        assert_eq!(updated.block(entry).unwrap().operations(), [operation]);
        assert_eq!(execution.retained_storages(), [storage]);

        let mut builder = MirUnitBuilder::from_unit(updated);

        builder
            .set_cleanup_execution(operation, Some(crate::MirFrameExecutionState::new([], [])))
            .unwrap();

        let checked_empty = builder.finish(entry).unwrap();

        assert_eq!(
            checked_empty
                .operation(operation)
                .unwrap()
                .cleanup_execution(),
            Some(&crate::MirFrameExecutionState::new([], []))
        );

        let mut builder = MirUnitBuilder::from_unit(checked_empty);

        builder.set_cleanup_execution(operation, None).unwrap();

        assert_eq!(
            builder
                .finish(entry)
                .unwrap()
                .operation(operation)
                .unwrap()
                .cleanup_execution(),
            None
        );
    }

    #[test]
    fn cleanup_execution_rejects_invalid_operations_and_retained_storage() {
        for invalid_kind in [false, true] {
            let bound = test_bound_unit(19);
            let source = MirSourceAnchor::from(bound.key().source());
            let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);
            let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
            let ty = crate::test_support::test_type();
            let storage = push_storage(&mut builder, source.clone(), ty);
            let place = MirPlace::new(storage, [], ty);

            let kind = if invalid_kind {
                MirOperationKind::Store {
                    kind: crate::MirStoreKind::Initialize,
                    destination: place,
                    value: MirOperand::Immediate {
                        value: MirImmediateValue::Unit,
                        ty,
                    },
                }
            } else {
                MirOperationKind::Destroy(place)
            };

            let operation = builder
                .push_operation(entry, source.clone(), kind, None)
                .unwrap()
                .operation();

            let foreign_storage =
                crate::MirStorageId::from_slot(crate::MirUnitId::new(u32::MAX), 0);

            let retained = if invalid_kind {
                storage
            } else {
                foreign_storage
            };

            builder
                .set_cleanup_execution(
                    operation,
                    Some(crate::MirFrameExecutionState::new([], [retained])),
                )
                .unwrap();

            set_terminator(&mut builder, entry, source, MirTerminatorKind::Return(None));

            let expected = if invalid_kind {
                MirUnitBuildError::InvalidCleanupExecution(operation)
            } else {
                MirUnitBuildError::ForeignStorage(foreign_storage)
            };

            assert_eq!(builder.finish(entry), Err(expected));
        }
    }

    #[test]
    fn fresh_frame_states_include_descriptor_only_states_and_committed_suspensions() {
        let bound = test_bound_unit(19);
        let source = MirSourceAnchor::from(bound.key().source());
        let frame = ProtectedAsyncFrameId::new([7; 32]);
        let mut builder = unit_builder(&bound, MirUnitKind::ProtectedAsyncFrame(frame));
        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let resume = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let later = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let ty = crate::test_support::test_type();
        let storage = push_storage(&mut builder, source.clone(), ty);
        let abi = builder.target().runtime_abi();

        assert_eq!(builder.next_frame_state(), Ok(MirFrameStateId::new(1)));

        let descriptor = MirFrameDescriptor::try_new(
            frame,
            abi,
            ProtectedFrameAbiVersions::uniform(abi),
            ty,
            [
                MirFrameState::new(
                    MirFrameStateId::new(0),
                    entry,
                    crate::MirFrameExecutionState::new([], []),
                ),
                MirFrameState::new(
                    MirFrameStateId::new(1),
                    resume,
                    crate::MirFrameExecutionState::new([], [storage]),
                ),
                MirFrameState::new(
                    MirFrameStateId::new(2),
                    later,
                    crate::MirFrameExecutionState::new([], [storage]),
                ),
            ],
        )
        .unwrap();

        builder.set_frame_descriptor(descriptor.clone()).unwrap();

        assert_eq!(builder.suspension_states().count(), 0);
        assert_eq!(builder.next_frame_state(), Ok(MirFrameStateId::new(3)));

        for (state, expected) in [
            (1, Ok(MirFrameStateId::new(3))),
            (3, Ok(MirFrameStateId::new(4))),
            (u32::MAX, Err(MirUnitBuildError::IdentityCapacityExceeded)),
        ] {
            builder
                .set_terminator(
                    entry,
                    source.clone(),
                    MirTerminatorKind::Suspend {
                        kind: crate::MirSuspensionKind::Awaited,
                        payload: None,
                        resume_state: MirFrameStateId::new(state),
                        resume: MirEdge::new(resume, []),
                        cancellation: None,
                        registration: MirRuntimeReference::new(
                            RuntimeAbiRole::SuspensionRegistration,
                            abi,
                        ),
                        wake: MirRuntimeReference::new(RuntimeAbiRole::Wake, abi),
                    },
                )
                .unwrap();

            assert_eq!(builder.next_frame_state(), expected);

            builder.take_terminator(entry).unwrap();
        }

        assert_eq!(builder.take_frame_descriptor(), Some(descriptor));
    }

    #[test]
    fn reopening_templates_preserves_identities_and_validates_replacements() {
        let bound = test_bound_unit(19);
        let source = MirSourceAnchor::from(bound.key().source());

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::Synchronous,
            crate::test_support::test_target(),
        );

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap();

        let ty = crate::test_support::test_type();

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, ty)
            .unwrap();

        let place = MirPlace::new(storage, [], ty);

        let effect = builder
            .push_operation(
                entry,
                source.clone(),
                MirOperationKind::Destroy(place.clone()),
                None,
            )
            .unwrap();

        builder
            .set_terminator(entry, source.clone(), MirTerminatorKind::Return(None))
            .unwrap();

        let original = builder.finish(entry).unwrap();
        let mut builder = MirUnitBuilder::from_unit(original.clone());
        let terminator = builder.take_terminator(entry).unwrap();

        assert_eq!(
            builder.take_terminator(entry),
            Err(MirUnitBuildError::MissingTerminator(entry))
        );

        assert!(builder.take_frame_descriptor().is_none());

        let replacement = builder
            .replace_effect(
                effect.operation(),
                MirOperationKind::Borrow {
                    kind: BorrowKind::Mutable,
                    place,
                },
                Some(ty),
            )
            .unwrap();

        assert_eq!(replacement.operation(), effect.operation());
        assert!(replacement.result().is_some());

        assert_eq!(
            builder.replace_effect(
                effect.operation(),
                MirOperationKind::Destroy(MirPlace::new(storage, [], ty)),
                None
            ),
            Err(MirUnitBuildError::UnexpectedOperationResult(
                effect.operation()
            ))
        );

        builder
            .set_terminator(entry, source, terminator.kind().clone())
            .unwrap();

        let rewritten = builder.finish(entry).unwrap();

        assert_eq!(rewritten.entry(), original.entry());
        assert_eq!(rewritten.key(), original.key());
        assert_eq!(rewritten.storages(), original.storages());

        assert_eq!(
            rewritten.block(entry).unwrap().operations(),
            original.block(entry).unwrap().operations()
        );

        assert!(matches!(
            original.operation(effect.operation()).unwrap().kind(),
            MirOperationKind::Destroy(_)
        ));

        assert!(matches!(
            rewritten.operation(effect.operation()).unwrap().kind(),
            MirOperationKind::Borrow { .. }
        ));
    }

    #[test]
    fn destructor_remainders_accept_only_receiver_destruction_roles() {
        use crate::{MirAbandonmentAction, MirGeneratedLifecycleRole};

        for role in [
            MirGeneratedLifecycleRole::Finalize,
            MirGeneratedLifecycleRole::StaticFinalize,
            MirGeneratedLifecycleRole::Destroy,
            MirGeneratedLifecycleRole::Cleanup(MirCleanupPhase::TaskCancellation),
            MirGeneratedLifecycleRole::Cleanup(MirCleanupPhase::LifecycleResolution),
            MirGeneratedLifecycleRole::Abandon(MirAbandonmentAction::Quiesce),
            MirGeneratedLifecycleRole::Abandon(MirAbandonmentAction::Destroy),
            MirGeneratedLifecycleRole::Abandon(MirAbandonmentAction::Destructor),
        ] {
            let bound = test_bound_unit(20);
            let source = MirSourceAnchor::from(bound.key().source());
            let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);

            let entry = builder
                .push_block(source.clone(), MirBlockKind::Ordinary)
                .unwrap();

            let broadcast = builder
                .push_block(source.clone(), MirBlockKind::CleanupBroadcast)
                .unwrap();

            let cleanup = builder
                .push_block(source.clone(), MirBlockKind::LifecycleResolution)
                .unwrap();

            let ty = crate::test_support::test_type();

            let storage = builder
                .push_storage(source.clone(), MirStorageKind::Temporary, ty)
                .unwrap();

            let operation = builder
                .push_operation(
                    cleanup,
                    source.clone(),
                    MirOperationKind::DestructorRemainder {
                        role,
                        place: MirPlace::new(storage, [], ty),
                    },
                    None,
                )
                .unwrap();

            builder
                .set_terminator(
                    entry,
                    source.clone(),
                    MirTerminatorKind::BeginCleanup(MirCleanupEdge::new(
                        MirCleanupPhase::TaskCancellation,
                        MirEdge::new(broadcast, []),
                    )),
                )
                .unwrap();

            builder
                .set_terminator(
                    broadcast,
                    source.clone(),
                    MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                        MirCleanupPhase::LifecycleResolution,
                        MirEdge::new(cleanup, []),
                    )),
                )
                .unwrap();

            builder
                .set_terminator(cleanup, source, MirTerminatorKind::Return(None))
                .unwrap();

            let result = builder.finish(entry);

            if matches!(
                role,
                MirGeneratedLifecycleRole::Destroy
                    | MirGeneratedLifecycleRole::Cleanup(MirCleanupPhase::LifecycleResolution)
            ) {
                assert!(result.is_ok(), "{role:?}: {result:?}");
            } else {
                assert_eq!(
                    result.unwrap_err(),
                    MirUnitBuildError::InvalidDestructorRemainder(operation.operation())
                );
            }
        }
    }

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
            crate::MirUnitKind::Synchronous,
            crate::test_support::test_target(),
        );

        let bound = test_bound_unit(4);

        for foreign in [
            MirSourceAnchor::CompilerProvidedCallable(definition(4)),
            MirSourceAnchor::from(bound.key().source()),
        ] {
            assert_eq!(
                builder.push_block(foreign, MirBlockKind::Ordinary),
                Err(MirUnitBuildError::SourceOriginMismatch)
            );
        }

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap();

        builder
            .set_terminator(entry, source, MirTerminatorKind::Return(None))
            .unwrap();

        let unit = builder.finish(entry).unwrap();

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
    fn storage_role_replacement_preserves_identity_and_rejects_invalid_storage() {
        let bound = test_bound_unit(4);
        let source = MirSourceAnchor::from(bound.key().source());
        let ty = crate::test_support::test_type();
        let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);
        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Parameter(0), ty)
            .unwrap();

        builder
            .replace_storage_kind(storage, MirStorageKind::Temporary)
            .unwrap();

        let missing = crate::MirStorageId::from_slot(storage.unit(), 9);
        let foreign = crate::MirStorageId::from_slot(crate::MirUnitId::new(99), 0);

        assert_eq!(
            builder.replace_storage_kind(missing, MirStorageKind::Local),
            Err(MirUnitBuildError::MissingStorage(missing))
        );

        assert_eq!(
            builder.replace_storage_kind(foreign, MirStorageKind::Local),
            Err(MirUnitBuildError::ForeignStorage(foreign))
        );

        builder
            .set_terminator(entry, source.clone(), MirTerminatorKind::Return(None))
            .unwrap();

        let unit = builder.finish(entry).unwrap();
        let stored = unit.storage(storage).unwrap();

        assert_eq!(stored.kind(), &MirStorageKind::Temporary);
        assert_eq!(stored.source(), &source);
        assert_eq!(stored.ty(), ty);
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
        assert_eq!(builder.block_kind(entry), Ok(MirBlockKind::Ordinary));
        assert_eq!(builder.operand_type(&MirOperand::Value(value)), Ok(ty));
        let foreign = crate::MirValueId::from_slot(crate::MirUnitId::new(97), 0);
        let missing = crate::MirValueId::from_slot(entry.unit(), 10);

        assert_eq!(
            builder.operand_type(&MirOperand::Value(foreign)),
            Err(MirUnitBuildError::ForeignValue(foreign))
        );

        assert_eq!(
            builder.operand_type(&MirOperand::Value(missing)),
            Err(MirUnitBuildError::MissingValue(missing))
        );

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
            assert_eq!(builder.operand_type(&operand), Ok(ty));
        }
    }

    #[test]
    fn builders_report_completed_incoming_edges() {
        let bound = test_bound_unit(28);
        let source = MirSourceAnchor::from(bound.key().source());

        let mut builder = unit_builder(&bound, MirUnitKind::Synchronous);

        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let join = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);

        assert_eq!(builder.has_incoming_edge(join), Ok(false));

        set_terminator(
            &mut builder,
            entry,
            source,
            MirTerminatorKind::Goto(MirEdge::new(join, [])),
        );

        assert_eq!(builder.has_incoming_edge(join), Ok(true));
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

        assert_eq!(builder.has_incoming_edge(join), Ok(true));
        assert_eq!(builder.is_reachable(entry, join), Ok(false));
        assert_eq!(builder.is_reachable(disconnected, join), Ok(true));
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
            MirTerminatorKind::Return(Some(MirOperand::Value(foreign))),
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

            let operation = match builder.push_operation(
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
                assert!(builder.finish(entry).is_ok());
            } else {
                assert_eq!(
                    builder.finish(entry),
                    Err(MirUnitBuildError::InvalidAggregateOperation(operation))
                );
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

        let operation = match builder.push_operation(
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

        assert_eq!(
            builder.finish(entry),
            Err(MirUnitBuildError::InvalidMemoryOperation(operation))
        );
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

        let operation = match builder.push_operation(
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

        assert_eq!(
            builder.finish(entry),
            Err(MirUnitBuildError::InvalidMemoryOperation(operation))
        );
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

        let operation = match builder.push_operation(
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

        assert_eq!(
            builder.finish(entry),
            Err(MirUnitBuildError::InvalidMemoryOperation(operation))
        );
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

        assert_eq!(
            builder.finish(entry),
            Err(MirUnitBuildError::StorageTypeMismatch(storage))
        );
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
                assert_eq!(unit, Err(MirUnitBuildError::InvalidCallPanicCheck(entry)));
            } else if cancellation_kind != MirBlockKind::Ordinary {
                assert_eq!(
                    unit,
                    Err(MirUnitBuildError::CleanupPhaseOrderViolation(cancelled))
                );
            } else {
                let unit = unit.unwrap();
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
    fn generated_lifecycle_provenance_is_independent_of_frame_execution() {
        let ty = crate::test_support::test_type();
        let reference = crate::MirHelperReference::Finalize(ty);

        let key = crate::MirUnitKey::GeneratedLifecycle(crate::MirGeneratedLifecycleKey::new(
            crate::MirGeneratedLifecycleRole::Finalize,
            [8; 32],
        ));

        let frame = ProtectedAsyncFrameId::new([9; 32]);

        for execution in [None, Some(frame)] {
            for attach_descriptor in [false, true] {
                let source = MirSourceAnchor::generated_lifecycle(reference.clone());

                let mut builder = MirUnitBuilder::for_generated_lifecycle(
                    crate::MirUnitId::new(10),
                    key.clone(),
                    reference.clone(),
                    execution,
                    crate::test_support::test_target(),
                );

                let entry = builder
                    .push_block(source.clone(), MirBlockKind::Ordinary)
                    .unwrap();

                builder
                    .set_terminator(entry, source, MirTerminatorKind::Return(None))
                    .unwrap();

                if attach_descriptor {
                    let result = builder.set_frame_descriptor(frame_descriptor(frame, entry, ty));

                    if execution.is_none() {
                        assert_eq!(result, Err(MirUnitBuildError::UnexpectedFrameDescriptor));

                        continue;
                    }

                    result.unwrap();
                }

                let result = builder.finish(entry);

                if execution.is_some() && !attach_descriptor {
                    assert_eq!(result, Err(MirUnitBuildError::MissingFrameDescriptor));

                    continue;
                }

                let unit = result.unwrap();

                assert_eq!(unit.key(), &key);

                assert_eq!(
                    unit.source(),
                    &crate::MirSourceOrigin::GeneratedLifecycle(reference.clone())
                );

                assert_eq!(unit.kind().protected_frame(), execution);
            }
        }
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

    #[test]
    fn shielded_suspension_preserves_the_lifecycle_phase() {
        let kinds = [
            MirBlockKind::Ordinary,
            MirBlockKind::CleanupBroadcast,
            MirBlockKind::LifecycleResolution,
        ];

        for current_kind in kinds {
            for resume_kind in kinds {
                let bound = test_bound_unit(11);
                let source = MirSourceAnchor::from(bound.key().source());
                let frame = ProtectedAsyncFrameId::new([11; 32]);
                let ty = crate::test_support::test_type();
                let abi = RuntimeAbiVersion::new(1, 0);
                let mut builder = unit_builder(&bound, MirUnitKind::ProtectedAsyncFrame(frame));
                let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
                let current = push_block(&mut builder, source.clone(), current_kind);
                let resume = push_block(&mut builder, source.clone(), resume_kind);

                let entry_edge = match current_kind {
                    MirBlockKind::Ordinary => MirTerminatorKind::Goto(MirEdge::new(current, [])),
                    MirBlockKind::CleanupBroadcast => {
                        MirTerminatorKind::BeginCleanup(MirCleanupEdge::new(
                            MirCleanupPhase::TaskCancellation,
                            MirEdge::new(current, []),
                        ))
                    }
                    MirBlockKind::LifecycleResolution => {
                        let broadcast = push_block(
                            &mut builder,
                            source.clone(),
                            MirBlockKind::CleanupBroadcast,
                        );

                        set_terminator(
                            &mut builder,
                            broadcast,
                            source.clone(),
                            MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                                MirCleanupPhase::LifecycleResolution,
                                MirEdge::new(current, []),
                            )),
                        );

                        MirTerminatorKind::BeginCleanup(MirCleanupEdge::new(
                            MirCleanupPhase::TaskCancellation,
                            MirEdge::new(broadcast, []),
                        ))
                    }
                };

                set_terminator(&mut builder, entry, source.clone(), entry_edge);

                set_terminator(
                    &mut builder,
                    current,
                    source.clone(),
                    MirTerminatorKind::Suspend {
                        kind: crate::MirSuspensionKind::Awaited,
                        payload: None,
                        resume_state: MirFrameStateId::new(1),
                        resume: MirEdge::new(resume, []),
                        cancellation: None,
                        registration: MirRuntimeReference::new(
                            RuntimeAbiRole::SuspensionRegistration,
                            abi,
                        ),
                        wake: MirRuntimeReference::new(RuntimeAbiRole::Wake, abi),
                    },
                );

                set_terminator(
                    &mut builder,
                    resume,
                    source,
                    MirTerminatorKind::Return(None),
                );

                builder
                    .set_frame_descriptor(
                        MirFrameDescriptor::try_new(
                            frame,
                            abi,
                            ProtectedFrameAbiVersions::uniform(abi),
                            ty,
                            [
                                MirFrameState::new(
                                    MirFrameStateId::new(0),
                                    entry,
                                    crate::MirFrameExecutionState::new([], []),
                                ),
                                MirFrameState::new(
                                    MirFrameStateId::new(1),
                                    resume,
                                    crate::MirFrameExecutionState::new([], []),
                                ),
                            ],
                        )
                        .unwrap(),
                    )
                    .unwrap();

                let result = builder.finish(entry);

                if current_kind != MirBlockKind::LifecycleResolution {
                    assert_eq!(
                        result,
                        Err(MirUnitBuildError::CleanupPhaseOrderViolation(current))
                    );
                } else if resume_kind != current_kind {
                    assert_eq!(
                        result,
                        Err(MirUnitBuildError::CleanupPhaseOrderViolation(resume))
                    );
                } else {
                    let unit = result.unwrap();
                    let mut successors = Vec::new();

                    unit.block(current)
                        .unwrap()
                        .terminator()
                        .kind()
                        .for_each_successor(|block| successors.push(block));

                    assert_eq!(successors, [resume]);
                }
            }
        }
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
        let state = MirFrameState::new(
            MirFrameStateId::new(0),
            entry,
            crate::MirFrameExecutionState::new([], []),
        );

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
