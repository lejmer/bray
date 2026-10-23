use std::collections::BTreeMap;

use bray_symbols::TypeId;

use super::super::local_id_remap::MirLocalIdMapping;
use crate::{
    MirBlockId, MirCapacityError, MirOperand, MirOperationId, MirStorageId, MirStorageKind,
    MirUnit, MirUnitBuilder, MirUnitId, MirValueId,
};

pub(super) fn concrete_type(ty: TypeId, types: &BTreeMap<TypeId, TypeId>) -> TypeId {
    types.get(&ty).copied().unwrap_or(ty)
}

pub(super) struct LocalIds<'a> {
    unit: MirUnitId,
    concrete_types: &'a BTreeMap<TypeId, TypeId>,
    pub(super) blocks: Vec<Option<MirBlockId>>,
    pub(super) storages: Vec<Option<MirStorageId>>,
    pub(super) values: Vec<Option<MirValueId>>,
    pub(super) parameters: BTreeMap<MirStorageId, MirValueId>,
    pub(super) places: BTreeMap<MirStorageId, crate::MirPlace>,
    pub(super) callables:
        Option<&'a BTreeMap<crate::MirCallableReference, crate::MirCallableReference>>,
}

impl<'a> LocalIds<'a> {
    pub(super) fn new(unit: &MirUnit, concrete_types: &'a BTreeMap<TypeId, TypeId>) -> Self {
        Self {
            unit: unit.unit(),
            concrete_types,
            blocks: vec![None; unit.blocks().len()],
            storages: vec![None; unit.storages().len()],
            values: vec![None; unit.values().len()],
            parameters: BTreeMap::new(),
            places: BTreeMap::new(),
            callables: None,
        }
    }

    fn resolve<T: Copy>(&self, unit: MirUnitId, index: Option<usize>, table: &[Option<T>]) -> T {
        assert_eq!(
            unit, self.unit,
            "MIR splice identity belongs to another unit"
        );

        table[index.expect("valid MIR local slot")].expect("MIR splice identity must be mapped")
    }
}

impl MirLocalIdMapping for LocalIds<'_> {
    fn block(&self, old: MirBlockId) -> MirBlockId {
        self.resolve(old.unit(), old.to_index(), &self.blocks)
    }
    fn storage(&self, old: MirStorageId) -> MirStorageId {
        self.resolve(old.unit(), old.to_index(), &self.storages)
    }
    fn value(&self, old: MirValueId) -> MirValueId {
        self.resolve(old.unit(), old.to_index(), &self.values)
    }

    fn storage_place(&self, old: MirStorageId) -> Option<&crate::MirPlace> {
        self.places.get(&old)
    }

    fn ty(&self, old: TypeId) -> TypeId {
        concrete_type(old, self.concrete_types)
    }

    fn callable(&self, old: crate::MirCallableReference) -> crate::MirCallableReference {
        self.callables
            .and_then(|callables| callables.get(&old))
            .copied()
            .unwrap_or(old)
    }

    fn remap_operand(&self, operand: &mut MirOperand) {
        if let MirOperand::Copy(place) | MirOperand::Move(place) = operand
            && place.projections().is_empty()
            && let Some(value) = self.parameters.get(&place.storage())
        {
            *operand = MirOperand::Value(*value);
            return;
        }

        super::super::local_id_remap::remap_operand_ids(operand, self);

        if let MirOperand::Constant { ty, .. }
        | MirOperand::ConstantTerm { ty, .. }
        | MirOperand::Immediate { ty, .. } = operand
        {
            *ty = concrete_type(*ty, self.concrete_types);
        }
    }
}

pub(super) fn copy_unit_layout<'unit>(
    unit: &MirUnit,
    ids: &mut LocalIds<'_>,
    builder: &mut MirUnitBuilder,
    blocks: impl Clone + Iterator<Item = (MirBlockId, &'unit crate::MirBlock)>,
    storages: impl Iterator<Item = (MirStorageId, &'unit crate::MirStorage)>,
    storage_kind: impl Fn(&MirStorageKind) -> MirStorageKind,
) -> Result<(), MirCapacityError> {
    for (old, block) in blocks.clone() {
        ids.blocks[old.to_index().expect("valid block")] =
            Some(builder.push_block(block.source().clone(), block.kind())?);
    }

    for (old, storage) in storages {
        let mapped = match ids.places.get(&old) {
            Some(place) => place.storage(),
            None => builder.push_storage(
                storage.source().clone(),
                storage_kind(storage.kind()),
                ids.ty(storage.ty()),
            )?,
        };

        ids.storages[old.to_index().expect("valid storage")] = Some(mapped);
    }

    for (old, block) in blocks {
        for parameter in block.parameters() {
            let value = unit.value(*parameter).expect("valid parameter");

            ids.values[parameter.to_index().expect("valid value")] =
                Some(builder.push_block_parameter(
                    ids.block(old),
                    value.source().clone(),
                    ids.ty(value.ty()),
                )?);
        }
    }

    Ok(())
}

pub(super) fn predict_results(
    unit: &MirUnit,
    ids: &mut LocalIds,
    blocks: impl Iterator<Item = MirBlockId>,
    skip: Option<MirOperationId>,
    next: &mut usize,
    output_unit: MirUnitId,
) -> Result<(), MirCapacityError> {
    for block in blocks {
        let block = unit.block(block).expect("selected block exists");

        for operation_id in block.operations() {
            if Some(*operation_id) == skip {
                continue;
            }

            if let Some(result) = unit
                .operation(*operation_id)
                .expect("valid operation")
                .result()
            {
                let slot =
                    u32::try_from(*next).map_err(|_| MirCapacityError::IdentityCapacityExceeded)?;

                ids.values[result.to_index().expect("valid result")] =
                    Some(MirValueId::from_slot(output_unit, slot));

                *next += 1;
            }
        }
    }

    Ok(())
}
