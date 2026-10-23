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
    pub(in crate::lowering) fn frame_retention(
        &mut self,
        source: &MirSourceAnchor,
        occurrence: BoundExecutionSite,
    ) -> Result<bray_ir::MirFrameRetention, LoweringError> {
        let broadcast = self.frame_broadcast(source, occurrence, false)?;

        let mut initialized = match &self.completion_cleanup {
            Some(place) => vec![place.storage()],
            None => {
                let suspension = self.input.suspension(occurrence).unwrap_or_else(|| {
                    panic!("lowering contract violation: MissingSuspensionPoint {occurrence:?}")
                });

                self.retained_storages(suspension.retained_subjects())?
            }
        };

        for outcome in self
            .pending_reports
            .iter()
            .chain(
                self.catch_targets
                    .iter()
                    .filter_map(|catch| catch.reports.as_ref()),
            )
            .filter(|_| self.completion_cleanup.is_none())
        {
            initialized.extend(outcome.collection().places().map(|place| place.storage()));
        }

        Ok(bray_ir::MirFrameRetention::new(
            broadcast,
            self.execution_lane_requirements(),
            initialized,
        )
        .with_affinity(self.frame_affinity())
        .with_completion_cleanup(self.completion_cleanup.is_some()))
    }

    pub(super) fn cleanup_retention(
        &mut self,
        source: &MirSourceAnchor,
    ) -> Result<Option<std::sync::Arc<bray_ir::MirFrameRetention>>, LoweringError> {
        let Some(occurrence) = self.cleanup_occurrence.filter(|occurrence| {
            self.input.unit_kind().protected_frame().is_some()
                && (self.completion_cleanup.is_some()
                    || self.input.suspension(*occurrence).is_some())
        }) else {
            return Ok(None);
        };

        self.frame_retention(source, occurrence)
            .map(|retention| Some(std::sync::Arc::new(retention)))
    }

    pub(in crate::lowering) fn lifecycle_retention(
        &mut self,
        source: &MirSourceAnchor,
        invocation: bray_bound_tree::BoundCallResult,
    ) -> Result<Option<std::sync::Arc<bray_ir::MirFrameRetention>>, LoweringError> {
        match invocation {
            bray_bound_tree::BoundCallResult::Immediate(_) => self.cleanup_retention(source),
            bray_bound_tree::BoundCallResult::LazyFuture(_) => {
                // The just-published awaited state already owns the exact cleanup retention.
                Ok(Some(std::sync::Arc::new(
                    self.frame_states
                        .last()
                        .expect("awaited lifecycle call published its continuation")
                        .retention()
                        .clone(),
                )))
            }
        }
    }

    pub(in crate::lowering) fn frame_completion(
        &mut self,
        source: &MirSourceAnchor,
        result_type: bray_symbols::TypeId,
    ) -> Result<Option<bray_ir::MirFrameStateId>, LoweringError> {
        let Some(_) = self.input_cleanup_phases(result_type) else {
            return Ok(None);
        };

        assert!(
            self.input_temporaries.is_empty(),
            "completed source frame has no outstanding input temporaries"
        );

        let storage = self.builder.push_storage(
            Self::retained_source(source),
            bray_ir::MirStorageKind::FrameCompletion,
            result_type,
        )?;

        let place = bray_ir::MirPlace::new(storage, [], result_type);

        self.completion_cleanup = Some(Self::retained_place(&place));

        let occurrence = BoundExecutionSite::Cleanup(self.input.unit().root().into());
        let previous_occurrence = self.cleanup_occurrence.replace(occurrence);

        let entry = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::CleanupBroadcast,
        )?;

        let cancelled =
            self.terminal_state_block(source, super::control::TerminalState::Cancelled)?;

        self.finish_abnormal_cleanup(
            MirCleanupPhase::LifecycleResolution,
            entry,
            source,
            crate::cleanup_outcome::CleanupFailure::Cancellation(None),
            CleanupDestination::Goto(cancelled),
            &[],
            Some(&place),
        )?;

        let broadcast = self.frame_broadcast(source, occurrence, false)?;

        let state = self.next_frame_state()?;

        self.frame_states.push(bray_ir::MirFrameState::new(
            state,
            entry,
            bray_ir::MirFrameRetention::new(
                broadcast,
                self.execution_lane_requirements(),
                [storage],
            )
            .with_affinity(self.frame_affinity())
            .with_completion_cleanup(true),
        ));

        self.completion_cleanup = None;
        self.cleanup_occurrence = previous_occurrence;

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

        let accesses = if initial || self.completion_cleanup.is_some() {
            &[][..]
        } else {
            self.input
                .suspension(occurrence)
                .unwrap_or_else(|| {
                    panic!(
                        "suspended frame retains checked broadcast obligations: occurrence={occurrence:?}, unit={:?}",
                        self.input.unit().key()
                    )
                })
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

        let completion = self
            .completion_cleanup
            .as_ref()
            .filter(|place| {
                self.input_cleanup_phases(place.ty())
                    .is_some_and(|phases| phases.includes_cancellation())
            })
            .map(Self::retained_place);

        if owners.peek().is_none() && temporaries.peek().is_none() && completion.is_none() {
            self.set_terminator(
                entry,
                Self::retained_source(source),
                MirTerminatorKind::Return {
                    value: None,
                    report: None,
                },
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

        if let Some(place) = completion {
            block = self
                .push_guarded_cleanup(
                    block,
                    source,
                    MirCleanupPhase::TaskCancellation,
                    place,
                    None,
                    None,
                    false,
                    None,
                )?
                .0;
        }

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
        let states = self.cleanup_state_constants()?;

        let outcome = CleanupOutcome::allocate(
            &mut self.builder,
            source,
            boolean,
            report,
            unit,
            self.input.target().runtime_abi(),
            states,
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

        let cancelled = self.broadcast_terminal(
            source,
            CleanupDestination::PropagateCancellation,
            Some(outcome.report()),
        )?;

        let completed = outcome.dispatch(
            &mut self.builder,
            finish,
            source,
            panicked,
            cancelled,
            &mut None,
        )?;

        self.set_terminator(
            completed,
            Self::retained_source(source),
            MirTerminatorKind::Return {
                value: None,
                report: None,
            },
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
