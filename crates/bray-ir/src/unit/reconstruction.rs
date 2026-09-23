use std::collections::{BTreeMap, BTreeSet};

use crate::{
    MirBlockId, MirCapacityError, MirFrameDescriptor, MirOperationId, MirStorageId,
    MirTerminatorKind, MirUnit, MirUnitBuilder, MirValueId, MirValueOrigin,
};

use super::local_id_remap::{MirLocalIdMapping, remap_operation, remap_terminator};
use crate::walk::{for_each_operation_storage, for_each_terminator_storage};

/// Explicit old-to-new identities produced by immutable MIR reconstruction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirReconstructionMappings {
    unit: crate::MirUnitId,
    blocks: Vec<Option<MirBlockId>>,
    operations: Vec<Option<MirOperationId>>,
    storages: Vec<Option<MirStorageId>>,
    values: Vec<Option<MirValueId>>,
}

impl MirReconstructionMappings {
    /// Returns the rebuilt identity of a surviving block.
    pub fn block(&self, old: MirBlockId) -> Option<MirBlockId> {
        self.local_mapping(old.unit(), old.to_index(), &self.blocks)
    }

    /// Returns the rebuilt identity of a surviving operation.
    pub fn operation(&self, old: MirOperationId) -> Option<MirOperationId> {
        self.local_mapping(old.unit(), old.to_index(), &self.operations)
    }

    /// Returns the rebuilt identity of a surviving storage allocation.
    pub fn storage(&self, old: MirStorageId) -> Option<MirStorageId> {
        self.local_mapping(old.unit(), old.to_index(), &self.storages)
    }

    /// Returns the rebuilt identity of a surviving value.
    pub fn value(&self, old: MirValueId) -> Option<MirValueId> {
        self.local_mapping(old.unit(), old.to_index(), &self.values)
    }

    fn local_mapping<T: Copy>(
        &self,
        unit: crate::MirUnitId,
        index: Option<usize>,
        mappings: &[Option<T>],
    ) -> Option<T> {
        if unit != self.unit {
            return None;
        }

        mappings.get(index?).copied().flatten()
    }
}

impl MirLocalIdMapping for MirReconstructionMappings {
    fn block(&self, old: MirBlockId) -> MirBlockId {
        MirReconstructionMappings::block(self, old)
            .expect("surviving MIR must not reference a removed block")
    }

    fn storage(&self, old: MirStorageId) -> MirStorageId {
        MirReconstructionMappings::storage(self, old)
            .expect("surviving MIR must not reference removed storage")
    }

    fn value(&self, old: MirValueId) -> MirValueId {
        MirReconstructionMappings::value(self, old)
            .expect("surviving MIR must not reference a removed value")
    }
}

/// Rebuilds one unit from its entry and externally resumable frame entries.
///
/// Reconstruction preserves every operation in a surviving block. It does not decide whether an
/// operation is pure or whether an ownership or cleanup effect can be removed.
pub fn reconstruct_reachable(
    unit: &MirUnit,
) -> Result<(MirUnit, MirReconstructionMappings), MirCapacityError> {
    reconstruct_with_edits(unit, &BTreeMap::new(), &BTreeSet::new())
}

/// Rebuilds a unit after replacing terminators and omitting proven dead operations.
///
/// Callers must preserve effects and ensure no surviving operand refers to an omitted result.
/// Replacement terminators must refer to existing blocks and satisfy their checked contracts.
pub fn reconstruct_with_edits(
    unit: &MirUnit,
    terminators: &BTreeMap<MirBlockId, MirTerminatorKind>,
    omitted_operations: &BTreeSet<MirOperationId>,
) -> Result<(MirUnit, MirReconstructionMappings), MirCapacityError> {
    assert!(unit.is_valid(), "MIR reconstruction requires a valid unit");

    let retained_blocks = retained_blocks(unit, terminators);
    let operation_owners = operation_owners(unit);

    let retained_operations =
        retained_operations(unit, &retained_blocks, &operation_owners, omitted_operations);

    let retained_values = retained_values(unit, &retained_blocks, &retained_operations);

    let retained_storages = retained_storages(
        unit,
        &retained_blocks,
        &retained_operations,
        terminators,
    );

    let mappings = build_mappings(
        unit,
        &retained_blocks,
        &retained_operations,
        &retained_storages,
        &retained_values,
    );

    let reconstructed = rebuild_unit(unit, &mappings, &operation_owners, terminators)?;

    assert!(
        reconstructed.is_valid(),
        "MIR reconstruction must produce a valid unit"
    );

    Ok((reconstructed, mappings))
}

