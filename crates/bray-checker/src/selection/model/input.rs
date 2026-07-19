use super::{CallableSelectionRequest, OperationSelectionRequest};

/// Binder-enumerated semantic choices for one bound unit.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SemanticSelectionInput {
    calls: Vec<CallableSelectionRequest>,
    operations: Vec<OperationSelectionRequest>,
}

impl SemanticSelectionInput {
    /// Creates an input without any candidate-bearing expressions.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces callable candidate requests in source-semantic order.
    pub fn with_calls(mut self, calls: impl IntoIterator<Item = CallableSelectionRequest>) -> Self {
        self.calls = calls.into_iter().collect();

        self
    }

    /// Replaces operation candidate requests in source-semantic order.
    pub fn with_operations(
        mut self,
        operations: impl IntoIterator<Item = OperationSelectionRequest>,
    ) -> Self {
        self.operations = operations.into_iter().collect();

        self
    }

    pub(in crate::selection) fn into_parts(
        self,
    ) -> (
        Vec<CallableSelectionRequest>,
        Vec<OperationSelectionRequest>,
    ) {
        (self.calls, self.operations)
    }
}
