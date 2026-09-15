use bray_ir::{
    MirOperationKind, MirRuntimeReference, MirTerminatorKind, MirUnitBuildError, MirUnitBuilder,
};
use bray_symbols::TypeId;

pub(super) fn discharge_owner(
    builder: &mut MirUnitBuilder,
    ty: TypeId,
) -> Result<(), MirUnitBuildError> {
    let runtime = MirRuntimeReference::new(
        bray_runtime_interface::RuntimeAbiRole::OutgoingDischarge,
        builder.target().runtime_abi(),
    );

    let exits = builder
        .terminated_blocks()
        .filter_map(|(id, terminal)| {
            matches!(
                terminal.kind(),
                MirTerminatorKind::Return(_)
                    | MirTerminatorKind::PropagatePanic { .. }
                    | MirTerminatorKind::PropagateCancellation { .. }
            )
            // Each discharge operation retains its exit's immutable source provenance.
            .then(|| (id, terminal.source().clone()))
        })
        .collect::<Vec<_>>();

    for (block, source) in exits {
        builder.push_operation(
            block,
            source,
            MirOperationKind::DischargeOutgoing { ty, runtime },
            None,
        )?;
    }

    Ok(())
}