fn retained_blocks(
    unit: &MirUnit,
    terminators: &BTreeMap<MirBlockId, MirTerminatorKind>,
) -> Vec<bool> {
    let mut retained = vec![false; unit.blocks().len()];
    let mut pending = vec![unit.entry()];

    if let Some(descriptor) = unit.frame_descriptor() {
        pending.extend(descriptor.states().iter().map(crate::MirFrameState::entry));
    }

    while let Some(block) = pending.pop() {
        let index = local_index(unit, block.unit(), block.to_index(), retained.len(), "block");

        if retained[index] {
            continue;
        }

        retained[index] = true;

        let terminator = terminators.get(&block).unwrap_or_else(|| {
            unit.block(block)
                .expect("reachable MIR block must resolve")
                .terminator()
                .kind()
        });

        terminator.for_each_successor(|successor| pending.push(successor));
    }

    retained
}

fn retained_operations(
    unit: &MirUnit,
    blocks: &[bool],
    owners: &[MirBlockId],
    omitted: &BTreeSet<MirOperationId>,
) -> Vec<bool> {
    owners
        .iter()
        .enumerate()
        .map(|(slot, owner)| {
            let index = local_index(unit, owner.unit(), owner.to_index(), blocks.len(), "block");

            blocks[index] && !omitted.contains(&operation_id(unit, slot))
        })
        .collect()
}

fn retained_values(unit: &MirUnit, blocks: &[bool], operations: &[bool]) -> Vec<bool> {
    unit.values()
        .iter()
        .map(|value| match value.origin() {
            MirValueOrigin::BlockParameter(block) => {
                let index = local_index(unit, block.unit(), block.to_index(), blocks.len(), "block");

                blocks[index]
            }
            MirValueOrigin::Operation(operation) => {
                let index = local_index(
                    unit,
                    operation.unit(),
                    operation.to_index(),
                    operations.len(),
                    "operation",
                );

                operations[index]
            }
        })
        .collect()
}

fn retained_storages(
    unit: &MirUnit,
    blocks: &[bool],
    operations: &[bool],
    terminators: &BTreeMap<MirBlockId, MirTerminatorKind>,
) -> Vec<bool> {
    let mut retained = vec![false; unit.storages().len()];

    for (block_index, block) in unit.blocks().iter().enumerate() {
        if !blocks[block_index] {
            continue;
        }

        for operation in block.operations() {
            if !operations[local_index(unit, operation.unit(), operation.to_index(), operations.len(), "operation")] {
                continue;
            }

            let kind = unit
                .operation(*operation)
                .expect("surviving MIR operation must resolve")
                .kind();

            for_each_operation_storage(kind, |storage| {
                retain_storage(unit, &mut retained, storage);
            });
        }

        let block_id = MirBlockId::from_slot(unit.unit(), u32::try_from(block_index).expect("validated MIR block slot must fit its identity"));

        let terminator = terminators
            .get(&block_id)
            .unwrap_or_else(|| block.terminator().kind());

        for_each_terminator_storage(terminator, |storage| {
            retain_storage(unit, &mut retained, storage);
        });
    }

    if let Some(descriptor) = unit.frame_descriptor() {
        for state in descriptor.states() {
            for storage in state.initialized_storages() {
                retain_storage(unit, &mut retained, *storage);
            }
        }
    }

    retained
}

fn retain_storage(unit: &MirUnit, retained: &mut [bool], storage: MirStorageId) {
    let index = local_index(
        unit,
        storage.unit(),
        storage.to_index(),
        retained.len(),
        "storage",
    );

    retained[index] = true;
}

