use bray_bound_tree::{AnyBoundNodeId, AsyncScopeExitPlan};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge, MirOperand,
    MirSourceAnchor, MirTerminatorKind,
};

use super::control::{CleanupDestination, TerminalState};
use crate::cleanup_outcome::CleanupOutcome;
use crate::lowering::LoweringError;
use crate::lowering::inputs::InputExit;
use crate::lowering::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(in crate::lowering) fn create_cleanup_outcome(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<CleanupOutcome, LoweringError> {
        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;
        let report = self.representation_type(RepresentationRole::PanicReport)?;
        let unit = self.representation_type(RepresentationRole::Unit)?;

        CleanupOutcome::new(
            &mut self.builder,
            block,
            source,
            boolean,
            report,
            unit,
            self.input.target().runtime_abi(),
        )
        .map_err(Into::into)
    }

    pub(in crate::lowering) fn suspension_cleanup_edge(
        &mut self,
        source: &MirSourceAnchor,
        exit: AnyBoundNodeId,
    ) -> Result<MirCleanupEdge, LoweringError> {
        let plans = self.cleanup_plans(0, exit)?;

        let broadcast = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::CleanupBroadcast,
        )?;

        let outcome = self.create_cleanup_outcome(broadcast, source)?;

        outcome.initialize_cancellation(&mut self.builder, broadcast, source)?;
        self.cleanup_outcome = Some(outcome);

        let lifecycle = self.resolve_cleanup(
            broadcast,
            source,
            &plans,
            None,
            &std::collections::BTreeMap::new(),
            InputExit::All,
        )?;

        let outcome = self
            .cleanup_outcome
            .take()
            .ok_or(LoweringError::SemanticValueUnavailable)?;

        let terminal = self.terminal_state_block(source, TerminalState::Cancelled)?;

        self.finish_cancelled_cleanup(
            lifecycle,
            source,
            outcome,
            CleanupDestination::Goto(terminal),
        )?;

        Ok(MirCleanupEdge::new(
            MirCleanupPhase::TaskCancellation,
            MirEdge::new(broadcast, []),
        ))
    }

    pub(super) fn finish_abnormal_cleanup(
        &mut self,
        current: MirBlockId,
        source: &MirSourceAnchor,
        report: Option<MirOperand>,
        destination: CleanupDestination,
        plans: &[AsyncScopeExitPlan],
        abandoned: Option<&bray_ir::MirPlace>,
    ) -> Result<(), LoweringError> {
        let outcome = self.create_cleanup_outcome(current, source)?;
        let panicking = report.is_some();

        if let Some(report) = report {
            outcome.initialize_panic(&mut self.builder, current, source, report)?;
        } else {
            outcome.initialize_cancellation(&mut self.builder, current, source)?;
        }

        let broadcast = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::CleanupBroadcast,
        )?;

        let cleanup = MirCleanupEdge::new(
            MirCleanupPhase::TaskCancellation,
            MirEdge::new(broadcast, []),
        );

        let entry = if panicking {
            MirTerminatorKind::Panic {
                report: outcome.report(),
                cleanup,
            }
        } else {
            MirTerminatorKind::CancelCurrentRun { cleanup }
        };

        self.set_terminator(current, Self::retained_source(source), entry)?;
        self.cleanup_outcome = Some(outcome);

        let lifecycle = self.resolve_cleanup(
            broadcast,
            source,
            plans,
            abandoned,
            &std::collections::BTreeMap::new(),
            if panicking {
                self.abnormal_input_exit(destination)
            } else {
                InputExit::All
            },
        )?;

        let outcome = self
            .cleanup_outcome
            .take()
            .ok_or(LoweringError::SemanticValueUnavailable)?;

        if panicking {
            outcome.end_shield(&mut self.builder, lifecycle, source)?;

            return self.set_destination(lifecycle, source, destination, Some(outcome.report()));
        }

        self.finish_cancelled_cleanup(lifecycle, source, outcome, destination)
    }

    pub(super) fn abnormal_input_exit(&self, destination: CleanupDestination) -> InputExit {
        if let CleanupDestination::Goto(block) = destination
            && let Some(index) = self
                .catch_targets
                .iter()
                .position(|target| target.block == block)
        {
            return InputExit::Catch(index + 1);
        }

        InputExit::All
    }

    pub(super) fn resolve_cleanup(
        &mut self,
        mut broadcast: MirBlockId,
        source: &MirSourceAnchor,
        plans: &[AsyncScopeExitPlan],
        abandoned: Option<&bray_ir::MirPlace>,
        failures: &std::collections::BTreeMap<
            bray_bound_tree::BoundBlockId,
            (MirBlockId, MirBlockId, bray_symbols::TypeId),
        >,
        input_exit: InputExit,
    ) -> Result<MirBlockId, LoweringError> {
        let temporaries = self.input_cleanup(input_exit);

        let mut lifecycle = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        if let Some(place) = abandoned {
            broadcast = self
                .push_guarded_cleanup(
                    broadcast,
                    source,
                    MirCleanupPhase::TaskCancellation,
                    Self::retained_place(place),
                    None,
                    None,
                    false,
                    None,
                )?
                .0;
        }

        let (broadcast, _) = self.push_cleanup_operations(
            broadcast,
            source,
            MirCleanupPhase::TaskCancellation,
            plans,
            &temporaries,
            None,
            failures,
        )?;

        self.set_terminator(
            broadcast,
            Self::retained_source(source),
            MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                MirCleanupPhase::LifecycleResolution,
                MirEdge::new(lifecycle, []),
            )),
        )?;

        if let Some(place) = abandoned {
            lifecycle = self
                .push_guarded_cleanup(
                    lifecycle,
                    source,
                    MirCleanupPhase::LifecycleResolution,
                    Self::retained_place(place),
                    None,
                    None,
                    false,
                    None,
                )?
                .0;
        }

        self.push_cleanup_operations(
            lifecycle,
            source,
            MirCleanupPhase::LifecycleResolution,
            plans,
            &temporaries,
            None,
            failures,
        )
        .map(|(block, _)| block)
    }

    fn finish_cancelled_cleanup(
        &mut self,
        lifecycle: MirBlockId,
        source: &MirSourceAnchor,
        outcome: CleanupOutcome,
        destination: CleanupDestination,
    ) -> Result<(), LoweringError> {
        let panicked = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let cancelled = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let completed =
            outcome.dispatch(&mut self.builder, lifecycle, source, panicked, cancelled)?;

        self.set_terminator(
            completed,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(cancelled, [])),
        )?;

        self.set_destination(cancelled, source, destination, None)?;

        let panic_destination = if self.input.unit_kind().protected_frame().is_some() {
            let report = self.representation_type(RepresentationRole::PanicReport)?;

            CleanupDestination::Goto(
                self.terminal_state_block(source, TerminalState::Panicked(report))?,
            )
        } else {
            CleanupDestination::PropagatePanic
        };

        self.set_destination(panicked, source, panic_destination, Some(outcome.report()))
    }
}
