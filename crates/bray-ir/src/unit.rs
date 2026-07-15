use std::sync::Arc;

use bray_bound_tree::{BoundSourceAnchor, BoundUnitId, BoundUnitIdentity, BoundUnitKey};

/// Identifies one basic block in a Bray MIR unit.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MirBlockId {
    unit: BoundUnitId,
    slot: u32,
}

impl MirBlockId {
    /// Returns the canonical bound unit that owns this MIR block.
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

/// One source-correlated basic block in canonical MIR order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MirBlock {
    source: BoundSourceAnchor,
}

impl MirBlock {
    /// Returns the source construct responsible for this MIR block.
    pub const fn source(self) -> BoundSourceAnchor {
        self.source
    }
}

/// One immutable backend-independent Bray MIR unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirUnit {
    key: BoundUnitKey,
    unit: BoundUnitId,
    entry: MirBlockId,
    blocks: Arc<[MirBlock]>,
}

impl MirUnit {
    /// Returns the canonical semantic key shared with the source-shaped bound unit.
    pub const fn key(&self) -> &BoundUnitKey {
        &self.key
    }

    /// Returns the compilation-local identity shared with the source-shaped bound unit.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the MIR block where this unit begins execution.
    pub const fn entry(&self) -> MirBlockId {
        self.entry
    }

    /// Returns the block identified within this MIR unit, when present.
    pub fn block(&self, id: MirBlockId) -> Option<&MirBlock> {
        if id.unit() != self.unit {
            return None;
        }

        id.to_index().and_then(|index| self.blocks.get(index))
    }

    /// Returns MIR blocks in canonical order.
    pub fn blocks(&self) -> &[MirBlock] {
        &self.blocks
    }
}

/// Builder for one Bray MIR unit.
#[derive(Debug)]
pub struct MirUnitBuilder {
    key: BoundUnitKey,
    unit: BoundUnitId,
    source: BoundSourceAnchor,
    blocks: Vec<MirBlock>,
}

impl MirUnitBuilder {
    /// Creates a MIR builder for one canonical bound-unit identity.
    pub fn new(identity: BoundUnitIdentity<'_>) -> Self {
        Self {
            // Publication retains the Arc-backed key after the identity view borrow ends.
            key: identity.key().clone(),
            unit: identity.unit(),
            source: identity.key().source(),
            blocks: Vec::new(),
        }
    }

    /// Commits one source-correlated MIR block.
    pub fn push_block(
        &mut self,
        source: BoundSourceAnchor,
    ) -> Result<MirBlockId, MirUnitBuildError> {
        if !same_source_snapshot(self.source, source) {
            return Err(MirUnitBuildError::SourceSnapshotMismatch {
                expected: self.source,
                actual: source,
            });
        }

        let Ok(slot) = u32::try_from(self.blocks.len()) else {
            return Err(MirUnitBuildError::BlockCapacityExceeded);
        };

        self.blocks.push(MirBlock { source });

        Ok(MirBlockId::from_slot(self.unit, slot))
    }

    /// Completes the MIR unit after validating its entry block.
    pub fn finish(self, entry: MirBlockId) -> Result<MirUnit, MirUnitBuildError> {
        if entry.unit() != self.unit {
            return Err(MirUnitBuildError::ForeignEntry {
                expected: self.unit,
                actual: entry.unit(),
            });
        }

        let Some(index) = entry.to_index() else {
            return Err(MirUnitBuildError::MissingEntry(entry));
        };

        if index >= self.blocks.len() {
            return Err(MirUnitBuildError::MissingEntry(entry));
        }

        Ok(MirUnit {
            key: self.key,
            unit: self.unit,
            entry,
            blocks: self.blocks.into(),
        })
    }
}

/// A contract violation that prevents creation of a MIR unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MirUnitBuildError {
    /// The source anchor belongs to another source identity or revision.
    SourceSnapshotMismatch {
        /// The source snapshot containing the checked bound unit.
        expected: BoundSourceAnchor,
        /// The source snapshot containing the rejected block anchor.
        actual: BoundSourceAnchor,
    },
    /// The MIR block table exceeded its compact identity space.
    BlockCapacityExceeded,
    /// The entry block belongs to another compilation-local bound unit.
    ForeignEntry {
        /// The unit being published.
        expected: BoundUnitId,
        /// The unit carried by the rejected entry block.
        actual: BoundUnitId,
    },
    /// The entry ID does not name a committed MIR block.
    MissingEntry(MirBlockId),
}

fn same_source_snapshot(expected: BoundSourceAnchor, actual: BoundSourceAnchor) -> bool {
    expected.syntax().source_id() == actual.syntax().source_id()
        && expected.source_version() == actual.source_version()
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundSourceAnchor, BoundUnitId};
    use bray_source::SourceVersion;
    use bray_testing::test_bound_unit;

    use super::{MirBlockId, MirUnitBuildError, MirUnitBuilder};

    #[test]
    fn builders_publish_identity_entry_and_source_order() {
        let bound = test_bound_unit(4);
        let key = bound.key().clone();
        let unit = bound.unit();
        let source = key.source();

        let mut builder = MirUnitBuilder::new(bound.identity());

        let first = push_block(&mut builder, source);
        let second = push_block(&mut builder, source);

        let mir = finish(builder, first);

        assert_eq!(mir.key(), &key);
        assert_eq!(mir.unit(), unit);
        assert_eq!(mir.entry(), first);
        assert_eq!(mir.blocks().len(), 2);
        assert_eq!(mir.block(second).map(|block| block.source()), Some(source));

        assert_eq!(
            mir.block(MirBlockId::from_slot(BoundUnitId::new(5), 0)),
            None
        );
    }

    #[test]
    fn builders_reject_foreign_sources_and_entry_blocks() {
        let bound = test_bound_unit(4);
        let unit = bound.unit();
        let source = bound.key().source();
        let missing_builder = MirUnitBuilder::new(bound.identity());
        let missing = MirBlockId::from_slot(unit, 0);

        assert_eq!(
            missing_builder.finish(missing),
            Err(MirUnitBuildError::MissingEntry(missing))
        );

        let mut builder = MirUnitBuilder::new(bound.identity());

        let foreign_source = BoundSourceAnchor::new(
            source.syntax(),
            SourceVersion::new(source.source_version().raw() + 1),
        );

        assert_eq!(
            builder.push_block(foreign_source),
            Err(MirUnitBuildError::SourceSnapshotMismatch {
                expected: source,
                actual: foreign_source,
            })
        );

        let local = push_block(&mut builder, source);
        let foreign = MirBlockId::from_slot(BoundUnitId::new(5), 0);

        assert_eq!(
            builder.finish(foreign),
            Err(MirUnitBuildError::ForeignEntry {
                expected: unit,
                actual: BoundUnitId::new(5),
            })
        );

        assert_eq!(local.unit(), unit);
    }

    #[test]
    fn mir_units_and_ids_are_safe_to_share_between_workers() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<super::MirUnit>();
        assert_send_sync::<MirBlockId>();
    }

    fn push_block(builder: &mut MirUnitBuilder, source: BoundSourceAnchor) -> MirBlockId {
        match builder.push_block(source) {
            Ok(block) => block,
            Err(error) => panic!("test MIR block must validate: {error:?}"),
        }
    }

    fn finish(builder: MirUnitBuilder, entry: MirBlockId) -> super::MirUnit {
        match builder.finish(entry) {
            Ok(unit) => unit,
            Err(error) => panic!("test MIR unit must validate: {error:?}"),
        }
    }
}
