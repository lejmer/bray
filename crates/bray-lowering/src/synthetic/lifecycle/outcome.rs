use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge, MirOperationKind, MirPlace,
    MirRuntimeReference, MirSourceAnchor, MirTerminatorKind, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;

use super::super::{SyntheticLowerer, SyntheticLoweringContext};
use crate::cleanup_outcome::CleanupOutcome;

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(crate) fn cleanup_outcome(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<CleanupOutcome, C::Error> {
        let boolean = self
            .context
            .representation_type(RepresentationRole::ScalarBool)?;

        let report = self
            .context
            .representation_type(RepresentationRole::PanicReport)?;

        let unit = self.context.representation_type(RepresentationRole::Unit)?;

        CleanupOutcome::new(
            builder,
            block,
            source,
            boolean,
            report,
            unit,
            builder.target().runtime_abi(),
        )
        .map_err(|cause| self.mir_error(source, cause))
    }

    pub(in crate::synthetic) fn resolve_lifecycle_sequence(
        &self,
        builder: &mut MirUnitBuilder,
        mut block: MirBlockId,
        source: &MirSourceAnchor,
        operations: impl IntoIterator<Item = MirOperationKind>,
    ) -> Result<MirBlockId, C::Error> {
        let mut operations = operations.into_iter().peekable();

        if operations.peek().is_none() {
            return Ok(block);
        }

        let outcome = self.cleanup_outcome(builder, block, source)?;

        for operation in operations {
            block = self.resolve_lifecycle_action(builder, block, source, operation, &outcome)?;
        }

        self.finish_cleanup_outcome(builder, block, source, &outcome)
    }

    pub(in crate::synthetic) fn finish_cleanup_outcome(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        outcome: &CleanupOutcome,
    ) -> Result<MirBlockId, C::Error> {
        let invalid = |cause| self.mir_error(source, cause);
        let kind = builder.block_kind(block).map_err(invalid)?;

        // Each completion block independently retains generated-body provenance.
        let panicked = builder.push_block(source.clone(), kind).map_err(invalid)?;
        let cancelled = builder.push_block(source.clone(), kind).map_err(invalid)?;
        let abi = builder.target().runtime_abi();

        let completed = outcome
            .dispatch(builder, block, source, panicked, cancelled)
            .map_err(invalid)?;

        let panicked = self.cleanup_propagation_block(builder, panicked, source)?;
        let cancelled = self.cleanup_propagation_block(builder, cancelled, source)?;

        for (block, termination, state) in [
            (
                panicked,
                MirTerminatorKind::PropagatePanic {
                    report: outcome.report(),
                    runtime: MirRuntimeReference::new(RuntimeAbiRole::PanicPropagation, abi),
                },
                bray_ir::MirTaskTerminalState::Panicked(outcome.report()),
            ),
            (
                cancelled,
                MirTerminatorKind::PropagateCancellation {
                    runtime: MirRuntimeReference::new(
                        RuntimeAbiRole::CurrentRunCancellationPropagation,
                        abi,
                    ),
                },
                bray_ir::MirTaskTerminalState::Cancelled,
            ),
        ] {
            if builder.protected_frame().is_some() {
                // A cleanup continuation reports failure to its awaiting owner through the frame
                // protocol. The synchronous propagation ABI has no caller outcome slot here.
                crate::cleanup_await::finish_cleanup_frame(builder, block, source, state)
                    .map_err(invalid)?;

                continue;
            }

            builder
                .set_terminator(block, source.clone(), termination)
                .map_err(invalid)?;
        }

        Ok(completed)
    }

    fn cleanup_propagation_block(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<MirBlockId, C::Error> {
        let invalid = |cause| self.mir_error(source, cause);

        if builder.block_kind(block).map_err(invalid)? != MirBlockKind::CleanupBroadcast {
            return Ok(block);
        }

        let terminal = builder
            .push_block(source.clone(), MirBlockKind::LifecycleResolution)
            .map_err(invalid)?;

        builder
            .set_terminator(
                block,
                source.clone(),
                MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                    MirCleanupPhase::LifecycleResolution,
                    MirEdge::new(terminal, []),
                )),
            )
            .map_err(invalid)?;

        Ok(terminal)
    }

    pub(crate) fn resolve_owner_completion(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        result: MirPlace,
        contract: (bray_ir::MirRunResultVariants, bray_symbols::TypeId),
        outcome: &crate::cleanup_outcome::CleanupOutcome,
    ) -> Result<MirBlockId, C::Error> {
        let (variants, completion) = contract;

        let (completed, finished, payload) = outcome
            .resolve_completion(builder, block, source, result, (variants, completion))
            .map_err(|cause| self.mir_error(source, cause))?;

        let cleanup = self.context.cleanup_type_execution(completion)?;

        // Cancellation and lifecycle resolution independently own the completed payload path.
        let completed = crate::cleanup_payload::broadcast_cancellation(
            builder,
            completed,
            source,
            payload.clone(),
            cleanup.cleanup(),
            outcome,
        )
        .map_err(|cause| self.mir_error(source, cause))?;

        let completed = self.resolve_lifecycle_action(
            builder,
            completed,
            source,
            MirOperationKind::Cleanup {
                phase: MirCleanupPhase::LifecycleResolution,
                place: payload,
            },
            outcome,
        )?;

        builder
            .set_terminator(
                completed,
                source.clone(),
                MirTerminatorKind::Goto(MirEdge::new(finished, [])),
            )
            .map_err(|cause| self.mir_error(source, cause))?;

        Ok(finished)
    }
}
