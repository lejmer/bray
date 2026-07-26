use crate::{
    MirBlock, MirBlockId, MirOperation, MirOperationId, MirStorage, MirStorageId, MirTerminator,
    MirUnit, MirValue, MirValueId,
};

/// Controls deterministic traversal of immutable MIR.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MirVisitControl {
    /// Visit the current element's owned children and later siblings.
    Continue,
    /// Skip the current element's owned children and continue with later siblings.
    Skip,
    /// Stop the walk immediately.
    Stop,
}

/// Callback interface for deterministic serial MIR traversal.
pub trait MirVisitor {
    /// Visits the MIR unit before any contained table.
    fn visit_unit(&mut self, _unit: &MirUnit) -> MirVisitControl {
        MirVisitControl::Continue
    }

    /// Visits one storage allocation.
    fn visit_storage(&mut self, _id: MirStorageId, _storage: &MirStorage) -> MirVisitControl {
        MirVisitControl::Continue
    }

    /// Visits one value.
    fn visit_value(&mut self, _id: MirValueId, _value: &MirValue) -> MirVisitControl {
        MirVisitControl::Continue
    }

    /// Visits one block before its operations and terminator.
    fn visit_block(&mut self, _id: MirBlockId, _block: &MirBlock) -> MirVisitControl {
        MirVisitControl::Continue
    }

    /// Visits one operation in block execution order.
    fn visit_operation(
        &mut self,
        _id: MirOperationId,
        _operation: &MirOperation,
    ) -> MirVisitControl {
        MirVisitControl::Continue
    }

    /// Visits one block terminator after its operations.
    fn visit_terminator(
        &mut self,
        _block: MirBlockId,
        _terminator: &MirTerminator,
    ) -> MirVisitControl {
        MirVisitControl::Continue
    }
}

/// Walks one immutable MIR unit in deterministic table and block order.
pub fn walk_mir_unit<V: MirVisitor + ?Sized>(unit: &MirUnit, visitor: &mut V) {
    match visitor.visit_unit(unit) {
        MirVisitControl::Continue => {}
        MirVisitControl::Skip | MirVisitControl::Stop => return,
    }

    for (index, storage) in unit.storages().iter().enumerate() {
        let Some(slot) = u32::try_from(index).ok() else {
            return;
        };

        let id = MirStorageId::from_slot(unit.unit(), slot);

        if visitor.visit_storage(id, storage) == MirVisitControl::Stop {
            return;
        }
    }

    for (index, value) in unit.values().iter().enumerate() {
        let Some(slot) = u32::try_from(index).ok() else {
            return;
        };

        let id = MirValueId::from_slot(unit.unit(), slot);

        if visitor.visit_value(id, value) == MirVisitControl::Stop {
            return;
        }
    }

    for (index, block) in unit.blocks().iter().enumerate() {
        let Some(slot) = u32::try_from(index).ok() else {
            return;
        };

        let block_id = MirBlockId::from_slot(unit.unit(), slot);

        match visitor.visit_block(block_id, block) {
            MirVisitControl::Continue => {}
            MirVisitControl::Skip => continue,
            MirVisitControl::Stop => return,
        }

        for operation_id in block.operations() {
            let Some(operation) = unit.operation(*operation_id) else {
                return;
            };

            if visitor.visit_operation(*operation_id, operation) == MirVisitControl::Stop {
                return;
            }
        }

        if visitor.visit_terminator(block_id, block.terminator()) == MirVisitControl::Stop {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_testing::test_bound_unit;

    use super::{MirVisitControl, MirVisitor, walk_mir_unit};
    use crate::{
        MirBlock, MirBlockId, MirBlockKind, MirSourceAnchor, MirTerminatorKind, MirUnitBuilder,
        MirUnitKind,
    };

    #[test]
    fn walkers_visit_blocks_and_terminators_in_construction_order() {
        let unit = mir_unit();
        let mut visitor = BlockVisitor::default();

        walk_mir_unit(&unit, &mut visitor);

        assert_eq!(visitor.blocks, vec![unit.entry()]);
        assert_eq!(visitor.terminators, vec![unit.entry()]);
    }

    #[derive(Default)]
    struct BlockVisitor {
        blocks: Vec<MirBlockId>,
        terminators: Vec<MirBlockId>,
    }

    impl MirVisitor for BlockVisitor {
        fn visit_block(&mut self, id: MirBlockId, _block: &MirBlock) -> MirVisitControl {
            self.blocks.push(id);
            MirVisitControl::Continue
        }

        fn visit_terminator(
            &mut self,
            block: MirBlockId,
            _terminator: &crate::MirTerminator,
        ) -> MirVisitControl {
            self.terminators.push(block);
            MirVisitControl::Continue
        }
    }

    fn mir_unit() -> crate::MirUnit {
        let bound = test_bound_unit(4);
        let source = MirSourceAnchor::from(bound.key().source());
        let target = crate::test_support::test_target();

        let mut builder =
            MirUnitBuilder::for_bound(bound.identity(), MirUnitKind::Synchronous, target);

        let Ok(entry) = builder.push_block(source.clone(), MirBlockKind::Ordinary) else {
            panic!("test MIR block must be valid");
        };

        let Ok(()) = builder.set_terminator(entry, source, MirTerminatorKind::Return(None)) else {
            panic!("test MIR terminator must be valid");
        };

        match builder.finish(entry) {
            Ok(unit) => unit,
            Err(error) => panic!("test MIR unit must be valid: {error:?}"),
        }
    }
}
