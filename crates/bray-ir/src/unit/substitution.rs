use std::collections::BTreeMap;

use crate::{
    MirBlockId, MirOperand, MirOperationKind, MirPlace, MirStorageId, MirTerminatorKind, MirValueId,
};

use super::local_id_remap::{
    MirLocalIdMapping, remap_operand_ids, remap_operation, remap_terminator,
};

struct Substitution<'a> {
    values: Option<&'a BTreeMap<MirValueId, MirOperand>>,
    places: Option<&'a BTreeMap<MirStorageId, MirPlace>>,
}

impl MirLocalIdMapping for Substitution<'_> {
    fn block(&self, old: MirBlockId) -> MirBlockId {
        old
    }

    fn storage(&self, old: MirStorageId) -> MirStorageId {
        old
    }

    fn value(&self, old: MirValueId) -> MirValueId {
        old
    }

    fn storage_place(&self, old: MirStorageId) -> Option<&MirPlace> {
        self.places.and_then(|places| places.get(&old))
    }

    fn remap_operand(&self, operand: &mut MirOperand) {
        if let MirOperand::Value(value) = operand
            && let Some(replacement) = self.values.and_then(|values| values.get(value))
        {
            // Each instruction owns its operand while paths retain shared immutable storage.
            *operand = replacement.clone();
        } else {
            remap_operand_ids(operand, self);
        }
    }
}

impl MirOperationKind {
    /// Replaces storage roots with existing places while preserving their projections.
    /// The caller must preserve type, initialization, authority and lifetime at every use.
    pub fn substitute_places(&mut self, replacements: &BTreeMap<MirStorageId, MirPlace>) {
        remap_operation(
            self,
            &Substitution {
                values: None,
                places: Some(replacements),
            },
        );
    }

    /// Replaces computed inputs, including projection selectors, with established equal operands.
    /// Replacements must dominate their uses and preserve the original value's type and ownership.
    pub fn substitute_values(&mut self, replacements: &BTreeMap<MirValueId, MirOperand>) {
        remap_operation(
            self,
            &Substitution {
                values: Some(replacements),
                places: None,
            },
        );
    }
}

impl MirTerminatorKind {
    /// Replaces storage roots with existing places, including edge and projection operands.
    /// The caller must preserve the source place's checked ownership and lifetime.
    pub fn substitute_places(&mut self, replacements: &BTreeMap<MirStorageId, MirPlace>) {
        remap_terminator(
            self,
            &Substitution {
                values: None,
                places: Some(replacements),
            },
        );
    }

    /// Replaces computed inputs and edge arguments with established equal operands.
    /// Replacements must dominate their uses and preserve the original value's type and ownership.
    pub fn substitute_values(&mut self, replacements: &BTreeMap<MirValueId, MirOperand>) {
        remap_terminator(
            self,
            &Substitution {
                values: Some(replacements),
                places: None,
            },
        );
    }
}
