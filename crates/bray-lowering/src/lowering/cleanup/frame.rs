use super::control::CleanupDestination;
use bray_bound_tree::BoundExecutionSite;
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCleanupPhase, MirEdge, MirOperand, MirSourceAnchor,
    MirTerminatorKind,
};

use crate::cleanup_outcome::CleanupOutcome;
use crate::lowering::LoweringError;
use crate::lowering::inputs::InputExit;
use crate::lowering::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(in crate::lowering) fn frame_completion(
        &mut self,
        source: &MirSourceAnchor,
        result_type: bray_symbols::TypeId,
    ) -> Result<Option<bray_ir::MirFrameStateId>, LoweringError> {
        let Some(phases) = self.input_cleanup_phases(result_type) else {
            return Ok(None);
        };

        assert!(
            self.input_temporaries.is_empty(),
            "completed source frame has no outstanding input temporaries"
        );

        self.completion_cleanup = Some(result_type);

        let storage = self.builder.push_storage(
            Self::retained_source(source),
            bray_ir::MirStorageKind::FrameCompletion,
            result_type,
        )?;

        let place = bray_ir::MirPlace::new(storage, [], result_type);

        let entry = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::CleanupBroadcast,
        )?;

        let cancelled =
            self.terminal_state_block(source, super::control::TerminalState::Cancelled)?;

        self.finish_abnormal_cleanup(
            bray_ir::MirCleanupPhase::LifecycleResolution,
            entry,
            source,
            None,
            CleanupDestination::Goto(cancelled),
            &[],
            Some(&place),
        )?;

        let broadcast = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::CleanupBroadcast,
        )?;

        if phases.includes_cancellation() {
            let (outcome, finish) = self.frame_broadcast_outcome(source)?;

            outcome.begin(&mut self.builder, broadcast, source)?;

            let previous = self.cleanup_outcome.replace(outcome);
            let failures = self.cleanup_failure_targets.take();

            let (end, _) = self.push_guarded_cleanup(
                broadcast,
                source,
                MirCleanupPhase::TaskCancellation,
                place,
                None,
                None,
                false,
                None,
            )?;

            self.set_terminator(
                end,
                Self::retained_source(source),
                MirTerminatorKind::Goto(MirEdge::new(finish, [])),
            )?;

            self.cleanup_outcome = previous;
            self.cleanup_failure_targets = failures;
        } else {
            self.set_terminator(
                broadcast,
                Self::retained_source(source),
                MirTerminatorKind::Return(None),
            )?;
        }

        let state = self.next_frame_state()?;

        self.frame_states.push(
            bray_ir::MirFrameState::new(
                state,
                entry,
                broadcast,
                self.execution_lane_requirements(),
                [storage],
            )
            .with_affinity(self.frame_affinity())
            .with_completion_cleanup(true),
        );

        self.completion_cleanup = None;

        Ok(Some(state))
    }

    pub(in crate::lowering) fn frame_broadcast(
        &mut self,
        source: &MirSourceAnchor,
        occurrence: BoundExecutionSite,
        initial: bool,
    ) -> Result<MirBlockId, LoweringError> {
        let entry = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::CleanupBroadcast,
        )?;

        let plans = if initial {
            self.cleanup_plans(0, occurrence.node())
        } else {
            Vec::new()
        };

        let accesses = if initial {
            &[][..]
        } else {
            self.input
                .suspension(occurrence)
                .expect("suspended frame retains checked broadcast obligations")
                .cancellation_broadcast()
        };

        let mut owners = plans
            .iter()
            .flat_map(|plan| plan.cancellation_broadcast())
            .chain(accesses.iter().rev())
            .copied()
            .peekable();

        let temporaries = if initial {
            Vec::new()
        } else {
            self.input_cleanup(InputExit::All)
        };

        let mut temporaries = temporaries
            .iter()
            .rev()
            .filter(|temporary| temporary.requires_cleanup(MirCleanupPhase::TaskCancellation))
            .peekable();

        if owners.peek().is_none() && temporaries.peek().is_none() {
            self.set_terminator(
                entry,
                Self::retained_source(source),
                MirTerminatorKind::Return(None),
            )?;

            return Ok(entry);
        }

        let (outcome, finish) = self.frame_broadcast_outcome(source)?;

        outcome.begin(&mut self.builder, entry, source)?;

        // This callback has its own incident destinations. A suspended lifecycle
        // operation retains its original pending outcome and continuation.
        let previous = self.cleanup_outcome.replace(outcome);
        let failures = self.cleanup_failure_targets.take();
        let mut block = entry;

        for temporary in temporaries {
            block = self.push_input_cleanup_temporary(
                block,
                MirCleanupPhase::TaskCancellation,
                temporary,
            )?;
        }

        for access in owners {
            block = if initial {
                let place = self.place_for_access(access, false)?;

                // Captures are wholly initialized before the owner is published.
                // The body's guards have not executed in an inactive frame.
                self.push_guarded_cleanup(
                    block,
                    source,
                    MirCleanupPhase::TaskCancellation,
                    place,
                    None,
                    None,
                    false,
                    None,
                )?
                .0
            } else {
                self.push_cleanup_access(
                    block,
                    source,
                    MirCleanupPhase::TaskCancellation,
                    access,
                    false,
                    None,
                )?
                .0
            };
        }

        self.set_terminator(
            block,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(finish, [])),
        )?;

        self.cleanup_outcome = previous;
        self.cleanup_failure_targets = failures;

        Ok(entry)
    }

    fn frame_broadcast_outcome(
        &mut self,
        source: &MirSourceAnchor,
    ) -> Result<(CleanupOutcome, MirBlockId), LoweringError> {
        if let Some((outcome, finish)) = &self.broadcast_outcome {
            // Every state uses the same callback-local storage and completion tail.
            return Ok((outcome.clone(), *finish));
        }

        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;
        let report = self.representation_type(RepresentationRole::PanicReport)?;
        let unit = self.representation_type(RepresentationRole::Unit)?;

        let outcome = CleanupOutcome::allocate(
            &mut self.builder,
            source,
            boolean,
            report,
            unit,
            self.input.target().runtime_abi(),
            bray_ir::MirStorageKind::BroadcastLocal,
        )?;

        let finish = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::CleanupBroadcast,
        )?;

        let panicked = self.broadcast_terminal(
            source,
            CleanupDestination::PropagatePanic,
            Some(outcome.report()),
        )?;

        let cancelled =
            self.broadcast_terminal(source, CleanupDestination::PropagateCancellation, None)?;

        let completed = outcome.dispatch(&mut self.builder, finish, source, panicked, cancelled)?;

        self.set_terminator(
            completed,
            Self::retained_source(source),
            MirTerminatorKind::Return(None),
        )?;

        // The retained descriptor shares immutable paths with subsequent state entries.
        self.broadcast_outcome = Some((outcome.clone(), finish));

        Ok((outcome, finish))
    }

    fn broadcast_terminal(
        &mut self,
        source: &MirSourceAnchor,
        destination: CleanupDestination,
        value: Option<MirOperand>,
    ) -> Result<MirBlockId, LoweringError> {
        let block = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::CleanupBroadcast,
        )?;

        self.set_destination(block, source, destination, value)?;

        Ok(block)
    }
}
