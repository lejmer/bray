use std::sync::Arc;

use bray_bound_tree::{BoundSourceAnchor, BoundUnitId, BoundUnitKey};

use crate::LoweringInput;

/// Identifies one basic block in a normalized lowered unit.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LoweredBlockId {
    unit: BoundUnitId,
    slot: u32,
}

impl LoweredBlockId {
    /// Returns the canonical bound unit that owns this lowered block.
    pub const fn unit(self) -> BoundUnitId {
        self.unit
    }

    const fn from_slot(unit: BoundUnitId, slot: u32) -> Self {
        Self { unit, slot }
    }

    fn to_index(self) -> Option<usize> {
        usize::try_from(self.slot).ok()
    }
}

/// One source-correlated basic block in normalized execution order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LoweredBlock {
    source: BoundSourceAnchor,
}

impl LoweredBlock {
    /// Returns the source construct responsible for this normalized block.
    pub const fn source(self) -> BoundSourceAnchor {
        self.source
    }
}

/// One immutable normalized lowered-bound unit ready for lower-level IR construction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoweredUnit {
    key: BoundUnitKey,
    unit: BoundUnitId,
    entry: LoweredBlockId,
    blocks: Arc<[LoweredBlock]>,
}

impl LoweredUnit {
    /// Returns the canonical semantic key shared with the source-shaped bound unit.
    pub const fn key(&self) -> &BoundUnitKey {
        &self.key
    }

    /// Returns the compilation-local identity shared with the source-shaped bound unit.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the normalized block where this unit begins execution.
    pub const fn entry(&self) -> LoweredBlockId {
        self.entry
    }

    /// Returns one block when its ID belongs to this lowered unit and names a committed slot.
    pub fn block(&self, id: LoweredBlockId) -> Option<&LoweredBlock> {
        if id.unit() != self.unit {
            return None;
        }

        id.to_index().and_then(|index| self.blocks.get(index))
    }

    /// Returns normalized blocks in deterministic construction order.
    pub fn blocks(&self) -> &[LoweredBlock] {
        &self.blocks
    }
}

/// Mutable task-local construction state for one normalized lowered unit.
#[derive(Debug)]
pub struct LoweredUnitBuilder {
    key: BoundUnitKey,
    unit: BoundUnitId,
    source: BoundSourceAnchor,
    blocks: Vec<LoweredBlock>,
}

impl LoweredUnitBuilder {
    /// Starts normalized construction from one validated checked-HIR input.
    pub fn new(input: LoweringInput<'_>) -> Self {
        let unit = input.unit();

        Self {
            // Publication owns the Arc-backed key independently of the checked-HIR query result.
            key: unit.key().clone(),
            unit: unit.unit(),
            source: unit.key().source(),
            blocks: Vec::new(),
        }
    }

    /// Commits one source-correlated normalized basic block.
    pub fn push_block(
        &mut self,
        source: BoundSourceAnchor,
    ) -> Result<LoweredBlockId, LoweredUnitBuildError> {
        if !same_source_snapshot(self.source, source) {
            return Err(LoweredUnitBuildError::SourceSnapshotMismatch {
                expected: self.source,
                actual: source,
            });
        }

        let Ok(slot) = u32::try_from(self.blocks.len()) else {
            return Err(LoweredUnitBuildError::BlockCapacityExceeded);
        };

        self.blocks.push(LoweredBlock { source });

        Ok(LoweredBlockId::from_slot(self.unit, slot))
    }

    /// Freezes the normalized block table after validating its exact entry block.
    pub fn finish(self, entry: LoweredBlockId) -> Result<LoweredUnit, LoweredUnitBuildError> {
        if entry.unit() != self.unit {
            return Err(LoweredUnitBuildError::ForeignEntry {
                expected: self.unit,
                actual: entry.unit(),
            });
        }

        let Some(index) = entry.to_index() else {
            return Err(LoweredUnitBuildError::MissingEntry(entry));
        };

        if index >= self.blocks.len() {
            return Err(LoweredUnitBuildError::MissingEntry(entry));
        }

        Ok(LoweredUnit {
            key: self.key,
            unit: self.unit,
            entry,
            blocks: self.blocks.into(),
        })
    }
}

/// A contract violation that prevents normalized lowered-unit publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoweredUnitBuildError {
    /// The source anchor belongs to another source identity or revision.
    SourceSnapshotMismatch {
        /// The source snapshot containing the checked bound unit.
        expected: BoundSourceAnchor,
        /// The source snapshot containing the rejected block anchor.
        actual: BoundSourceAnchor,
    },
    /// The normalized block table exceeded its compact identity space.
    BlockCapacityExceeded,
    /// The entry block belongs to another compilation-local bound unit.
    ForeignEntry {
        /// The unit being published.
        expected: BoundUnitId,
        /// The unit carried by the rejected entry block.
        actual: BoundUnitId,
    },
    /// The entry ID does not name a committed normalized block.
    MissingEntry(LoweredBlockId),
}

