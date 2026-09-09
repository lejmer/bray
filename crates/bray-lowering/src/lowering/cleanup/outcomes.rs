use bray_bound_tree::{AnyBoundNodeId, AsyncScopeExitPlan};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge, MirOperand,
    MirSourceAnchor, MirTerminatorKind,
};

use super::control::{CleanupDestination, TerminalState};
use crate::cleanup_outcome::CleanupOutcome;
use crate::lowering::LoweringError;
use crate::lowering::construction::ConstructionExit;
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
            ConstructionExit::All,
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
                self.abnormal_construction_exit(destination)
            } else {
                ConstructionExit::All
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

    pub(super) fn abnormal_construction_exit(
        &self,
        destination: CleanupDestination,
    ) -> ConstructionExit {
        if let CleanupDestination::Goto(block) = destination
            && let Some(index) = self
                .catch_targets
                .iter()
                .position(|target| target.block == block)
        {
            return ConstructionExit::Catch(index + 1);
        }

        ConstructionExit::All
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
        construction_exit: ConstructionExit,
    ) -> Result<MirBlockId, LoweringError> {
        let retained = self.cleanup_retained_storages.len();
        let temporaries = self.construction_cleanup(construction_exit);

        self.cleanup_retained_storages.extend(
            temporaries
                .iter()
                .map(|temporary| temporary.place.storage()),
        );

        if let Some(place) = abandoned {
            self.cleanup_retained_storages.push(place.storage());
        }

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
                    None,
                    false,
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
                    None,
                    false,
                )?
                .0;
        }

        let result = self
            .push_cleanup_operations(
                lifecycle,
                source,
                MirCleanupPhase::LifecycleResolution,
                plans,
                &temporaries,
                None,
                failures,
            )
            .map(|(block, _)| block);

        self.cleanup_retained_storages.truncate(retained);

        result
    }

    pub(super) fn finish_cancelled_cleanup(
        &mut self,
        lifecycle: MirBlockId,
        source: &MirSourceAnchor,
        outcome: CleanupOutcome,
        destination: CleanupDestination,
    ) -> Result<(), LoweringError> {
        let completed = self.dispatch_cleanup_outcome(lifecycle, source, outcome, destination)?;

        self.set_destination(completed, source, destination, None)
    }

    pub(super) fn dispatch_cleanup_outcome(
        &mut self,
        lifecycle: MirBlockId,
        source: &MirSourceAnchor,
        outcome: CleanupOutcome,
        cancellation: CleanupDestination,
    ) -> Result<MirBlockId, LoweringError> {
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

        self.set_destination(cancelled, source, cancellation, None)?;

        let panic_destination = if self.input.unit_kind().protected_frame().is_some() {
            let report = self.representation_type(RepresentationRole::PanicReport)?;

            CleanupDestination::Goto(
                self.terminal_state_block(source, TerminalState::Panicked(report))?,
            )
        } else {
            CleanupDestination::PropagatePanic
        };

        self.set_destination(panicked, source, panic_destination, Some(outcome.report()))?;

        Ok(completed)
    }
}
