use std::sync::Arc;

use bray_bound_tree::{BoundSourceAnchor, BoundUnitIdentity, BoundUnitKey};
use bray_execution::ExecutableHostContract;
use bray_symbols::ProductIdentity;

use crate::MirUnitExecution;

/// Compilation-local identity of one source or compiler-generated MIR unit.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MirUnitId(u32);

impl MirUnitId {
    /// Creates an identity from its compilation-local representation.
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the compilation-local representation.
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Stable structural identity of one source or compiler-generated MIR unit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirUnitKey {
    /// Unit lowered from one canonical checked semantic unit.
    Bound(BoundUnitKey),
    /// Compiler-generated executable host stub for one selected product.
    ExecutableHost(ProductIdentity),
}

/// Source or generated-product origin retained for diagnostics and metadata.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirSourceOrigin {
    /// Exact source snapshot responsible for emitted MIR.
    Source(BoundSourceAnchor),
    /// Compiler-generated executable host associated with one selected product.
    ExecutableHost(ProductIdentity),
}

/// Identifies one basic block in a Bray MIR unit.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MirBlockId {
    unit: MirUnitId,
    slot: u32,
}

impl MirBlockId {
    /// Returns the compilation-local MIR unit that owns this block.
    pub const fn unit(self) -> MirUnitId {
        self.unit
    }

    const fn from_slot(unit: MirUnitId, slot: u32) -> Self {
        Self { unit, slot }
    }

    fn to_index(self) -> Option<usize> {
        usize::try_from(self.slot).ok()
    }
}

/// One source-correlated or compiler-generated basic block in canonical MIR order.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirBlock {
    source: MirSourceOrigin,
}

impl MirBlock {
    /// Returns the source snapshot or generated product responsible for this block.
    pub const fn source(&self) -> &MirSourceOrigin {
        &self.source
    }
}

/// One immutable backend-independent Bray MIR unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirUnit {
    key: MirUnitKey,
    unit: MirUnitId,
    source: MirSourceOrigin,
    execution: MirUnitExecution,
    entry: MirBlockId,
    blocks: Arc<[MirBlock]>,
}

impl MirUnit {
    /// Returns the stable source or generated-product identity.
    pub const fn key(&self) -> &MirUnitKey {
        &self.key
    }

    /// Returns the compilation-local MIR identity.
    pub const fn unit(&self) -> MirUnitId {
        self.unit
    }

    /// Returns the source snapshot or generated product responsible for this unit.
    pub const fn source(&self) -> &MirSourceOrigin {
        &self.source
    }

    /// Returns the protected execution representation owned by this unit.
    pub const fn execution(&self) -> &MirUnitExecution {
        &self.execution
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
    key: MirUnitKey,
    unit: MirUnitId,
    source: MirSourceOrigin,
    execution: MirUnitExecution,
    blocks: Vec<MirBlock>,
}

impl MirUnitBuilder {
    /// Creates a MIR builder for one canonical checked semantic unit.
    pub fn for_bound(identity: BoundUnitIdentity<'_>, execution: MirUnitExecution) -> Self {
        let source = identity.key().source();

        Self {
            // Publication retains the Arc-backed key after the identity view borrow ends.
            key: MirUnitKey::Bound(identity.key().clone()),
            unit: MirUnitId::new(identity.unit().raw()),
            source: MirSourceOrigin::Source(source),
            execution,
            blocks: Vec::new(),
        }
    }

    /// Creates a MIR builder for a compiler-generated executable host stub.
    pub fn for_executable_host(unit: MirUnitId, host: ExecutableHostContract) -> Self {
        // Stable unit and source metadata retain the Arc-backed product independently of the host.
        let product = host.product().clone();

        Self {
            key: MirUnitKey::ExecutableHost(product.clone()),
            unit,
            source: MirSourceOrigin::ExecutableHost(product),
            execution: MirUnitExecution::ExecutableHost(host),
            blocks: Vec::new(),
        }
    }

    /// Commits one MIR block belonging to the unit's source or generated product.
    pub fn push_block(&mut self, source: MirSourceOrigin) -> Result<MirBlockId, MirUnitBuildError> {
        if !same_origin(&self.source, &source) {
            return Err(MirUnitBuildError::SourceOriginMismatch);
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
            source: self.source,
            execution: self.execution,
            entry,
            blocks: self.blocks.into(),
        })
    }
}

