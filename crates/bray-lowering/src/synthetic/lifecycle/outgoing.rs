use bray_ir::{
    MirCapacityError, MirOperationKind, MirRuntimeReference, MirTerminatorKind, MirUnitBuilder,
};
use bray_symbols::TypeId;

pub(super) fn discharge_owner(
    builder: &mut MirUnitBuilder,
    ty: TypeId,
) -> Result<(), MirCapacityError> {
    let runtime = MirRuntimeReference::new(
        bray_runtime_interface::RuntimeAbiRole::OutgoingDischarge,
        builder.target().runtime_abi(),
    );

    let exits = builder
        .terminated_blocks()
        .filter(|&(_id, terminal)| matches!(
                terminal.kind(),
                MirTerminatorKind::Return { .. }
                    | MirTerminatorKind::PropagatePanic { .. }
                    | MirTerminatorKind::PropagateCancellation { .. }
            )).map(|(id, terminal)| (id, terminal.source().clone()))
        .collect::<Vec<_>>();

    for (block, source) in exits {
        builder.push_operation(
            block,
            source,
            MirOperationKind::DischargeOutgoing {
                owner: bray_ir::MirOutgoingOwner::Value(ty),
                runtime,
            },
            None,
        )?;
    }

    Ok(())
}
