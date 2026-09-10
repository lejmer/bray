use bray_ir::{
    MirBlockId, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge, MirOperationKind, MirPlace,
    MirSourceAnchor, MirTerminatorKind, MirUnitBuildError, MirUnitBuilder,
};

use crate::cleanup_outcome::CleanupOutcome;

/// Broadcasts cancellation to tasks discovered in a transferred completion before waiting on them.
pub(crate) fn broadcast_cancellation(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    payload: MirPlace,
    cleanup: bray_bound_tree::AsyncStorageCleanupRequirement,
    outcome: &CleanupOutcome,
) -> Result<MirBlockId, MirUnitBuildError> {
    if !matches!(cleanup, bray_bound_tree::AsyncStorageCleanupRequirement::Cleanup(phases) if phases.includes_cancellation())
    {
        return Ok(block);
    }

    // Each generated block and edge owns the same shared source provenance.
    let entry = builder.push_block(source.clone(), MirBlockKind::Ordinary)?;
    let broadcast = builder.push_block(source.clone(), MirBlockKind::CleanupBroadcast)?;
    let lifecycle = builder.push_block(source.clone(), MirBlockKind::LifecycleResolution)?;

    builder.set_terminator(
        block,
        source.clone(),
        MirTerminatorKind::Goto(MirEdge::new(entry, [])),
    )?;

    builder.set_terminator(
        entry,
        source.clone(),
        MirTerminatorKind::BeginCleanup(MirCleanupEdge::new(
            MirCleanupPhase::TaskCancellation,
            MirEdge::new(broadcast, []),
        )),
    )?;

    builder.push_operation(
        broadcast,
        source.clone(),
        MirOperationKind::Cleanup {
            phase: MirCleanupPhase::TaskCancellation,
            place: payload,
        },
        None,
    )?;

    let broadcast = outcome.check(builder, broadcast, source)?;

    builder.set_terminator(
        broadcast,
        source.clone(),
        MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
            MirCleanupPhase::LifecycleResolution,
            MirEdge::new(lifecycle, []),
        )),
    )?;

    Ok(lifecycle)
}
