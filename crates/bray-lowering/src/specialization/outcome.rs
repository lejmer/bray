use bray_ir::{
    MirBlockId, MirBlockKind, MirCallPanicEdge, MirEdge, MirSourceAnchor, MirTerminatorKind,
    MirUnitBuildError, MirUnitBuilder,
};

pub(super) fn forward_collected_outcome(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    outcome: &crate::cleanup_outcome::CleanupOutcome,
    edges: (&MirEdge, MirCallPanicEdge, &MirEdge),
) -> Result<(), MirUnitBuildError> {
    let (completed_edge, panicked, cancelled_edge) = edges;

    let panic_bridge = builder.push_block(source.clone(), MirBlockKind::LifecycleResolution)?;

    let cancellation_bridge =
        builder.push_block(source.clone(), MirBlockKind::LifecycleResolution)?;

    let completed = outcome.dispatch(builder, block, source, panic_bridge, cancellation_bridge)?;

    // Preserve the caller's ownership arguments while forwarding only the newly collected failure.
    for (block, edge) in [
        (completed, completed_edge.clone()),
        (
            panic_bridge,
            MirEdge::new(panicked.target(), [outcome.report()]),
        ),
        (cancellation_bridge, cancelled_edge.clone()),
    ] {
        builder.set_terminator(block, source.clone(), MirTerminatorKind::Goto(edge))?;
    }

    Ok(())
}