fn build_mappings(
    unit: &MirUnit,
    blocks: &[bool],
    operations: &[bool],
    storages: &[bool],
    values: &[bool],
) -> MirReconstructionMappings {
    MirReconstructionMappings {
        unit: unit.unit(),
        blocks: compact_mapping(blocks, |slot| MirBlockId::from_slot(unit.unit(), slot)),
        operations: compact_mapping(operations, |slot| {
            MirOperationId::from_slot(unit.unit(), slot)
        }),
        storages: compact_mapping(storages, |slot| {
            MirStorageId::from_slot(unit.unit(), slot)
        }),
        values: compact_mapping(values, |slot| MirValueId::from_slot(unit.unit(), slot)),
    }
}

fn compact_mapping<T>(retained: &[bool], from_slot: impl Fn(u32) -> T) -> Vec<Option<T>> {
    let mut next = 0_u32;

    retained
        .iter()
        .map(|retained| {
            if !retained {
                return None;
            }

            let mapped = from_slot(next);

            next = next
                .checked_add(1)
                .expect("validated MIR table size must fit local identities");

            Some(mapped)
        })
        .collect()
}

fn rebuild_unit(
    unit: &MirUnit,
    mappings: &MirReconstructionMappings,
    operation_owners: &[MirBlockId],
    terminators: &BTreeMap<MirBlockId, MirTerminatorKind>,
) -> Result<MirUnit, MirCapacityError> {
    let mut builder = MirUnitBuilder::for_reconstruction(unit);

    // Every rebuilt node owns its source anchor independently of the input snapshot.
    for (old, block) in unit.blocks_with_ids() {
        let Some(expected) = mappings.block(old) else {
            continue;
        };

        let actual = builder.push_block(block.source().clone(), block.kind())?;

        assert_eq!(actual, expected, "MIR block reconstruction order changed");
    }

    for (old, storage) in unit.storages_with_ids() {
        let Some(expected) = mappings.storage(old) else {
            continue;
        };

        let actual = builder.push_storage(
            storage.source().clone(),
            storage.kind().clone(),
            storage.ty(),
        )?;

        assert_eq!(actual, expected, "MIR storage reconstruction order changed");
    }

    let mut next_operation = 0;

    for (index, value) in unit.values().iter().enumerate() {
        let old = MirValueId::from_slot(
            unit.unit(),
            u32::try_from(index).expect("validated MIR value slot must fit its identity"),
        );

        let Some(expected) = mappings.value(old) else {
            continue;
        };

        match value.origin() {
            MirValueOrigin::BlockParameter(block) => {
                let actual = builder.push_block_parameter(
                    MirLocalIdMapping::block(mappings, block),
                    value.source().clone(),
                    value.ty(),
                )?;

                assert_eq!(actual, expected, "MIR value reconstruction order changed");
            }
            MirValueOrigin::Operation(operation) => {
                let stop = operation
                    .to_index()
                    .expect("validated MIR operation must have a local slot");

                push_operations_before(
                    unit,
                    mappings,
                    operation_owners,
                    &mut builder,
                    &mut next_operation,
                    stop,
                )?;

                let commit = push_operation(
                    unit,
                    mappings,
                    operation_owners,
                    &mut builder,
                    stop,
                    Some(value.ty()),
                )?
                .expect("operation defining a surviving MIR value must be retained");

                assert_eq!(
                    commit.result(),
                    Some(expected),
                    "MIR operation result reconstruction order changed"
                );

                next_operation = stop + 1;
            }
        }
    }

    push_operations_before(
        unit,
        mappings,
        operation_owners,
        &mut builder,
        &mut next_operation,
        unit.operations().len(),
    )?;

    for (old, block) in unit.blocks_with_ids() {
        let Some(mapped) = mappings.block(old) else {
            continue;
        };

        let mut kind = terminators
            .get(&old)
            .unwrap_or_else(|| block.terminator().kind())
            .clone();

        remap_terminator(&mut kind, mappings);

        builder.set_terminator(mapped, block.terminator().source().clone(), kind);
    }

    if let Some(descriptor) = unit.frame_descriptor() {
        // The output descriptor owns its state table independently of the input snapshot.
        let mut descriptor: MirFrameDescriptor = descriptor.clone();
        descriptor.remap_local_ids(mappings);
        builder.set_frame_descriptor(descriptor);
    }

    Ok(builder.finish(MirLocalIdMapping::block(mappings, unit.entry())))
}

