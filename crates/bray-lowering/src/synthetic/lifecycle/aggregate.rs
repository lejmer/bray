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
            constant(length)?,
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

        let operations = super::representation::child_lifecycle_operations(role, child);

        let completed = outcome
            .resolve(
                builder,
                cleanup.body,
                source,
                operations.into_iter().flatten(),
            )
        .map_err(|cause| self.capacity_error(cause))?;

        cleanup
            .close(builder, completed, source, None)
        .map_err(|cause| self.capacity_error(cause))?;

        self.finish_cleanup_outcome(builder, cleanup.continuation, source, &outcome)
    }
}
