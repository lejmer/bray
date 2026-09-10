use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirAbandonmentAction, MirBlockId, MirGeneratedLifecycleRole, MirHelperReference,
    MirOperationKind, MirPlace, MirSourceAnchor, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiVersion;
use bray_symbols::{TypeAssociatedLifecycleSlot, TypeData};

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(super) fn lower_quiescence_body(
        &self,
        builder: &mut MirUnitBuilder,
        entry: MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        runtime_abi: RuntimeAbiVersion,
    ) -> Result<(), C::Error> {
        let broadcast = builder
            .push_block(source.clone(), bray_ir::MirBlockKind::CleanupBroadcast)
            .map_err(|cause| self.mir_error(source, cause))?;

        let lifecycle = builder
            .push_block(source.clone(), bray_ir::MirBlockKind::LifecycleResolution)
            .map_err(|cause| self.mir_error(source, cause))?;

        builder
            .set_terminator(
                entry,
                source.clone(),
                bray_ir::MirTerminatorKind::BeginCleanup(bray_ir::MirCleanupEdge::new(
                    bray_ir::MirCleanupPhase::TaskCancellation,
                    bray_ir::MirEdge::new(broadcast, []),
                )),
            )
            .map_err(|cause| self.mir_error(source, cause))?;

        // A terminal task can expose values that were unavailable to its enclosing broadcast.
        let broadcast = self.push_generated_lifecycle_operations(
            builder,
            broadcast,
            source,
            &MirHelperReference::Cleanup {
                phase: bray_ir::MirCleanupPhase::TaskCancellation,
                ty: place.ty(),
            },
            place.clone(),
            runtime_abi,
        )?;

        builder
            .set_terminator(
                broadcast,
                source.clone(),
                bray_ir::MirTerminatorKind::ContinueCleanup(bray_ir::MirCleanupEdge::new(
                    bray_ir::MirCleanupPhase::LifecycleResolution,
                    bray_ir::MirEdge::new(lifecycle, []),
                )),
            )
            .map_err(|cause| self.mir_error(source, cause))?;

        let end = self.push_abandonment_operations(
            builder,
            lifecycle,
            source,
            MirAbandonmentAction::Quiesce,
            place,
            runtime_abi,
        )?;

        self.finish_lifecycle_body(builder, end, source)
    }

    pub(in crate::synthetic) fn push_abandonment_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        action: MirAbandonmentAction,
        place: MirPlace,
        runtime_abi: RuntimeAbiVersion,
    ) -> Result<MirBlockId, C::Error> {
        let role = MirGeneratedLifecycleRole::Abandon(action);

        if action == MirAbandonmentAction::Destructor {
            return Err(SyntheticLoweringError::UnsupportedLifecycleRole(role).into());
        }

        let data = self
            .context
            .semantic_values()
            .type_data(place.ty())
            .map_err(SyntheticLoweringError::SemanticValue)?;

        if let TypeData::Named {
            definition,
            substitution,
        } = data.as_ref()
        {
            if let Some(element) = self
                .context
                .raw_buffer_element(*definition, *substitution)?
            {
                return self.push_buffer_lifecycle(builder, block, source, role, place, element);
            }

            match self.context.representation_role(*definition) {
                Some(RepresentationRole::Task) if action == MirAbandonmentAction::Quiesce => {
                    return self.push_task_quiescence(builder, block, source, place);
                }
                Some(RepresentationRole::Task) if action == MirAbandonmentAction::Destroy => {
                    return self.push_abandoned_task_destruction(builder, block, source, place);
                }
                Some(RepresentationRole::Future) if action == MirAbandonmentAction::Quiesce => {
                    return self.quiesce_inactive_future(builder, block, source, place);
                }
                Some(RepresentationRole::Future) if action == MirAbandonmentAction::Destroy => {
                    return self.destroy_inactive_captures(builder, block, source, place);
                }
                Some(RepresentationRole::String | RepresentationRole::PanicReport) => {
                    return if action == MirAbandonmentAction::Destroy {
                        self.push_generated_lifecycle_operations(
                            builder,
                            block,
                            source,
                            &MirHelperReference::Destroy(place.ty()),
                            place,
                            runtime_abi,
                        )
                    } else {
                        Ok(block)
                    };
                }
                _ => {}
            }

            if action == MirAbandonmentAction::Destroy
                && self
                    .context
                    .lifecycle_callable(place.ty(), TypeAssociatedLifecycleSlot::Destructor)?
                    .is_some()
            {
                return self.resolve_lifecycle_sequence(
                    builder,
                    block,
                    source,
                    [MirOperationKind::Abandon {
                        action: MirAbandonmentAction::Destructor,
                        place,
                    }],
                );
            }
        }

        self.push_represented_lifecycle_operations(builder, block, source, role, place)
    }
}
