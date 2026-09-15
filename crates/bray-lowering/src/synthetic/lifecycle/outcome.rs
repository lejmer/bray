use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge, MirOperationKind,
    MirRuntimeReference, MirSourceAnchor, MirTerminatorKind, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;

use super::super::{SyntheticLowerer, SyntheticLoweringContext};
use crate::cleanup_outcome::CleanupOutcome;

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(in crate::synthetic) fn cleanup_outcome(
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
        block: MirBlockId,
        source: &MirSourceAnchor,
        operations: impl IntoIterator<Item = MirOperationKind>,
    ) -> Result<MirBlockId, C::Error> {
        let mut operations = operations.into_iter().peekable();

        if operations.peek().is_none() {
            return Ok(block);
        }

        let outcome = self.cleanup_outcome(builder, block, source)?;

        let block = outcome
            .resolve(builder, block, source, operations)
            .map_err(|cause| self.mir_error(source, cause))?;

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

        builder
            .set_terminator(
                panicked,
                source.clone(),
                MirTerminatorKind::PropagatePanic {
                    report: outcome.report(),
                    runtime: MirRuntimeReference::new(RuntimeAbiRole::PanicPropagation, abi),
                },
            )
            .map_err(invalid)?;

        builder
            .set_terminator(
                cancelled,
                source.clone(),
                MirTerminatorKind::PropagateCancellation {
                    runtime: MirRuntimeReference::new(
                        RuntimeAbiRole::CurrentRunCancellationPropagation,
                        abi,
                    ),
                },
            )
            .map_err(invalid)?;

        Ok(completed)
    }

    pub(super) fn check_lifecycle_value(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        value: bray_ir::MirValueId,
        result: bray_symbols::TypeId,
    ) -> Result<(MirBlockId, bray_ir::MirValueId), C::Error> {
        let invalid = |cause| self.mir_error(source, cause);
        let kind = builder.block_kind(block).map_err(invalid)?;

        let report_type = self
            .context
            .representation_type(RepresentationRole::PanicReport)?;

        let completed = builder.push_block(source.clone(), kind).map_err(invalid)?;

        let value_parameter = builder
            .push_block_parameter(completed, source.clone(), result)
            .map_err(invalid)?;

        let panicked = builder.push_block(source.clone(), kind).map_err(invalid)?;

        let report = builder
            .push_block_parameter(panicked, source.clone(), report_type)
            .map_err(invalid)?;

        let cancelled = builder.push_block(source.clone(), kind).map_err(invalid)?;

        builder
            .set_terminator(
                block,
                source.clone(),
                MirTerminatorKind::CheckCallOutcome {
                    completed: MirEdge::new(completed, [bray_ir::MirOperand::Value(value)]),
                    panicked: bray_ir::MirCallPanicEdge::new(panicked, report_type),
                    cancelled: MirEdge::new(cancelled, []),
                },
            )
            .map_err(invalid)?;

        let panicked = self.cleanup_propagation_block(builder, panicked, source)?;
        let cancelled = self.cleanup_propagation_block(builder, cancelled, source)?;
        let abi = builder.target().runtime_abi();

        builder
            .set_terminator(
                panicked,
                source.clone(),
                MirTerminatorKind::PropagatePanic {
                    report: bray_ir::MirOperand::Value(report),
                    runtime: MirRuntimeReference::new(RuntimeAbiRole::PanicPropagation, abi),
                },
            )
            .map_err(invalid)?;

        builder
            .set_terminator(
                cancelled,
                source.clone(),
                MirTerminatorKind::PropagateCancellation {
                    runtime: MirRuntimeReference::new(
                        RuntimeAbiRole::CurrentRunCancellationPropagation,
                        abi,
                    ),
                },
            )
            .map_err(invalid)?;

        Ok((completed, value_parameter))
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
}
