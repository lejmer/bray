use std::sync::Arc;

use crate::unit::local_id_remap::MirLocalIdMapping;
use crate::{MirBlockId, MirPlace, MirStorageId};

/// One checked ownership-end handoff and its disposition-specific continuations.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MirCleanupEntry {
    handoff: MirBlockId,
    ordinary: MirBlockId,
    abandoned: MirBlockId,
    initial: bool,
}

impl MirCleanupEntry {
    /// Records a handoff. Initial entries can be selected directly by the synchronous body.
    pub const fn new(
        handoff: MirBlockId,
        ordinary: MirBlockId,
        abandoned: MirBlockId,
        initial: bool,
    ) -> Self {
        Self {
            handoff,
            ordinary,
            abandoned,
            initial,
        }
    }

    /// Returns the block that publishes this ownership-end handoff.
    pub const fn handoff(self) -> MirBlockId {
        self.handoff
    }
    /// Returns ordinary receiver cleanup.
    pub const fn ordinary(self) -> MirBlockId {
        self.ordinary
    }
    /// Returns already-abandoned receiver destruction.
    pub const fn abandoned(self) -> MirBlockId {
        self.abandoned
    }
    /// Returns whether the shared synchronous body can publish this handoff.
    pub const fn is_initial(self) -> bool {
        self.initial
    }
    /// Returns both disposition-specific roots in a stable order.
    pub const fn targets(self) -> [MirBlockId; 2] {
        [self.ordinary, self.abandoned]
    }
}

/// Checked receiver cleanup resumed by the owner after its synchronous body returns.
/// The context is caller-owned storage borrowed by the body. Entry zero means the
/// receiver has no remaining cleanup. Other selectors index the checked entries.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirCleanupContinuation {
    context: MirStorageId,
    selector: MirPlace,
    entries: Arc<[MirCleanupEntry]>,
}

impl MirCleanupContinuation {
    /// Records the checked body/tail boundary without reinterpreting its operations.
    pub fn new(
        context: MirStorageId,
        selector: MirPlace,
        entries: impl IntoIterator<Item = MirCleanupEntry>,
    ) -> Self {
        Self {
            context,
            selector,
            entries: entries.into_iter().collect(),
        }
    }

    /// Returns the hidden borrowed context parameter retained by every tail entry.
    pub const fn context(&self) -> MirStorageId {
        self.context
    }
    /// Returns the body-published entry selector in the caller-owned context.
    pub const fn selector(&self) -> &MirPlace {
        &self.selector
    }
    /// Returns selected handoffs in publication order.
    pub fn entries(&self) -> &[MirCleanupEntry] {
        &self.entries
    }

    pub(crate) fn remap_local_ids(&mut self, mappings: &impl MirLocalIdMapping) {
        self.context = mappings.storage(self.context);
        self.selector.remap_local_ids(mappings);

        for entry in Arc::make_mut(&mut self.entries) {
            entry.handoff = mappings.block(entry.handoff);
            entry.ordinary = mappings.block(entry.ordinary);
            entry.abandoned = mappings.block(entry.abandoned);
        }
    }
}
