use bray_bound_tree::{PatternProjection, StorageProjection};
use bray_ir::{MirFieldReference, MirOperand, MirPlace, MirProjection, MirProjectionKind};
use bray_symbols::{TypeData, TypeId};

use super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn append_projection_dereferences(
        &self,
        mut source_type: TypeId,
        projections: &mut Vec<MirProjection>,
    ) -> TypeId {
        loop {
            let data = self.input.semantic_values().type_data(source_type);

            let TypeData::Borrow { target, .. } = data.as_ref() else {
                return source_type;
            };

            projections.push(MirProjection::new(
                MirProjectionKind::Dereference,
                source_type,
                *target,
            ));

            source_type = *target;
        }
    }
}

pub(super) fn projected_pattern_place(
    subject: &MirOperand,
    projection: PatternProjection,
    result_type: TypeId,
) -> Option<MirPlace> {
    let (MirOperand::Copy(place) | MirOperand::Move(place)) = subject else {
        return None;
    };

    let kind = static_projection_kind(projection.into())?;

    Some(place.project(kind, result_type))
}

pub(super) fn static_projection_kind(projection: StorageProjection) -> Option<MirProjectionKind> {
    match projection {
        StorageProjection::ProductField(field) => {
            Some(MirProjectionKind::Field(MirFieldReference::Struct(field)))
        }
        StorageProjection::TupleElement(ordinal) => {
            Some(MirProjectionKind::TupleField(ordinal.raw()))
        }
        StorageProjection::ElementFromStart(ordinal) => {
            Some(MirProjectionKind::ElementFromStart(ordinal.raw()))
        }
        StorageProjection::ElementFromEnd(ordinal) => {
            Some(MirProjectionKind::ElementFromEnd(ordinal.raw()))
        }
        StorageProjection::ActiveUnionPayloadField { variant, field } => {
            Some(MirProjectionKind::ActiveUnionPayloadField { variant, field })
        }
        StorageProjection::NullableValue => Some(MirProjectionKind::NullableValue),
        StorageProjection::OwnedTarget
        | StorageProjection::Element(_)
        | StorageProjection::SliceRange { .. } => None,
    }
}

pub(super) fn static_cleanup_projection_kind(
    projection: bray_bound_tree::StorageCleanupProjectionKind,
) -> Option<MirProjectionKind> {
    use bray_bound_tree::StorageCleanupProjectionKind;

    match projection {
        StorageCleanupProjectionKind::Component(component) => static_projection_kind(component),
        StorageCleanupProjectionKind::UnionPayloadElement { variant, ordinal } => {
            Some(MirProjectionKind::ActiveUnionPayloadElement { variant, ordinal })
        }
        StorageCleanupProjectionKind::OwnedTarget(_)
        | StorageCleanupProjectionKind::ArrayElements(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{projected_pattern_place, static_projection_kind};
    use bray_bound_tree::{PatternProjection, StorageProjection};
    use bray_ir::{
        MirOperand, MirPlace, MirProjection, MirProjectionKind, MirStorageId, MirUnitId,
    };
    use bray_symbols::SymbolOrdinal;

    #[test]
    fn hidden_union_cleanup_members_preserve_variant_and_ordinal_without_source_access() {
        let variant =
            bray_symbols::UnionVariantSymbolId::from_symbol_id(bray_symbols::SymbolId::new(7));

        let ordinal = SymbolOrdinal::new(3);

        let projection =
            bray_bound_tree::StorageCleanupProjectionKind::UnionPayloadElement { variant, ordinal };

        assert_eq!(
            super::static_cleanup_projection_kind(projection),
            Some(MirProjectionKind::ActiveUnionPayloadElement { variant, ordinal })
        );

        assert!(
            !projection.contains(StorageProjection::ActiveUnionPayloadField {
                variant,
                field: bray_symbols::UnionPayloadFieldSymbolId::from_symbol_id(
                    bray_symbols::SymbolId::new(8)
                ),
            })
        );
    }

    #[test]
    fn cleanup_places_retain_static_projection_paths() {
        assert_eq!(
            static_projection_kind(StorageProjection::TupleElement(SymbolOrdinal::new(2))),
            Some(MirProjectionKind::TupleField(2))
        );
    }

    #[test]
    fn owned_targets_require_a_policy_call_instead_of_a_static_projection() {
        let values = bray_symbols::SemanticValueStore::try_new().unwrap();

        let ty = values
            .intern_type(bray_symbols::TypeData::tuple([]))
            .unwrap();

        let subject = MirOperand::Move(MirPlace::new(
            MirStorageId::from_slot(MirUnitId::new(7), 0),
            [],
            ty,
        ));

        assert_eq!(static_projection_kind(StorageProjection::OwnedTarget), None);

        assert_eq!(
            projected_pattern_place(&subject, PatternProjection::OwnedTarget, ty),
            None
        );
    }

    #[test]
    fn pattern_field_moves_retain_the_exact_nested_place() {
        let values = bray_symbols::SemanticValueStore::try_new().unwrap();

        let ty = values
            .intern_type(bray_symbols::TypeData::Tuple([].into()))
            .unwrap();

        let storage = MirStorageId::from_slot(MirUnitId::new(7), 0);
        let nullable = MirProjection::new(MirProjectionKind::NullableValue, ty, ty);
        let subject = MirOperand::Move(MirPlace::new(storage, [nullable.clone()], ty));

        assert_eq!(
            projected_pattern_place(
                &subject,
                PatternProjection::TupleElement(SymbolOrdinal::new(1)),
                ty
            ),
            Some(MirPlace::new(
                storage,
                [
                    nullable,
                    MirProjection::new(MirProjectionKind::TupleField(1), ty, ty)
                ],
                ty
            ))
        );
    }
}
