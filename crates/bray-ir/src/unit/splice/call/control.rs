use super::super::mapping::LocalIds;
use super::values::InlineValues;
use crate::unit::local_id_remap::{MirLocalIdMapping, remap_operation, remap_terminator};
use crate::{
    MirBlockId, MirBlockKind, MirCallFailureEdge, MirCapacityError, MirEdge, MirOperand,
    MirOperationId, MirOperationKind, MirPlace, MirStoreKind, MirTerminatorKind, MirUnit,
    MirUnitBuilder, MirValueId,
};

pub(super) fn copy_inline_body(
    callee: &MirUnit,
    ids: &mut LocalIds<'_>,
    builder: &mut MirUnitBuilder,
    normal: (MirBlockId, Option<&MirPlace>),
    failure: Option<(&MirCallFailureEdge, &MirCallFailureEdge)>,
    caller_ids: &mut LocalIds<'_>,
    values: &InlineValues,
) -> Result<(), MirCapacityError> {
    for (old, block) in callee.blocks_with_ids() {
        ids.parameters = values.parameters[&old].clone();

        let retained = &values.retained[&old];

        values.map_caller(caller_ids, retained.iter().copied());

        let destination = ids.block(old);

        for id in block.operations() {
            copy_operation(callee, *id, destination, ids, builder)?;
        }

        let mut kind = block.terminator().kind().clone();

        remap_terminator(&mut kind, ids);

        if !matches!(
            kind,
            MirTerminatorKind::Return { .. }
                | MirTerminatorKind::PropagatePanic { .. }
                | MirTerminatorKind::PropagateCancellation { .. }
        ) {
            kind.try_for_each_edge_mut::<()>(|edge| {
                *edge = MirEdge::new(
                    edge.target(),
                    edge.arguments()
                        .iter()
                        .cloned()
                        .chain(
                            values
                                .parameter_order
                                .iter()
                                .map(|parameter| MirOperand::Value(ids.parameters[parameter])),
                        )
                        .chain(retained.iter().copied().map(MirOperand::Value)),
                );

                Ok(())
            })
            .expect("edge arguments can be extended");

            if let MirTerminatorKind::CheckCallOutcome {
                panicked,
                cancelled,
                ..
            } = &mut kind
            {
                bridge_failure(builder, panicked, block.terminator().source(), values)?;
                bridge_failure(builder, cancelled, block.terminator().source(), values)?;
            }
        }

        let kind = inline_outcome(
            builder,
            destination,
            block.terminator().source(),
            kind,
            (normal.0, normal.1, retained),
            failure,
            caller_ids,
        )?;

        builder.set_terminator(destination, block.terminator().source().clone(), kind);
    }

    Ok(())
}

fn bridge_failure(
    builder: &mut MirUnitBuilder,
    edge: &mut MirCallFailureEdge,
    source: &crate::MirSourceAnchor,
    values: &InlineValues,
) -> Result<(), MirCapacityError> {
    if values.continuation.is_empty() {
        return Ok(());
    }

    assert!(
        values.parameter_order.is_empty(),
        "scalar callees have no report-only failure edges"
    );

    let bridge = builder.push_block(source.clone(), MirBlockKind::Ordinary)?;

    builder.set_terminator(
        bridge,
        source.clone(),
        MirTerminatorKind::Goto(MirEdge::new(
            edge.target(),
            values
                .continuation
                .keys()
                .map(|value| MirOperand::Move(values.spills[value].clone())),
        )),
    );

    edge.target = bridge;

    Ok(())
}

pub(super) fn copy_operation(
    unit: &MirUnit,
    id: MirOperationId,
    destination: MirBlockId,
    ids: &LocalIds<'_>,
    builder: &mut MirUnitBuilder,
) -> Result<(), MirCapacityError> {
    let operation = unit.operation(id).expect("operation exists");
    let mut kind = operation.kind().clone();

    remap_operation(&mut kind, ids);

    let ty = operation
        .result()
        .map(|value| ids.ty(unit.value(value).expect("operation result").ty()));

    let actual = builder.push_operation(destination, operation.source().clone(), kind, ty)?;

    assert_eq!(
        actual.result(),
        operation.result().map(|value| ids.value(value))
    );

    Ok(())
}

pub(super) fn store(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &crate::MirSourceAnchor,
    destination: MirPlace,
    value: MirOperand,
) -> Result<(), MirCapacityError> {
    builder.push_operation(
        block,
        source.clone(),
        MirOperationKind::Store {
            kind: MirStoreKind::Initialize,
            destination,
            value,
        },
        None,
    )?;

    Ok(())
}

fn inline_outcome(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &crate::MirSourceAnchor,
    kind: MirTerminatorKind,
    normal: (MirBlockId, Option<&MirPlace>, &[MirValueId]),
    failure: Option<(&MirCallFailureEdge, &MirCallFailureEdge)>,
    ids: &LocalIds<'_>,
) -> Result<MirTerminatorKind, MirCapacityError> {
    let (report, edge) = match kind {
        MirTerminatorKind::Return {
            value,
            report: None,
        } => {
            if let Some(report) = normal.1 {
                let mut destination = report.clone();

                destination.remap_local_ids(ids);

                builder.push_operation(
                    block,
                    source.clone(),
                    MirOperationKind::TakeIncomingReport { destination },
                    None,
                )?;
            }

            return Ok(MirTerminatorKind::Goto(MirEdge::new(
                normal.0,
                value
                    .into_iter()
                    .chain(normal.2.iter().copied().map(MirOperand::Value)),
            )));
        }
        MirTerminatorKind::PropagatePanic { report, .. } => {
            (report, failure.expect("checked panic exit").0)
        }
        MirTerminatorKind::PropagateCancellation { report, .. } => {
            (report, failure.expect("checked cancellation exit").1)
        }
        kind => return Ok(kind),
    };

    let mut destination = edge.report().clone();

    destination.remap_local_ids(ids);
    store(builder, block, source, destination, report)?;

    Ok(MirTerminatorKind::Goto(MirEdge::new(
        ids.block(edge.target()),
        [],
    )))
}
