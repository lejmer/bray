use bray_bound_tree::AsyncStorageCleanupRequirement;
use bray_ir::{
    MirAbandonmentAction, MirBlockId, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge,
    MirGeneratedLifecycleRole, MirOperationKind, MirSourceAnchor, MirTerminatorKind,
};

use super::control::{CleanupDestination, TerminalState};
use crate::lowering::LoweringError;
use crate::lowering::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(in crate::lowering) fn lower_inactive_cleanup(
        &mut self,
        source: &MirSourceAnchor,
    ) -> Result<MirBlockId, LoweringError> {
        self.lower_capture_cleanup(
            source,
            MirGeneratedLifecycleRole::Cleanup(MirCleanupPhase::LifecycleResolution),
        )
    }

    pub(in crate::lowering) fn lower_capture_abandonment(
        &mut self,
        source: &MirSourceAnchor,
    ) -> Result<(MirBlockId, MirBlockId), LoweringError> {
        let quiescence = self.lower_capture_cleanup(
            source,
            MirGeneratedLifecycleRole::Abandon(MirAbandonmentAction::Quiesce),
        )?;

        let destruction = self.lower_capture_cleanup(
            source,
            MirGeneratedLifecycleRole::Abandon(MirAbandonmentAction::Destroy),
        )?;

        Ok((quiescence, destruction))
    }

    fn lower_capture_cleanup(
        &mut self,
        source: &MirSourceAnchor,
        role: MirGeneratedLifecycleRole,
    ) -> Result<MirBlockId, LoweringError> {
        let plan = self
            .input
            .lowering_plans()
            .capture_cleanup()
            .ok_or_else(|| LoweringError::SemanticValueUnavailable)?;

        let abandonment = matches!(role, MirGeneratedLifecycleRole::Abandon(_));

        let broadcast = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::CleanupBroadcast,
        )?;

        let entry = if abandonment {
            let entry = self
                .builder
                .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

            self.set_terminator(
                entry,
                Self::retained_source(source),
                MirTerminatorKind::BeginCleanup(MirCleanupEdge::new(
                    MirCleanupPhase::TaskCancellation,
                    MirEdge::new(broadcast, []),
                )),
            )?;

            entry
        } else {
            broadcast
        };

        let outcome = self.create_cleanup_outcome(broadcast, source)?;

        if !abandonment {
            outcome.initialize_cancellation(&mut self.builder, broadcast, source)?;
        }

        self.cleanup_outcome = Some(outcome);

        let mut captures = Vec::with_capacity(plan.captures().len());
        let retained_count = plan.captures().len();

        for access in plan.captures() {
            let place = self.place_for_access(*access, false)?;

            self.cleanup_retained_storages.push(place.storage());

            let shape = self
                .input
                .lowering_plans()
                .cleanup_type(place.ty())
                .ok_or(LoweringError::MissingCleanupExecution(place.storage()))?;

            if let AsyncStorageCleanupRequirement::Cleanup(phases) = shape.cleanup() {
                captures.push((place, phases));
            }
        }

        let mut block = broadcast;

        for (place, phases) in &captures {
            if phases.includes_cancellation()
                && role != MirGeneratedLifecycleRole::Abandon(MirAbandonmentAction::Destroy)
            {
                self.push_cleanup_operation(
                    block,
                    Self::retained_source(source),
                    MirOperationKind::Cleanup {
                        phase: MirCleanupPhase::TaskCancellation,
                        place: Self::retained_place(place),
                    },
                    [place.storage()],
                )?;

                block = self.check_cleanup_action_outcome(block, source)?;
            }
        }

        let lifecycle = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        self.set_terminator(
            block,
            Self::retained_source(source),
            MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                MirCleanupPhase::LifecycleResolution,
                MirEdge::new(lifecycle, []),
            )),
        )?;

        block = lifecycle;

        for (place, phases) in &captures {
            if phases.includes_lifecycle() {
                block = self
                    .push_lifecycle_cleanup(
                        block,
                        source,
                        role,
                        Self::retained_place(place),
                        None,
                        false,
                    )?
                    .0;
            }
        }

        for _ in 0..retained_count {
            self.cleanup_retained_storages.pop();
        }

        let outcome = self
            .cleanup_outcome
            .take()
            .ok_or(LoweringError::SemanticValueUnavailable)?;

        let terminal = self.terminal_state_block(source, TerminalState::Cancelled)?;

        if abandonment {
            let completed = self.dispatch_cleanup_outcome(
                block,
                source,
                outcome,
                CleanupDestination::Goto(terminal),
            )?;

            let terminal = self.terminal_state_block(source, TerminalState::CapturesCompleted)?;

            self.set_destination(completed, source, CleanupDestination::Goto(terminal), None)?;
        } else {
            self.finish_cancelled_cleanup(
                block,
                source,
                outcome,
                CleanupDestination::Goto(terminal),
            )?;
        }

        Ok(entry)
    }
}
