use std::collections::{BTreeMap, BTreeSet};

use bray_ir::{
    MirBlockId, MirCleanupContinuation, MirCleanupEntry, MirOperand, MirOperationKind, MirPlace,
    MirProjectionKind, MirStorageId, MirStorageKind, MirStoreKind, MirUnit, MirUnitBuilder,
    MirValueId, MirValueOrigin,
};
use bray_symbols::{BorrowKind, TypeData};

use super::boundary::ReceiverCleanup;
use crate::input::LoweringInput;
use crate::lowering::LoweringError;

pub(in crate::lowering) fn finish_receiver_cleanup(
    input: &LoweringInput<'_>,
    unit: MirUnit,
    receiver: Option<ReceiverCleanup>,
    storages: &BTreeMap<bray_bound_tree::StorageIdentityId, MirStorageId>,
) -> Result<MirUnit, LoweringError> {
    let Some(receiver) = receiver.filter(|receiver| !receiver.entries.is_empty()) else {
        return Ok(unit);
    };

    let body = bray_ir::reachable_blocks(&unit, [unit.entry()], &BTreeMap::new());

    let handoffs = receiver
        .entries
        .iter()
        .map(|(handoff, ordinary, abandoned)| (*handoff, vec![*ordinary, *abandoned]))
        .collect();

    let roots = receiver
        .entries
        .iter()
        .filter(|(handoff, _, _)| body.contains(handoff))
        .flat_map(|(_, ordinary, abandoned)| [*ordinary, *abandoned]);

    let tail = bray_ir::reachable_blocks(&unit, roots, &handoffs);
    let body_storage = referenced_storage(&unit, &body);
    let tail_storage = referenced_storage(&unit, &tail);

    let mut captured = body_storage
        .intersection(&tail_storage)
        .copied()
        .filter(|id| {
            !matches!(
                unit.storage(*id).expect("referenced storage exists").kind(),
                MirStorageKind::Parameter(0)
                    | MirStorageKind::BorrowedParameter(0)
                    | MirStorageKind::Static(_)
                    | MirStorageKind::NativeStatic(_)
            )
        })
        .collect::<BTreeSet<_>>();

    let consumers = storages
        .iter()
        .filter_map(|(identity, storage)| tail_storage.contains(storage).then_some(*identity))
        .chain([receiver.identity])
        .collect();

    // The owning analysis already closed this relation. A tail can retain a local through
    // a receiver-held reference without naming that local in one of its MIR operands.
    captured.extend(
        input
            .dependency_contracts()
            .retained_lifecycle_storage(&consumers)
            .filter_map(|identity| storages.get(&identity).copied())
            .filter(|id| {
                !matches!(
                    unit.storage(*id)
                        .expect("lowered dependency storage exists")
                        .kind(),
                    MirStorageKind::Parameter(0)
                        | MirStorageKind::BorrowedParameter(0)
                        | MirStorageKind::Static(_)
                        | MirStorageKind::NativeStatic(_)
                )
            }),
    );

    captured.insert(receiver.selector.storage());

    let mut builder = MirUnitBuilder::for_expansion(&unit);
    let values = crossing_values(&unit, &tail);
    let mut replacements = BTreeMap::new();

    for (value, block) in values {
        let definition = unit
            .value(value)
            .expect("tail input has its MIR definition");

        let storage = builder.push_storage(
            definition.source().clone(),
            MirStorageKind::Temporary,
            definition.ty(),
        )?;

        let place = MirPlace::new(storage, [], definition.ty());

        builder.push_operation(
            block,
            definition.source().clone(),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: place.clone(),
                value: MirOperand::Value(value),
            },
            None,
        )?;

        captured.insert(storage);
        replacements.insert(value, MirOperand::Copy(place));
    }

    for block in &tail {
        builder.substitute_block_values(*block, &replacements);
    }

    let source = unit
        .storage(receiver.selector.storage())
        .expect("selector storage exists")
        .source();

    let fields = std::iter::once(receiver.selector.storage())
        .chain(
            captured
                .into_iter()
                .filter(|id| *id != receiver.selector.storage()),
        )
        .collect::<Vec<_>>();

    let types = fields
        .iter()
        .map(|id| builder.storage_type(*id))
        .collect::<Vec<_>>();

    let tuple = input
        .semantic_values()
        .intern_type(TypeData::Tuple(types.into()))?;

    let pointer = input.semantic_values().intern_type(TypeData::Borrow {
        kind: BorrowKind::Mutable,
        target: tuple,
    })?;

    let context = builder.push_storage(source.clone(), MirStorageKind::Parameter(1), pointer)?;
    let record = MirPlace::new(context, [], pointer).project(MirProjectionKind::Dereference, tuple);

    let places = fields
        .into_iter()
        .enumerate()
        .map(|(index, storage)| {
            let index = u32::try_from(index).expect("MIR storage capacity bounds record fields");

            (
                storage,
                record.project(
                    MirProjectionKind::TupleField(index),
                    builder.storage_type(storage),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let selector = places
        .get(&receiver.selector.storage())
        .expect("selector is the first captured field")
        .clone();

    builder.substitute_places(&places);

    builder.set_cleanup_continuation(MirCleanupContinuation::new(
        context,
        selector,
        receiver
            .entries
            .into_iter()
            .map(|(handoff, ordinary, abandoned)| {
                MirCleanupEntry::new(handoff, ordinary, abandoned, body.contains(&handoff))
            }),
    ));

    // One reconstruction retires callee-local slots replaced by the caller's record.
    Ok(bray_ir::reconstruct_reachable(&builder.finish(unit.entry()))?.0)
}

fn referenced_storage(unit: &MirUnit, blocks: &BTreeSet<MirBlockId>) -> BTreeSet<MirStorageId> {
    let mut storage = BTreeSet::new();

    for id in blocks {
        let block = unit.block(*id).expect("reachable block exists");

        for operation in block.operations() {
            bray_ir::for_each_operation_storage(
                unit.operation(*operation)
                    .expect("block operation exists")
                    .kind(),
                |id| {
                    storage.insert(id);
                },
            );
        }

        bray_ir::for_each_terminator_storage(block.terminator().kind(), |id| {
            storage.insert(id);
        });
    }

    storage
}

fn crossing_values(
    unit: &MirUnit,
    tail: &BTreeSet<MirBlockId>,
) -> BTreeMap<MirValueId, MirBlockId> {
    let owners = unit
        .blocks_with_ids()
        .flat_map(|(id, block)| {
            block
                .operations()
                .iter()
                .map(move |operation| (*operation, id))
        })
        .collect::<BTreeMap<_, _>>();

    let mut crossing = BTreeMap::new();

    let mut collect = |operand: &MirOperand| {
        let MirOperand::Value(value) = operand else {
            return;
        };

        let block = match unit.value(*value).expect("tail input exists").origin() {
            MirValueOrigin::BlockParameter(block) => block,
            MirValueOrigin::Operation(operation) => owners[&operation],
        };

        if !tail.contains(&block) {
            crossing.insert(*value, block);
        }
    };

    for id in tail {
        let block = unit.block(*id).expect("reachable tail block exists");

        for operation in block.operations() {
            unit.operation(*operation)
                .expect("tail operation exists")
                .kind()
                .for_each_operand(&mut collect);
        }

        block.terminator().kind().for_each_input(&mut collect);

        // Edge arguments include inputs not visited by the terminator's direct-input walker.
        let mut terminal = block.terminator().kind().clone();

        terminal
            .try_for_each_edge_mut::<()>(|edge| {
                for argument in edge.arguments() {
                    argument.for_each_operand(&mut collect);
                }

                Ok(())
            })
            .expect("observing tail edges is infallible");
    }

    crossing
}
