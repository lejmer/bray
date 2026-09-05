use std::collections::BTreeMap;

use bray_bound_tree::{
    AsyncStorageCleanupRequirement, CheckedAsync, StorageCleanupPart, StorageCleanupProjection,
    StorageCleanupType,
};
use bray_symbols::TypeId;

use super::{LoweringPlanFailure, LoweringPlanFailureCause};

pub(super) fn cleanup_type_index(
    analysis: &CheckedAsync,
) -> Result<BTreeMap<TypeId, &StorageCleanupType>, LoweringPlanFailure> {
    let mut types = BTreeMap::new();

    for shape in analysis.cleanup_types() {
        if types.insert(shape.ty(), shape).is_some() {
            return Err(LoweringPlanFailure::analysis(
                LoweringPlanFailureCause::Duplicate,
            ));
        }
    }

    Ok(types)
}

pub(super) fn verify_partition(
    types: &BTreeMap<TypeId, &StorageCleanupType>,
    ty: TypeId,
    parts: &[StorageCleanupPart],
    destructor_receiver: bool,
) -> Result<(), LoweringPlanFailureCause> {
    let mut remaining = parts;

    verify_subtree(
        types,
        ty,
        &mut remaining,
        &mut Vec::new(),
        destructor_receiver,
    )?;

    if !remaining.is_empty() {
        return Err(LoweringPlanFailureCause::Contradictory);
    }

    Ok(())
}

