use bray_ir::{
    MirAbandonmentAction, MirAsyncOperation, MirBlockId, MirBlockKind, MirFrameDescriptor,
    MirFrameState, MirFrameStateId, MirOperationKind, MirPlace, MirRuntimeReference,
    MirSourceAnchor, MirTaskTerminalState, MirTerminatorKind, MirUnitBuilder,
};
use bray_runtime_interface::{
    ExecutionLaneRequirement, ProtectedFrameAbiVersions, ProtectedFrameAffinity,
};
use bray_symbols::TypeId;

use super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    #[expect(
        clippy::too_many_arguments,
        reason = "the descriptor combines its receiver, entry points, and checked execution lanes"
    )]
    pub(super) fn attach_frame_descriptor(
        &self,
        builder: &mut MirUnitBuilder,
        entry: MirBlockId,
        receiver: MirPlace,
        completion: TypeId,
        inactive_cleanup: MirBlockId,
        lane_requirements: &[ExecutionLaneRequirement],
        source: &MirSourceAnchor,
    ) -> Result<(), C::Error> {
        let frame = builder.protected_frame().ok_or_else(|| {
            self.mir_error(source, bray_ir::MirUnitBuildError::ProtectedFrameMismatch)
        })?;

        let quiescence = self.lower_frame_capture_abandonment(
            builder,
            source,
            &receiver,
            MirAbandonmentAction::Quiesce,
        )?;

        let destruction = self.lower_frame_capture_abandonment(
            builder,
            source,
            &receiver,
            MirAbandonmentAction::Destroy,
        )?;

        let mut entries = builder.suspension_states().collect::<Vec<_>>();

        entries.push((MirFrameStateId::new(0), entry));
        entries.sort_unstable_by_key(|(state, _)| *state);

        // This unit owns one generated cleanup body. Keep its conditional outcome slots and
        // traversal counters with the receiver across nested cleanup suspensions.
        let execution = bray_ir::MirFrameExecutionState::new(
            lane_requirements.iter().copied(),
            builder.storage_ids(),
        )
        .with_affinity(ProtectedFrameAffinity::OriginThread);

        let states = entries
            .into_iter()
            .map(|(state, entry)| MirFrameState::new(state, entry, execution.clone()));

        let abi = builder.target().runtime_abi();

        let descriptor = MirFrameDescriptor::try_new(
            frame,
            abi,
            ProtectedFrameAbiVersions::uniform(abi),
            completion,
            states,
        )
        .map_err(SyntheticLoweringError::FrameDescriptor)?;

        builder
            .set_frame_descriptor(
                descriptor
                    .with_inactive_cleanup(inactive_cleanup)
                    .with_capture_abandonment(quiescence, destruction),
            )
            .map_err(|cause| self.mir_error(source, cause))
    }

    fn lower_frame_capture_abandonment(
        &self,
        builder: &mut MirUnitBuilder,
        source: &MirSourceAnchor,
        receiver: &MirPlace,
        action: MirAbandonmentAction,
    ) -> Result<MirBlockId, C::Error> {
        let invalid = |cause| self.mir_error(source, cause);

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .map_err(invalid)?;

        let cleanup = self.context.cleanup_type_execution(receiver.ty())?;

        let block = if matches!(
            cleanup.cleanup(),
            bray_bound_tree::AsyncStorageCleanupRequirement::None
        ) {
            entry
        } else {
            let block = self.lifecycle_resolution_block(builder, entry, source)?;

            self.push_abandonment_operations(
                builder,
                block,
                source,
                action,
                receiver.clone(),
                builder.target().runtime_abi(),
            )?
        };

        builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Async(MirAsyncOperation::PublishTerminalState {
                    state: MirTaskTerminalState::CapturesCompleted,
                    runtime: MirRuntimeReference::new(
                        bray_runtime_interface::RuntimeAbiRole::TerminalPublication,
                        builder.target().runtime_abi(),
                    ),
                }),
                None,
            )
            .map_err(invalid)?;

        builder
            .set_terminator(block, source.clone(), MirTerminatorKind::Return(None))
            .map_err(invalid)?;

        Ok(entry)
    }
}
