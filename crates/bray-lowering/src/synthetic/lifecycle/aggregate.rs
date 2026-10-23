use bray_ir::{MirPlace, MirProjectionKind};

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(super) fn push_array_lifecycle(
        &self,
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

        self.push_sequence_lifecycle(builder, block, source, role, place, element, length)
    }

    pub(super) fn push_sequence_lifecycle(
        &self,
        builder: &mut bray_ir::MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &bray_ir::MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        element: bray_symbols::TypeId,
        length: bray_ir::MirOperand,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let integer = builder.operand_type(&length);

        let boolean = self
            .context
            .representation_type(bray_compiler_known::RepresentationRole::ScalarBool)?;

        let constant = |value| {
            crate::operand::integer_constant(self.context.semantic_values(), integer, value)
                .map_err(SyntheticLoweringError::SemanticValue)
        };

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

        // The loop owns the counter while its indexed child retains the same storage identity.
        let child = place.project(
            MirProjectionKind::Index(bray_ir::MirOperand::Copy(cleanup.counter.clone())),
            element,
        );

        let completed = self.push_child_lifecycle_operation(
            builder,
            cleanup.body,
            source,
            role,
            child,
            &outcome,
        )?;

        cleanup
            .close(builder, completed, source, None)
            .map_err(|cause| self.capacity_error(cause))?;

        self.finish_cleanup_outcome(builder, cleanup.continuation, source, &outcome, &mut None)
    }
}
