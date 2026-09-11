use bray_ir::{MirPlace, MirProjectionKind};
use bray_symbols::{NamedTypeSymbolId, TypeData};

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    #[expect(
        clippy::too_many_arguments,
        reason = "array traversal retains its checked element type and fixed length"
    )]
    pub(super) fn push_array_lifecycle(
        &self,
        builder: &mut bray_ir::MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &bray_ir::MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        element: bray_symbols::TypeId,
        length: bray_symbols::ConstantTermId,
        outcome: &crate::cleanup_outcome::CleanupOutcome,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let length = self.context.array_length(length)?;

        if !builder.target().machine().fits_usize(u128::from(length)) {
            return Err(SyntheticLoweringError::LayoutOverflow(place.ty()).into());
        }

        if length == 0 {
            return Ok(block);
        }

        let values = self.context.semantic_values();

        let integer = self
            .context
            .representation_type(bray_compiler_known::RepresentationRole::ScalarUsize)?;

        let boolean = self
            .context
            .representation_type(bray_compiler_known::RepresentationRole::ScalarBool)?;

        let constant = |value| {
            crate::operand::integer_constant(values, integer, value)
                .map_err(SyntheticLoweringError::SemanticValue)
        };

        let cleanup = crate::cleanup_loop::ReverseCleanupLoop::new(
            builder,
            block,
            source,
            constant(length)?,
            boolean,
            [constant(0)?, constant(1)?],
            None,
        )
        .map_err(|cause| self.mir_error(source, cause))?;

        let child = place.project(
            MirProjectionKind::Index(bray_ir::MirOperand::Copy(cleanup.counter.clone())),
            element,
        );

        let mut completed = cleanup.body;

        for operation in super::representation::child_lifecycle_operations(role, child)?
            .into_iter()
            .flatten()
        {
            completed =
                self.resolve_lifecycle_action(builder, completed, source, operation, outcome)?;
        }

        cleanup
            .close(builder, completed, source, None)
            .map_err(|cause| self.mir_error(source, cause))?;

        Ok(cleanup.continuation)
    }

    pub(super) fn lifecycle_children(&self, place: MirPlace) -> Result<Vec<MirPlace>, C::Error> {
        let values = self.context.semantic_values();

        let data = values
            .type_data(place.ty())
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let children = match data.as_ref() {
            TypeData::Named {
                definition: NamedTypeSymbolId::Struct(structure),
                substitution,
            } => {
                if self
                    .context
                    .representation_role(NamedTypeSymbolId::Struct(*structure))
                    .is_some()
                {
                    if self.context.cleanup_type_execution(place.ty())?.cleanup()
                        == bray_bound_tree::AsyncStorageCleanupRequirement::None
                    {
                        return Ok(Vec::new());
                    }

                    return Err(SyntheticLoweringError::UnsupportedType(place.ty()).into());
                }

                let representation = self
                    .context
                    .declared_representation(NamedTypeSymbolId::Struct(*structure))?;

                let bray_symbols::DeclaredStorageShape::Structure(members) =
                    representation.storage()
                else {
                    return Err(SyntheticLoweringError::UnresolvedType(place.ty()).into());
                };

                members
                    .iter()
                    .enumerate()
                    .map(|(index, member)| {
                        let ty = self.context.resolve_type(member.ty(), *substitution)?;

                        let projection = MirProjectionKind::TupleField(
                            u32::try_from(index)
                                .map_err(|_| SyntheticLoweringError::LayoutOverflow(place.ty()))?,
                        );

                        Ok((projection, ty))
                    })
                    .collect::<Result<Vec<_>, C::Error>>()?
            }
            TypeData::Tuple(elements) => elements
                .iter()
                .copied()
                .enumerate()
                .map(|(index, ty)| {
                    let index = u32::try_from(index)
                        .map_err(|_| SyntheticLoweringError::LayoutOverflow(place.ty()))?;

                    Ok((MirProjectionKind::TupleField(index), ty))
                })
                .collect::<Result<Vec<_>, C::Error>>()?,
            TypeData::Error
            | TypeData::Array { .. }
            | TypeData::Named {
                definition: NamedTypeSymbolId::Union(_),
                ..
            }
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. }
            | TypeData::FlexibleArray(_)
            | TypeData::Slice(_)
            | TypeData::Generator(_)
            | TypeData::Nullable(_)
            | TypeData::Borrow { .. }
            | TypeData::TraitView(_)
            | TypeData::OwnedIndirection { .. }
            | TypeData::Callable(_) => {
                return Err(SyntheticLoweringError::UnsupportedType(place.ty()).into());
            }
        };

        Ok(children
            .into_iter()
            .map(|(kind, ty)| place.project(kind, ty))
            .collect())
    }
}