fn verify_subtree(
    types: &BTreeMap<TypeId, &StorageCleanupType>,
    ty: TypeId,
    remaining: &mut &[StorageCleanupPart],
    path: &mut Vec<StorageCleanupProjection>,
    destructor_receiver: bool,
) -> Result<(), LoweringPlanFailureCause> {
    let shape = types.get(&ty).ok_or(LoweringPlanFailureCause::Missing)?;

    if let AsyncStorageCleanupRequirement::Recovered(cause) = shape.cleanup() {
        return Err(LoweringPlanFailureCause::StorageRecovery(cause));
    }

    if !destructor_receiver {
        if let Some((part, rest)) = remaining.split_first()
            && part.projections() == path.as_slice()
            && part.release().is_none()
        {
            if shape.cleanup() != AsyncStorageCleanupRequirement::Cleanup(part.phases()) {
                return Err(LoweringPlanFailureCause::Contradictory);
            }

            *remaining = rest;

            return Ok(());
        }

        if shape.cleanup() == AsyncStorageCleanupRequirement::None {
            return Ok(());
        }

        if shape.requires_whole_value() {
            return Err(LoweringPlanFailureCause::Contradictory);
        }

        // A missing sibling cannot be replaced by traversing unrelated or recursive type storage.
        if remaining
            .first()
            .is_none_or(|part| !part.projections().starts_with(path))
        {
            return Err(LoweringPlanFailureCause::Missing);
        }
    }

    let components = shape
        .components()
        .ok_or(LoweringPlanFailureCause::Missing)?;

    for component in components.iter().rev() {
        if component.source_type() != ty {
            return Err(LoweringPlanFailureCause::Contradictory);
        }

        path.push(*component);

        verify_subtree(types, component.result_type(), remaining, path, false)?;

        path.pop();
    }

    if let Some(release) = shape.release() {
        let (part, rest) = remaining
            .split_first()
            .ok_or(LoweringPlanFailureCause::Missing)?;

        if part.projections() != path.as_slice()
            || part.release() != Some(release)
            || part.phases() != bray_bound_tree::AsyncCleanupPhases::Lifecycle
        {
            return Err(LoweringPlanFailureCause::Contradictory);
        }

        *remaining = rest;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bray_bound_tree::{
        AsyncCleanupPhases, AsyncStorageCleanupRequirement, StorageCleanupPart,
        StorageCleanupProjection, StorageCleanupType, StorageProjection,
    };
    use bray_symbols::{
        GenericOwnerId, GenericSubstitutionData, NamedTypeSymbolId, SemanticValueStore,
        StructFieldSymbolId, StructSymbolId, SymbolId, TypeData, TypeId,
    };

    use super::verify_partition;
    use crate::LoweringPlanFailureCause;

    #[test]
    fn cleanup_type_index_rejects_duplicate_type_identities() {
        let values = SemanticValueStore::try_new().unwrap();
        let ty = values.intern_type(TypeData::tuple([])).unwrap();
        let shape = StorageCleanupType::new(ty, AsyncStorageCleanupRequirement::None);

        for count in [0, 1, 2] {
            let analysis = bray_bound_tree::CheckedAsync::try_new(
                bray_bound_tree::BoundUnitId::new(1),
                bray_bound_tree::BoundUnitKind::CallableBody,
                [],
                [],
                [],
                [],
                vec![shape.clone(); count],
                [],
                false,
            )
            .unwrap();

            let result = super::cleanup_type_index(&analysis);

            if count == 2 {
                assert_eq!(
                    result.unwrap_err(),
                    super::LoweringPlanFailure::analysis(LoweringPlanFailureCause::Duplicate)
                );
            } else {
                assert_eq!(result.unwrap().len(), count);
            }
        }
    }

    fn named_type(values: &SemanticValueStore, index: u32) -> TypeId {
        let definition = StructSymbolId::from_symbol_id(SymbolId::new(index));
        let owner = GenericOwnerId::try_new(definition.into()).unwrap();

        let substitution = values
            .intern_generic_substitution(GenericSubstitutionData::try_new(owner, [], []).unwrap())
            .unwrap();

        values
            .intern_type(TypeData::Named {
                definition: NamedTypeSymbolId::Struct(definition),
                substitution,
            })
            .unwrap()
    }

    #[test]
    fn named_partitions_require_every_sibling_in_order_with_exact_types_and_phases() {
        let values = SemanticValueStore::try_new().unwrap();
        let root = named_type(&values, 1);
        let leaf = named_type(&values, 2);
        let other = named_type(&values, 3);
        let cleanup = AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Lifecycle);

        let projections = [10, 11].map(|field| {
            StorageCleanupProjection::new(
                StorageProjection::ProductField(StructFieldSymbolId::from_symbol_id(
                    SymbolId::new(field),
                )),
                root,
                leaf,
            )
        });

        let shapes = [
            StorageCleanupType::new(root, cleanup).with_components(projections, None, false),
            StorageCleanupType::new(leaf, cleanup),
        ];

        let types = shapes
            .iter()
            .map(|shape| (shape.ty(), shape))
            .collect::<BTreeMap<_, _>>();

        let parts = projections
            .into_iter()
            .rev()
            .map(|projection| StorageCleanupPart::new([projection], AsyncCleanupPhases::Lifecycle))
            .collect::<Vec<_>>();

        let wrong_type = StorageCleanupProjection::new(projections[1].projection(), root, other);
        let missing = Err(LoweringPlanFailureCause::Missing);
        let contradictory = Err(LoweringPlanFailureCause::Contradictory);

        for (parts, expected) in [
            (parts.clone(), Ok(())),
            (vec![parts[0].clone()], missing),
            (vec![parts[1].clone()], missing),
            (Vec::new(), missing),
            (vec![parts[1].clone(), parts[0].clone()], missing),
            (
                vec![
                    StorageCleanupPart::new([wrong_type], AsyncCleanupPhases::Lifecycle),
                    parts[1].clone(),
                ],
                missing,
            ),
            (
                vec![
                    StorageCleanupPart::new(
                        [projections[1]],
                        AsyncCleanupPhases::CancellationThenLifecycle,
                    ),
                    parts[1].clone(),
                ],
                contradictory,
            ),
            (
                vec![parts[0].clone(), parts[1].clone(), parts[1].clone()],
                contradictory,
            ),
        ] {
            assert_eq!(verify_partition(&types, root, &parts, false), expected);
        }

        assert_eq!(
            verify_partition(&BTreeMap::new(), root, &parts, false),
            missing
        );
    }

    #[test]
    fn declared_lifecycle_decomposition_is_restricted_to_its_own_receiver() {
        let values = SemanticValueStore::try_new().unwrap();
        let root = named_type(&values, 1);
        let leaf = named_type(&values, 2);
        let cleanup = AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Lifecycle);

        let projection = StorageCleanupProjection::new(
            StorageProjection::ProductField(StructFieldSymbolId::from_symbol_id(SymbolId::new(10))),
            root,
            leaf,
        );

        let shapes = [
            StorageCleanupType::new(root, cleanup).with_components([projection], None, true),
            StorageCleanupType::new(leaf, cleanup),
        ];

        let types = shapes
            .iter()
            .map(|shape| (shape.ty(), shape))
            .collect::<BTreeMap<_, _>>();

        let parts = [StorageCleanupPart::new(
            [projection],
            AsyncCleanupPhases::Lifecycle,
        )];

        assert_eq!(
            verify_partition(&types, root, &parts, false),
            Err(LoweringPlanFailureCause::Contradictory)
        );

        assert_eq!(verify_partition(&types, root, &parts, true), Ok(()));
    }

    #[test]
    fn trivial_members_need_no_cleanup_leaf_but_do_not_hide_nontrivial_siblings() {
        let values = SemanticValueStore::try_new().unwrap();
        let root = named_type(&values, 1);
        let leaf = named_type(&values, 2);
        let cleanup = AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Lifecycle);

        let projection = StorageCleanupProjection::new(
            StorageProjection::ProductField(StructFieldSymbolId::from_symbol_id(SymbolId::new(10))),
            root,
            leaf,
        );

        let shapes = [
            StorageCleanupType::new(root, cleanup).with_components([projection], None, true),
            StorageCleanupType::new(leaf, AsyncStorageCleanupRequirement::None),
        ];

        let types = shapes
            .iter()
            .map(|shape| (shape.ty(), shape))
            .collect::<BTreeMap<_, _>>();

        assert_eq!(verify_partition(&types, root, &[], true), Ok(()));

        assert_eq!(
            verify_partition(
                &types,
                root,
                &[StorageCleanupPart::new(
                    [projection],
                    AsyncCleanupPhases::Lifecycle
                )],
                true
            ),
            Err(LoweringPlanFailureCause::Contradictory)
        );
    }
}
