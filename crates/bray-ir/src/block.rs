use std::sync::Arc;

use crate::{MirOperationId, MirSourceAnchor, MirTerminator, MirValueId};
use bray_base::shared_slice;

/// Semantic role of one MIR control-flow block.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirBlockKind {
    /// Ordinary executable control flow.
    Ordinary,
    /// Phase-one task cancellation broadcast.
    CleanupBroadcast,
    /// Phase-two task and ordinary lifecycle resolution.
    LifecycleResolution,
}

/// One immutable basic block in a MIR unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirBlock {
    source: MirSourceAnchor,
    kind: MirBlockKind,
    parameters: Arc<[MirValueId]>,
    operations: Arc<[MirOperationId]>,
    terminator: MirTerminator,
}

impl MirBlock {
    pub(crate) fn new(
        source: MirSourceAnchor,
        kind: MirBlockKind,
        parameters: Vec<MirValueId>,
        operations: Vec<MirOperationId>,
        terminator: MirTerminator,
    ) -> Self {
        Self {
            source,
            kind,
            parameters: shared_slice(parameters),
            operations: shared_slice(operations),
            terminator,
        }
    }

    /// Returns the source provenance associated with this block.
    pub const fn source(&self) -> &MirSourceAnchor {
        &self.source
    }

    /// Returns the block's control-flow role.
    pub const fn kind(&self) -> MirBlockKind {
        self.kind
    }

    /// Returns incoming values in edge-argument order.
    pub fn parameters(&self) -> &[MirValueId] {
        &self.parameters
    }

    /// Returns operations in execution order.
    pub fn operations(&self) -> &[MirOperationId] {
        &self.operations
    }

    /// Returns the operation that ends this block.
    pub const fn terminator(&self) -> &MirTerminator {
        &self.terminator
    }
}
