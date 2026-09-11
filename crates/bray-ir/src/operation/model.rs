use crate::{MirOperationId, MirOperationKind, MirSourceAnchor, MirValueId};

/// One committed operation and its optional result value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MirOperationCommit {
    operation: MirOperationId,
    result: Option<MirValueId>,
}

impl MirOperationCommit {
    pub(crate) const fn new(operation: MirOperationId, result: Option<MirValueId>) -> Self {
        Self { operation, result }
    }

    /// Returns the committed operation ID.
    pub const fn operation(self) -> MirOperationId {
        self.operation
    }

    /// Returns the value produced by the operation, when any.
    pub const fn result(self) -> Option<MirValueId> {
        self.result
    }
}

/// One operation stored in a MIR unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirOperation {
    source: MirSourceAnchor,
    kind: MirOperationKind,
    result: Option<MirValueId>,
    cleanup_execution: Option<crate::MirFrameExecutionState>,
}

impl MirOperation {
    pub(crate) const fn new(
        source: MirSourceAnchor,
        kind: MirOperationKind,
        result: Option<MirValueId>,
    ) -> Self {
        Self {
            source,
            kind,
            result,
            cleanup_execution: None,
        }
    }

    /// Returns checked context for expanding cleanup, or none when no context was supplied.
    /// A present context with empty requirements remains distinct from unavailable context.
    pub const fn cleanup_execution(&self) -> Option<&crate::MirFrameExecutionState> {
        self.cleanup_execution.as_ref()
    }

    pub(crate) fn set_cleanup_execution(
        &mut self,
        execution: Option<crate::MirFrameExecutionState>,
    ) {
        self.cleanup_execution = execution;
    }

    pub(crate) fn replace(&mut self, kind: MirOperationKind, result: Option<MirValueId>) {
        self.kind = kind;
        self.result = result;
    }

    /// Returns the operation's source provenance.
    pub const fn source(&self) -> &MirSourceAnchor {
        &self.source
    }

    /// Returns the explicit operation payload.
    pub const fn kind(&self) -> &MirOperationKind {
        &self.kind
    }

    /// Returns the value produced by the operation, when any.
    pub const fn result(&self) -> Option<MirValueId> {
        self.result
    }
}