fn operation_owners(unit: &MirUnit) -> Vec<MirBlockId> {
    let mut owners = vec![None; unit.operations().len()];

    for (block, body) in unit.blocks_with_ids() {
        for operation in body.operations() {
            let index = local_index(
                unit,
                operation.unit(),
                operation.to_index(),
                owners.len(),
                "operation",
            );

            assert!(
                owners[index].replace(block).is_none(),
                "MIR operation must belong to exactly one block"
            );
        }
    }

    owners
        .into_iter()
        .map(|owner| owner.expect("every MIR operation must belong to a block"))
        .collect()
}

fn push_operations_before(
    unit: &MirUnit,
    mappings: &MirReconstructionMappings,
    owners: &[MirBlockId],
    builder: &mut MirUnitBuilder,
    next: &mut usize,
    stop: usize,
) -> Result<(), MirCapacityError> {
    while *next < stop {
        let operation = &unit.operations()[*next];

        if operation.result().is_some() && mappings.operation(operation_id(unit, *next)).is_some() {
            panic!("MIR operation result must be reconstructed at its value slot");
        }

        let _ = push_operation(unit, mappings, owners, builder, *next, None)?;
        *next += 1;
    }

    Ok(())
}

fn push_operation(
    unit: &MirUnit,
    mappings: &MirReconstructionMappings,
    owners: &[MirBlockId],
    builder: &mut MirUnitBuilder,
    index: usize,
    result_type: Option<bray_symbols::TypeId>,
) -> Result<Option<crate::MirOperationCommit>, MirCapacityError> {
    let old = operation_id(unit, index);

    let Some(expected) = mappings.operation(old) else {
        return Ok(None);
    };

    let operation = &unit.operations()[index];
    let mut kind = operation.kind().clone();
    remap_operation(&mut kind, mappings);

    let owner = owners[index];

    let commit = builder.push_operation(
        MirLocalIdMapping::block(mappings, owner),
        operation.source().clone(),
        kind,
        result_type,
    )?;

    assert_eq!(
        commit.operation(),
        expected,
        "MIR operation reconstruction order changed"
    );

    Ok(Some(commit))
}

fn operation_id(unit: &MirUnit, index: usize) -> MirOperationId {
    MirOperationId::from_slot(
        unit.unit(),
        u32::try_from(index).expect("validated MIR operation slot must fit its identity"),
    )
}

