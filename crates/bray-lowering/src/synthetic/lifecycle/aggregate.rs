use bray_ir::{MirPlace, MirProjectionKind};
use bray_symbols::{NamedTypeSymbolId, TypeData};

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
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
            TypeData::Array { element, length } => {
                let length = self.context.array_length(*length)?;

                let length = u32::try_from(length)
                    .map_err(|_| SyntheticLoweringError::LayoutOverflow(place.ty()))?;

                (0..length)
                    .map(|index| (MirProjectionKind::ElementFromStart(index), *element))
                    .collect()
            }
            TypeData::Error
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
