use std::sync::Arc;

use bray_bound_tree::BoundUnitKey;
use bray_symbols::ProductIdentity;

use crate::{
    MirBlock, MirBlockId, MirFrameDescriptor, MirOperation, MirOperationId, MirSourceOrigin,
    MirStorage, MirStorageId, MirTargetFacts, MirUnitId, MirUnitKind, MirValue, MirValueId,
};

/// Stable semantic key of one MIR unit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirUnitKey {
    /// MIR lowered from one checked bound unit.
    Bound(BoundUnitKey),
    /// MIR synthesized for one executable product host.
    ExecutableHost(ProductIdentity),
}

/// One immutable backend-independent MIR unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirUnit {
    pub(super) key: MirUnitKey,
    pub(super) unit: MirUnitId,
    pub(super) source: MirSourceOrigin,
    pub(super) target: MirTargetFacts,
    pub(super) kind: MirUnitKind,
    pub(super) frame_descriptor: Option<MirFrameDescriptor>,
    pub(super) entry: MirBlockId,
    pub(super) blocks: Arc<[MirBlock]>,
    pub(super) operations: Arc<[MirOperation]>,
    pub(super) storages: Arc<[MirStorage]>,
    pub(super) values: Arc<[MirValue]>,
}

impl MirUnit {
    /// Returns the stable semantic unit key.
    pub const fn key(&self) -> &MirUnitKey {
        &self.key
    }

    /// Returns the compilation-local MIR unit identity.
    pub const fn unit(&self) -> MirUnitId {
        self.unit
    }

    /// Returns the source or generated-product origin.
    pub const fn source(&self) -> &MirSourceOrigin {
        &self.source
    }

    /// Returns target facts used to construct this MIR.
    pub const fn target(&self) -> &MirTargetFacts {
        &self.target
    }

    /// Returns the unit's representation category.
    pub const fn kind(&self) -> &MirUnitKind {
        &self.kind
    }

    /// Returns the hidden protected-frame descriptor when this unit owns one.
    pub const fn frame_descriptor(&self) -> Option<&MirFrameDescriptor> {
        self.frame_descriptor.as_ref()
    }

    /// Returns the entry block.
    pub const fn entry(&self) -> MirBlockId {
        self.entry
    }

    /// Returns blocks in deterministic construction order.
    pub fn blocks(&self) -> &[MirBlock] {
        &self.blocks
    }

    /// Iterates blocks with their unit-local identities.
    pub fn blocks_with_ids(&self) -> impl ExactSizeIterator<Item = (MirBlockId, &MirBlock)> {
        self.blocks.iter().enumerate().map(|(index, block)| {
            let slot = u32::try_from(index).unwrap_or(u32::MAX);

            (MirBlockId::from_slot(self.unit, slot), block)
        })
    }

    /// Returns operations in deterministic construction order.
    pub fn operations(&self) -> &[MirOperation] {
        &self.operations
    }

    /// Iterates operations with their unit-local identities.
    pub fn operations_with_ids(
        &self,
    ) -> impl ExactSizeIterator<Item = (MirOperationId, &MirOperation)> {
        self.operations.iter().enumerate().map(|(index, operation)| {
            let slot = u32::try_from(index).unwrap_or(u32::MAX);

            (MirOperationId::from_slot(self.unit, slot), operation)
        })
    }

    /// Returns storage allocations in deterministic construction order.
    pub fn storages(&self) -> &[MirStorage] {
        &self.storages
    }

    /// Returns values in deterministic construction order.
    pub fn values(&self) -> &[MirValue] {
        &self.values
    }

    /// Resolves a block owned by this unit.
    pub fn block(&self, id: MirBlockId) -> Option<&MirBlock> {
        self.local_index(id.unit(), id.to_index())
            .and_then(|index| self.blocks.get(index))
    }

    /// Resolves an operation owned by this unit.
    pub fn operation(&self, id: MirOperationId) -> Option<&MirOperation> {
        self.local_index(id.unit(), id.to_index())
            .and_then(|index| self.operations.get(index))
    }

    /// Resolves a storage allocation owned by this unit.
    pub fn storage(&self, id: MirStorageId) -> Option<&MirStorage> {
        self.local_index(id.unit(), id.to_index())
            .and_then(|index| self.storages.get(index))
    }

    /// Resolves a value owned by this unit.
    pub fn value(&self, id: MirValueId) -> Option<&MirValue> {
        self.local_index(id.unit(), id.to_index())
            .and_then(|index| self.values.get(index))
    }

    fn local_index(&self, unit: MirUnitId, index: Option<usize>) -> Option<usize> {
        (unit == self.unit).then_some(index).flatten()
    }
}