fn local_index(
    unit: &MirUnit,
    owner: crate::MirUnitId,
    index: Option<usize>,
    length: usize,
    kind: &str,
) -> usize {
    assert_eq!(owner, unit.unit(), "MIR {kind} belongs to another unit");

    let index = index.unwrap_or_else(|| panic!("MIR {kind} has no valid slot"));

    assert!(index < length, "MIR {kind} was not allocated");

    index
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use bray_bound_tree::{
        InlineAssemblyContract, InlineAssemblyOperand, InlineAssemblyOperandKind,
        MAX_INLINE_ASSEMBLY_OPERANDS,
    };
    use bray_runtime_interface::{
        ProtectedAsyncFrameId, ProtectedFrameAbiVersions, RuntimeAbiRole, RuntimeAbiVersion,
    };
    use bray_testing::test_bound_unit;

    use super::{reconstruct_reachable, reconstruct_with_edits};
    use crate::{
        MirAsyncOperation, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge,
        MirFrameDescriptor, MirFrameState, MirFrameStateId, MirImmediateValue,
        MirInlineAssemblyTerminator, MirOperand, MirOperationKind, MirPlace, MirRuntimeReference,
        MirStorageKind, MirStoreKind, MirSuspensionKind, MirTerminatorKind, MirUnit,
        MirUnitBuilder, MirUnitKind,
    };

    #[test]
    fn unchanged_reconstruction_preserves_loops_joins_and_source_mapping() {
        let (unit, _) = ordinary_unit(false);

        let (reconstructed, mappings) = reconstruct_reachable(&unit)
            .unwrap_or_else(|error| panic!("test MIR must reconstruct: {error:?}"));

        assert_eq!(reconstructed, unit);

        for (block, _) in unit.blocks_with_ids() {
            assert_eq!(mappings.block(block), Some(block));
        }

        for (operation, _) in unit.operations_with_ids() {
            assert_eq!(mappings.operation(operation), Some(operation));
        }

        for (storage, _) in unit.storages_with_ids() {
            assert_eq!(mappings.storage(storage), Some(storage));
        }
    }

    #[test]
    fn reconstruction_removes_unreachable_blocks_and_compacts_local_tables() {
        let (unit, dead) = ordinary_unit(true);

        let dead = dead.expect("test unit must contain an unreachable branch");

        let old_live_storage = unit
            .storages_with_ids()
            .nth(1)
            .expect("test unit must contain live storage after dead storage")
            .0;

        let old_live_operation = unit
            .operations_with_ids()
            .nth(1)
            .expect("test unit must contain a live operation after a dead operation")
            .0;

        let (reconstructed, mappings) = reconstruct_reachable(&unit)
            .unwrap_or_else(|error| panic!("test MIR must reconstruct: {error:?}"));

        assert!(reconstructed.is_valid());
        assert_eq!(reconstructed.blocks().len(), unit.blocks().len() - 1);
        assert_eq!(reconstructed.operations().len(), 1);
        assert_eq!(reconstructed.storages().len(), 1);
        assert_eq!(mappings.block(dead), None);

        assert_eq!(
            mappings.storage(old_live_storage),
            reconstructed.storages_with_ids().next().map(|pair| pair.0)
        );

        assert_eq!(
            mappings.operation(old_live_operation),
            reconstructed
                .operations_with_ids()
                .next()
                .map(|pair| pair.0)
        );
    }

    #[test]
    fn edited_branch_removes_the_unselected_loop_body() {
        let (unit, _) = ordinary_unit(false);

        let blocks = unit.blocks_with_ids().map(|(id, _)| id).collect::<Vec<_>>();

        let [_, header, body, exit] = blocks.as_slice() else {
            panic!("test MIR must have four blocks");
        };

        let header_value = unit.block(*header).expect("header must exist").parameters()[0];

        let terminators = BTreeMap::from([(
            *header,
            MirTerminatorKind::Goto(MirEdge::new(*exit, [MirOperand::Value(header_value)])),
        )]);

        let (rewritten, mappings) = reconstruct_with_edits(&unit, &terminators, &BTreeSet::new())
            .expect("edited MIR must reconstruct");

        assert!(rewritten.is_valid());
        assert_eq!(rewritten.blocks().len(), 3);
        assert!(rewritten.operations().is_empty());
        assert!(rewritten.storages().is_empty());
        assert_eq!(mappings.block(*body), None);
    }

    #[test]
    fn omitted_operation_removes_its_unused_storage() {
        let (unit, _) = ordinary_unit(false);

        let operation = unit.operations_with_ids().next().expect("body has a store").0;
        let storage = unit.storages_with_ids().next().expect("body has storage").0;

        let (rewritten, mappings) = reconstruct_with_edits(
            &unit,
            &BTreeMap::new(),
            &BTreeSet::from([operation]),
        )
        .expect("MIR without the operation must reconstruct");

        assert!(rewritten.is_valid());
        assert!(rewritten.operations().is_empty());
        assert!(rewritten.storages().is_empty());
        assert_eq!(mappings.operation(operation), None);
        assert_eq!(mappings.storage(storage), None);
    }

    #[test]
    fn reconstruction_preserves_panic_and_cleanup_successors() {
        let bound = test_bound_unit(42);
        let source = crate::MirSourceAnchor::from(bound.key().source());
        let ty = crate::test_support::test_type();

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::Synchronous,
            crate::test_support::test_target(),
        );

        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);

        let cancellation = push_block(
            &mut builder,
            source.clone(),
            MirBlockKind::CleanupBroadcast,
        );

        let lifecycle = push_block(
            &mut builder,
            source.clone(),
            MirBlockKind::LifecycleResolution,
        );

        builder.set_terminator(
            entry,
            source.clone(),
            MirTerminatorKind::Panic {
                report: immediate(ty),
                cleanup: MirCleanupEdge::new(
                    MirCleanupPhase::TaskCancellation,
                    MirEdge::new(cancellation, []),
                ),
            },
        );

        builder.set_terminator(
            cancellation,
            source.clone(),
            MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                MirCleanupPhase::LifecycleResolution,
                MirEdge::new(lifecycle, []),
            )),
        );

        builder.set_terminator(lifecycle, source, MirTerminatorKind::Return(None));

        assert_round_trip(builder.finish(entry));
    }

    #[test]
    fn reconstruction_preserves_inline_assembly_targets() {
        let bound = test_bound_unit(43);
        let source = crate::MirSourceAnchor::from(bound.key().source());
        let ty = crate::test_support::test_type();
        let constant = crate::test_support::test_constant_value();

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::Synchronous,
            crate::test_support::test_target(),
        );

        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let normal = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let alternate = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);

        let output = builder
            .push_block_parameter(normal, source.clone(), ty)
            .unwrap_or_else(|error| panic!("assembly output must be valid: {error:?}"));

        let mut operands = [None; MAX_INLINE_ASSEMBLY_OPERANDS];

        operands[0] = Some(InlineAssemblyOperand::new(
            InlineAssemblyOperandKind::Label,
            ty,
            None,
            None,
            None,
            None,
            None,
            0,
            5,
        ));

        let contract = InlineAssemblyContract::try_new(
            constant, constant, constant, constant, constant, operands, 1, "", "label",
        )
        .unwrap_or_else(|| panic!("test assembly contract must validate"));

        builder.set_terminator(
            entry,
            source.clone(),
            MirTerminatorKind::InlineAssembly(MirInlineAssemblyTerminator::new(
                contract,
                immediate(ty),
                ty,
                ty,
                normal,
                [alternate],
                [],
            )),
        );

        builder.set_terminator(
            normal,
            source.clone(),
            MirTerminatorKind::Return(Some(MirOperand::Value(output))),
        );

        builder.set_terminator(alternate, source, MirTerminatorKind::Return(None));

        assert_round_trip(builder.finish(entry));
    }

    #[test]
    fn reconstruction_retains_every_frame_entry_and_initialized_storage() {
        let bound = test_bound_unit(44);
        let source = crate::MirSourceAnchor::from(bound.key().source());
        let ty = crate::test_support::test_type();
        let frame = ProtectedAsyncFrameId::new([11; 32]);
        let abi = RuntimeAbiVersion::new(1, 0);

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::ProtectedAsyncFrame(frame),
            crate::test_support::test_target(),
        );

        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let resume = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let external_resume = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);

        let cancellation = push_block(
            &mut builder,
            source.clone(),
            MirBlockKind::CleanupBroadcast,
        );

        let lifecycle = push_block(
            &mut builder,
            source.clone(),
            MirBlockKind::LifecycleResolution,
        );

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::CurrentFrame, ty)
            .unwrap_or_else(|error| panic!("frame storage must be valid: {error:?}"));

        builder
            .push_operation(
                entry,
                source.clone(),
                MirOperationKind::Async(MirAsyncOperation::ResumeFrame {
                    frame,
                    state: MirFrameStateId::new(0),
                    storage,
                    runtime: MirRuntimeReference::new(RuntimeAbiRole::FrameResume, abi),
                }),
                None,
            )
            .unwrap_or_else(|error| panic!("frame resume must be valid: {error:?}"));

        builder.set_terminator(
            entry,
            source.clone(),
            MirTerminatorKind::Suspend {
                kind: MirSuspensionKind::Yield,
                payload: None,
                resume_state: MirFrameStateId::new(1),
                resume: MirEdge::new(resume, []),
                cancellation: MirCleanupEdge::new(
                    MirCleanupPhase::TaskCancellation,
                    MirEdge::new(cancellation, []),
                ),
                registration: MirRuntimeReference::new(
                    RuntimeAbiRole::SuspensionRegistration,
                    abi,
                ),
                wake: MirRuntimeReference::new(RuntimeAbiRole::Wake, abi),
            },
        );

        builder.set_terminator(resume, source.clone(), MirTerminatorKind::Return(None));

        builder.set_terminator(
            external_resume,
            source.clone(),
            MirTerminatorKind::Return(None),
        );

        builder.set_terminator(
            cancellation,
            source.clone(),
            MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                MirCleanupPhase::LifecycleResolution,
                MirEdge::new(lifecycle, []),
            )),
        );

        builder.set_terminator(lifecycle, source, MirTerminatorKind::Return(None));

        let descriptor = MirFrameDescriptor::new(
            frame,
            abi,
            ProtectedFrameAbiVersions::uniform(abi),
            ty,
            [
                MirFrameState::new(MirFrameStateId::new(0), entry, [], []),
                MirFrameState::new(MirFrameStateId::new(1), resume, [], []),
                MirFrameState::new(
                    MirFrameStateId::new(2),
                    external_resume,
                    [],
                    [storage],
                ),
            ],
        )
        .unwrap_or_else(|error| panic!("frame descriptor must be valid: {error:?}"));

        builder.set_frame_descriptor(descriptor);

        assert_round_trip(builder.finish(entry));
    }

    fn ordinary_unit(with_dead_block: bool) -> (MirUnit, Option<crate::MirBlockId>) {
        let bound = test_bound_unit(u32::from(with_dead_block) + 40);
        let source = crate::MirSourceAnchor::from(bound.key().source());
        let ty = crate::test_support::test_type();

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::Synchronous,
            crate::test_support::test_target(),
        );

        let entry = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);

        let dead = with_dead_block
            .then(|| push_block(&mut builder, source.clone(), MirBlockKind::Ordinary));

        let header = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let body = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);
        let exit = push_block(&mut builder, source.clone(), MirBlockKind::Ordinary);

        let header_value = builder
            .push_block_parameter(header, source.clone(), ty)
            .unwrap_or_else(|error| panic!("header parameter must be valid: {error:?}"));

        let body_value = builder
            .push_block_parameter(body, source.clone(), ty)
            .unwrap_or_else(|error| panic!("body parameter must be valid: {error:?}"));

        let exit_value = builder
            .push_block_parameter(exit, source.clone(), ty)
            .unwrap_or_else(|error| panic!("exit parameter must be valid: {error:?}"));

        if let Some(dead) = dead {
            let storage = builder
                .push_storage(source.clone(), MirStorageKind::Temporary, ty)
                .unwrap_or_else(|error| panic!("dead storage must be valid: {error:?}"));

            push_store(&mut builder, dead, source.clone(), storage, ty);
            builder.set_terminator(dead, source.clone(), MirTerminatorKind::Return(None));
        }

        let live_storage = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, ty)
            .unwrap_or_else(|error| panic!("live storage must be valid: {error:?}"));

        push_store(&mut builder, body, source.clone(), live_storage, ty);

        builder.set_terminator(
            entry,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(header, [immediate(ty)])),
        );

        builder.set_terminator(
            header,
            source.clone(),
            MirTerminatorKind::Branch {
                condition: immediate(ty),
                then_edge: MirEdge::new(body, [MirOperand::Value(header_value)]),
                else_edge: MirEdge::new(exit, [MirOperand::Value(header_value)]),
            },
        );

        builder.set_terminator(
            body,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(header, [MirOperand::Value(body_value)])),
        );

        builder.set_terminator(
            exit,
            source,
            MirTerminatorKind::Return(Some(MirOperand::Value(exit_value))),
        );

        (builder.finish(entry), dead)
    }

    fn push_store(
        builder: &mut MirUnitBuilder,
        block: crate::MirBlockId,
        source: crate::MirSourceAnchor,
        storage: crate::MirStorageId,
        ty: bray_symbols::TypeId,
    ) {
        builder
            .push_operation(
                block,
                source,
                MirOperationKind::Store {
                    kind: MirStoreKind::Initialize,
                    destination: MirPlace::new(storage, [], ty),
                    value: immediate(ty),
                },
                None,
            )
            .unwrap_or_else(|error| panic!("store operation must be valid: {error:?}"));
    }

    fn immediate(ty: bray_symbols::TypeId) -> MirOperand {
        MirOperand::Immediate {
            value: MirImmediateValue::Unit,
            ty,
        }
    }

    fn push_block(
        builder: &mut MirUnitBuilder,
        source: crate::MirSourceAnchor,
        kind: MirBlockKind,
    ) -> crate::MirBlockId {
        builder
            .push_block(source, kind)
            .unwrap_or_else(|error| panic!("test block must be valid: {error:?}"))
    }

    fn assert_round_trip(unit: MirUnit) {
        assert!(unit.is_valid());

        let (reconstructed, _) = reconstruct_reachable(&unit)
            .unwrap_or_else(|error| panic!("test MIR must reconstruct: {error:?}"));

        assert_eq!(reconstructed, unit);
    }
}
