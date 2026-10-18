use std::collections::BTreeMap;

use crate::{
    MirBlockId, MirOperand, MirOperationKind, MirStorageId, MirTerminatorKind, MirValueId,
};

use super::local_id_remap::{
    MirLocalIdMapping, remap_operand_ids, remap_operation, remap_terminator,
};

struct ValueSubstitution<'a>(&'a BTreeMap<MirValueId, MirOperand>);

impl MirLocalIdMapping for ValueSubstitution<'_> {
    fn block(&self, old: MirBlockId) -> MirBlockId {
        old
    }

    fn storage(&self, old: MirStorageId) -> MirStorageId {
        old
    }

    fn value(&self, old: MirValueId) -> MirValueId {
        old
    }

    fn remap_operand(&self, operand: &mut MirOperand) {
        if let MirOperand::Value(value) = operand
            && let Some(replacement) = self.0.get(value)
        {
            // Each instruction owns its operand while paths retain shared immutable storage.
            *operand = replacement.clone();
        } else {
            remap_operand_ids(operand, self);
        }
    }
}

impl MirOperationKind {
    /// Replaces computed inputs, including projection selectors, with established equal operands.
    /// Replacements must dominate their uses and preserve the original value's type and ownership.
    pub fn substitute_values(&mut self, replacements: &BTreeMap<MirValueId, MirOperand>) {
        remap_operation(self, &ValueSubstitution(replacements));
    }
}

impl MirTerminatorKind {
    /// Replaces computed inputs and edge arguments with established equal operands.
    /// Replacements must dominate their uses and preserve the original value's type and ownership.
    pub fn substitute_values(&mut self, replacements: &BTreeMap<MirValueId, MirOperand>) {
        remap_terminator(self, &ValueSubstitution(replacements));
    }
}