fn same_source_snapshot(expected: BoundSourceAnchor, actual: BoundSourceAnchor) -> bool {
    expected.syntax().source_id() == actual.syntax().source_id()
        && expected.source_version() == actual.source_version()
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundSourceAnchor, BoundUnitId, CheckedControlFlowFacts, ControlCompletion,
    };
    use bray_source::SourceVersion;

    use super::{LoweredBlockId, LoweredUnitBuildError, LoweredUnitBuilder};
    use crate::LoweringInput;
    use crate::test_support::bound_unit;

    #[test]
    fn builders_publish_identity_entry_and_source_order_without_copying_bound_nodes() {
        let unit = bound_unit(4);
        let control_flow = CheckedControlFlowFacts::new(
            unit.unit(),
            unit.key().kind(),
            ControlCompletion::default(),
        );

        let input = lowering_input(&unit, &control_flow);
        let mut builder = LoweredUnitBuilder::new(input);
        let first = push_block(&mut builder, unit.key().source());
        let second = push_block(&mut builder, unit.key().source());
        let lowered = finish(builder, first);

        assert_eq!(lowered.key(), unit.key());
        assert_eq!(lowered.unit(), unit.unit());
        assert_eq!(lowered.entry(), first);
        assert_eq!(lowered.blocks().len(), 2);
        assert_eq!(
            lowered.block(second).map(|block| block.source()),
            Some(unit.key().source())
        );
        assert_eq!(
            lowered.block(LoweredBlockId::from_slot(BoundUnitId::new(5), 0)),
            None
        );
    }

    #[test]
    fn builders_reject_foreign_sources_and_entry_blocks() {
        let unit = bound_unit(4);
        let control_flow = CheckedControlFlowFacts::new(
            unit.unit(),
            unit.key().kind(),
            ControlCompletion::default(),
        );

        let input = lowering_input(&unit, &control_flow);
        let missing_builder = LoweredUnitBuilder::new(input);
        let missing = LoweredBlockId::from_slot(unit.unit(), 0);

        assert_eq!(
            missing_builder.finish(missing),
            Err(LoweredUnitBuildError::MissingEntry(missing))
        );

        let mut builder = LoweredUnitBuilder::new(input);
        let foreign_source = BoundSourceAnchor::new(
            unit.key().source().syntax(),
            SourceVersion::new(unit.key().source().source_version().raw() + 1),
        );

        assert_eq!(
            builder.push_block(foreign_source),
            Err(LoweredUnitBuildError::SourceSnapshotMismatch {
                expected: unit.key().source(),
                actual: foreign_source,
            })
        );

        let local = push_block(&mut builder, unit.key().source());
        let foreign = LoweredBlockId::from_slot(BoundUnitId::new(5), 0);

        assert_eq!(
            builder.finish(foreign),
            Err(LoweredUnitBuildError::ForeignEntry {
                expected: BoundUnitId::new(4),
                actual: BoundUnitId::new(5),
            })
        );

        assert_eq!(local.unit(), BoundUnitId::new(4));
    }

    #[test]
    fn normalized_units_and_ids_are_safe_to_share_between_workers() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<super::LoweredUnit>();
        assert_send_sync::<LoweredBlockId>();
    }

    fn lowering_input<'unit>(
        unit: &'unit bray_bound_tree::BoundUnit,
        control_flow: &'unit CheckedControlFlowFacts,
    ) -> LoweringInput<'unit> {
        match LoweringInput::try_new(
            unit,
            control_flow,
            crate::test_support::compiler_known_role_registry(),
        ) {
            Ok(input) => input,
            Err(error) => panic!("matching lowering input must validate: {error:?}"),
        }
    }

    fn push_block(builder: &mut LoweredUnitBuilder, source: BoundSourceAnchor) -> LoweredBlockId {
        match builder.push_block(source) {
            Ok(block) => block,
            Err(error) => panic!("test lowered block must validate: {error:?}"),
        }
    }

    fn finish(builder: LoweredUnitBuilder, entry: LoweredBlockId) -> super::LoweredUnit {
        match builder.finish(entry) {
            Ok(unit) => unit,
            Err(error) => panic!("test lowered unit must validate: {error:?}"),
        }
    }
}
