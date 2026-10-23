use std::collections::{BTreeMap, BTreeSet};

use bray_symbols::TypeId;

use super::super::local_id_remap::{MirLocalIdMapping, remap_operation, remap_terminator};
use super::mapping::{LocalIds, copy_unit_layout, predict_results};
use crate::{
    MirBlockId, MirCapacityError, MirEdge, MirOperationKind, MirPlace, MirStorageId,
    MirStorageKind, MirTerminatorKind, MirUnit, MirUnitBuilder,
};

/// Appends the selected checked receiver tail to its owner's existing continuation.
/// Storage replacements retain the consumed receiver and the body-borrowed boundary record.
/// Failure exits accept one owned report argument. No source body operation is copied.
#[expect(
    clippy::too_many_arguments,
    reason = "the receiver boundary retains its unit, disposition, substitutions and existing outcome exits"
)]
pub fn append_cleanup_continuation(
    builder: &mut MirUnitBuilder,
    unit: &MirUnit,
    concrete_types: &BTreeMap<TypeId, TypeId>,
    callables: &BTreeMap<crate::MirCallableReference, crate::MirCallableReference>,
    places: BTreeMap<MirStorageId, MirPlace>,
    abandoned: bool,
    completed: MirEdge,
    panicked: MirBlockId,
    cancelled: MirBlockId,
    retention: Option<&std::sync::Arc<crate::MirFrameRetention>>,
) -> Result<BTreeMap<u32, MirBlockId>, MirCapacityError> {
    builder.retain_checked_sources();

    let descriptor = unit
        .cleanup_continuation()
        .expect("a receiver tail has its checked descriptor");

    let selected = |entry: &crate::MirCleanupEntry| {
        if abandoned {
            entry.abandoned()
        } else {
            entry.ordinary()
        }
    };

    let roots = descriptor
        .entries()
        .iter()
        .filter(|entry| entry.is_initial())
        .map(&selected);

    let handoffs = descriptor
        .entries()
        .iter()
        .map(|entry| (entry.handoff(), vec![selected(entry)]))
        .collect::<BTreeMap<_, _>>();

    let blocks = crate::reachable_blocks(unit, roots, &handoffs);
    let mut ids = LocalIds::new(unit, concrete_types);

    ids.places = places;
    ids.callables = Some(callables);

    let mut storage = BTreeSet::new();

    for id in &blocks {
        let block = unit.block(*id).expect("selected tail block exists");

        for operation in block.operations() {
            crate::for_each_operation_storage(
                unit.operation(*operation)
                    .expect("tail operation exists")
                    .kind(),
                |id| {
                    storage.insert(id);
                },
            );
        }

        crate::for_each_terminator_storage(block.terminator().kind(), |id| {
            storage.insert(id);
        });
    }

    copy_unit_layout(
        unit,
        &mut ids,
        builder,
        blocks
            .iter()
            .map(|id| (*id, unit.block(*id).expect("selected tail block exists"))),
        storage
            .into_iter()
            .map(|id| (id, unit.storage(id).expect("tail storage exists"))),
        |kind| {
            assert!(
                !matches!(
                    kind,
                    MirStorageKind::Parameter(_) | MirStorageKind::BorrowedParameter(_)
                ),
                "receiver tail parameters must be supplied by their caller"
            );

            kind.clone()
        },
    )?;

    let output_unit = completed.target().unit();
    let mut next_value = builder.value_count();

    predict_results(
        unit,
        &mut ids,
        blocks.iter().copied(),
        None,
        &mut next_value,
        output_unit,
    )?;

    for id in &blocks {
        let block = unit.block(*id).expect("tail block exists");

        for operation in block.operations() {
            let operation = unit.operation(*operation).expect("tail operation exists");

            // Appended handoffs are direct edges. Only the shared body publishes a selector.
            if matches!(operation.kind(), MirOperationKind::Store { destination, .. } if destination == descriptor.selector())
            {
                continue;
            }

            let mut kind = operation.kind().clone();

            remap_operation(&mut kind, &ids);

            if let Some(Some(collection)) = kind.cleanup_collection_slot() {
                collection.retention = retention.cloned();
            }

            let result = operation
                .result()
                .map(|value| ids.ty(unit.value(value).expect("tail result exists").ty()));

            let actual =
                builder.push_operation(ids.block(*id), operation.source().clone(), kind, result)?;

            assert_eq!(
                actual.result(),
                operation.result().map(|value| ids.value(value)),
                "tail SSA allocation order is stable"
            );
        }

        let mut terminal = block.terminator().kind().clone();

        remap_terminator(&mut terminal, &ids);

        let terminal = if let Some(next) = handoffs.get(id) {
            MirTerminatorKind::Goto(MirEdge::new(ids.block(next[0]), []))
        } else {
            match terminal {
                MirTerminatorKind::Return {
                    value: None,
                    report,
                } => {
                    let edge = if let Some(report) = report {
                        assert_eq!(
                            completed.arguments().len(),
                            1,
                            "a report-producing receiver tail requires its caller's report destination"
                        );

                        MirEdge::new(completed.target(), [report])
                    } else {
                        completed.clone()
                    };

                    MirTerminatorKind::Goto(edge)
                }
                MirTerminatorKind::PropagatePanic { report, .. } => {
                    MirTerminatorKind::Goto(MirEdge::new(panicked, [report]))
                }
                MirTerminatorKind::PropagateCancellation { report, .. } => {
                    MirTerminatorKind::Goto(MirEdge::new(cancelled, [report]))
                }
                other => other,
            }
        };

        builder.set_terminator(
            ids.block(*id),
            block.terminator().source().clone(),
            terminal,
        );
    }

    Ok(descriptor
        .entries()
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.is_initial())
        .map(|(index, entry)| {
            (
                u32::try_from(index + 1).expect("entry capacity is bounded by MIR blocks"),
                ids.block(selected(entry)),
            )
        })
        .collect())
}