/// A contract violation that prevents creation of a MIR unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MirUnitBuildError {
    /// A block origin does not belong to the source snapshot or generated product of its unit.
    SourceOriginMismatch,
    /// The MIR block table exceeded its compact identity space.
    BlockCapacityExceeded,
    /// The entry block belongs to another compilation-local MIR unit.
    ForeignEntry {
        /// The unit being published.
        expected: MirUnitId,
        /// The unit carried by the rejected entry block.
        actual: MirUnitId,
    },
    /// The entry ID does not name a committed MIR block.
    MissingEntry(MirBlockId),
}

fn same_origin(expected: &MirSourceOrigin, actual: &MirSourceOrigin) -> bool {
    match (expected, actual) {
        (MirSourceOrigin::Source(expected), MirSourceOrigin::Source(actual)) => {
            expected.syntax().source_id() == actual.syntax().source_id()
                && expected.source_version() == actual.source_version()
        }
        (MirSourceOrigin::ExecutableHost(expected), MirSourceOrigin::ExecutableHost(actual)) => {
            expected == actual
        }
        (MirSourceOrigin::Source(_), MirSourceOrigin::ExecutableHost(_))
        | (MirSourceOrigin::ExecutableHost(_), MirSourceOrigin::Source(_)) => false,
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundSourceAnchor;
    use bray_source::SourceVersion;
    use bray_testing::test_bound_unit;

    use super::{MirBlockId, MirSourceOrigin, MirUnitBuildError, MirUnitBuilder, MirUnitId};
    use crate::MirUnitExecution;

    #[test]
    fn builders_publish_identity_execution_and_source_order() {
        let bound = test_bound_unit(4);
        let key = bound.key().clone();
        let source = key.source();

        let mut builder =
            MirUnitBuilder::for_bound(bound.identity(), MirUnitExecution::Synchronous);

        let first = push_source_block(&mut builder, source);
        let second = push_source_block(&mut builder, source);

        let mir = finish(builder, first);

        assert_eq!(mir.key(), &super::MirUnitKey::Bound(key));
        assert_eq!(mir.unit(), MirUnitId::new(4));
        assert_eq!(mir.entry(), first);
        assert_eq!(mir.blocks().len(), 2);
        assert_eq!(
            mir.block(second).map(|block| block.source()),
            Some(&MirSourceOrigin::Source(source))
        );

        assert_eq!(mir.block(MirBlockId::from_slot(MirUnitId::new(5), 0)), None);
    }

    #[test]
    fn builders_reject_foreign_sources_and_entry_blocks() {
        let bound = test_bound_unit(4);
        let source = bound.key().source();
        let missing_builder =
            MirUnitBuilder::for_bound(bound.identity(), MirUnitExecution::Synchronous);
        let missing = MirBlockId::from_slot(MirUnitId::new(4), 0);

        assert_eq!(
            missing_builder.finish(missing),
            Err(MirUnitBuildError::MissingEntry(missing))
        );

        let mut builder =
            MirUnitBuilder::for_bound(bound.identity(), MirUnitExecution::Synchronous);

        let foreign_source = BoundSourceAnchor::new(
            source.syntax(),
            SourceVersion::new(source.source_version().raw() + 1),
        );

        assert_eq!(
            builder.push_block(MirSourceOrigin::Source(foreign_source)),
            Err(MirUnitBuildError::SourceOriginMismatch)
        );

        let local = push_source_block(&mut builder, source);
        let foreign = MirBlockId::from_slot(MirUnitId::new(5), 0);

        assert_eq!(
            builder.finish(foreign),
            Err(MirUnitBuildError::ForeignEntry {
                expected: MirUnitId::new(4),
                actual: MirUnitId::new(5),
            })
        );

        assert_eq!(local.unit(), MirUnitId::new(4));
    }

    #[test]
    fn mir_units_and_ids_are_safe_to_share_between_workers() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<super::MirUnit>();
        assert_send_sync::<MirUnitId>();
        assert_send_sync::<MirBlockId>();
    }

    fn push_source_block(
        builder: &mut MirUnitBuilder,
        source: bray_bound_tree::BoundSourceAnchor,
    ) -> MirBlockId {
        match builder.push_block(MirSourceOrigin::Source(source)) {
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
