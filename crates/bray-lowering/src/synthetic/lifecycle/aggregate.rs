use bray_ir::{MirPlace, MirProjectionKind};

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(super) fn push_array_lifecycle(
        &mut self,
        builder: &mut bray_ir::MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &bray_ir::MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        element: bray_symbols::TypeId,
        length: bray_symbols::ConstantTermId,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let length = self.context.array_length(length)?;

        if !builder.target().machine().fits_usize(u128::from(length)) {
            return Err(SyntheticLoweringError::LayoutOverflow(place.ty()).into());
        }

        if length == 0 {
            return Ok(block);
        }

        let integer = self
            .context
            .representation_type(bray_compiler_known::RepresentationRole::ScalarUsize)?;

        let length =
            crate::operand::integer_constant(self.context.semantic_values(), integer, length)
                .map_err(SyntheticLoweringError::SemanticValue)?;

        self.push_sequence_lifecycle(builder, block, source, role, place, element, length, None)
    }

    pub(super) fn push_sequence_lifecycle(
        &mut self,
        builder: &mut bray_ir::MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &bray_ir::MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        element: bray_symbols::TypeId,
        length: bray_ir::MirOperand,
        progress: Option<bray_ir::MirOperand>,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let integer = builder.operand_type(&length);

        let boolean = self
            .context
            .representation_type(bray_compiler_known::RepresentationRole::ScalarBool)?;

        let constant = |value| {
            crate::operand::integer_constant(self.context.semantic_values(), integer, value)
                .map_err(SyntheticLoweringError::SemanticValue)
        };

        let phase = match role {
            bray_ir::MirGeneratedLifecycleRole::Destroy(
                bray_ir::MirDestructionDisposition::Ordinary
                | bray_ir::MirDestructionDisposition::Dynamic,
            )
            | bray_ir::MirGeneratedLifecycleRole::ReleaseBuffer => {
                bray_bound_tree::LifecyclePhase::Resolve
            }
            role => role.phase(),
        };

        if matches!(
            self.context.lifecycle_action(element, phase)?,
            bray_bound_tree::LifecycleAction::None
        ) {
            if let Some(buffer) = progress {
                let retirement =
                    super::storage::buffer_cleanup_progress(builder, buffer, constant(0)?);

                self.push_lifecycle_operation(builder, block, source, retirement)?;
            }

            return Ok(block);
        }

        let outcome = self.cleanup_outcome(builder, block, source)?;

        let cleanup = crate::cleanup_loop::ReverseCleanupLoop::new(
            builder,
            block,
            source,
            length,
            boolean,
            [constant(0)?, constant(1)?],
            None,
        )
        .map_err(|cause| self.capacity_error(cause))?;

        let retained = if progress.is_none()
            && self.frame.is_some()
            && matches!(role, bray_ir::MirGeneratedLifecycleRole::Destroy(_))
            && !matches!(
                self.context
                    .lifecycle_action(element, bray_bound_tree::LifecyclePhase::Cancel)?,
                bray_bound_tree::LifecycleAction::None
            ) {
            let broadcast = builder
                .push_block(source.clone(), bray_ir::MirBlockKind::CleanupBroadcast)
                .map_err(|cause| self.capacity_error(cause))?;

            let end = self.push_sequence_lifecycle(
                builder,
                broadcast,
                source,
                bray_ir::MirGeneratedLifecycleRole::Cleanup(
                    bray_ir::MirCleanupPhase::TaskCancellation,
                ),
                place.clone(),
                element,
                bray_ir::MirOperand::Copy(cleanup.counter.clone()),
                None,
            )?;

            let collection = outcome.collection();

            let retention = collection
                .retention()
                .expect("array cleanup retains its source contract");

            builder.set_terminator(
                end,
                source.clone(),
                bray_ir::MirTerminatorKind::Goto(bray_ir::MirEdge::new(retention.broadcast(), [])),
            );

            outcome.clone().with_retention(Some(std::sync::Arc::new(
                retention.as_ref().clone().with_broadcast(broadcast),
            )))
        } else {
            outcome.clone()
        };

        // The loop owns the counter while its indexed child retains the same storage identity.
        let child = place.project(
            MirProjectionKind::Index(bray_ir::MirOperand::Copy(cleanup.counter.clone())),
            element,
        );

        let retirement = progress.map(|buffer| {
            super::storage::buffer_cleanup_progress(
                builder,
                buffer,
                bray_ir::MirOperand::Copy(cleanup.counter.clone()),
            )
        });

        let completed = self.push_child_lifecycle_operation(
            builder,
            cleanup.body,
            source,
            role,
            child,
            &retained,
            retirement,
        )?;

        cleanup
            .close(builder, completed, source, None)
            .map_err(|cause| self.capacity_error(cause))?;

        self.finish_cleanup_outcome(builder, cleanup.continuation, source, &outcome, &mut None)
    }
}
