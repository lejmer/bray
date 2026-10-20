use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{ReceiverMode, SymbolOrdinal};

/// Executable declaration requirements and their receiver observation mode.
#[derive(Clone, Debug)]
pub struct DeclarationPreconditions {
    clauses: Arc<[SymbolOrdinal]>,
    receiver: Option<ReceiverMode>,
}

impl DeclarationPreconditions {
    /// Retains checked ordinary clause ordinals in declaration order.
    pub fn new(clauses: impl IntoIterator<Item = SymbolOrdinal>, receiver: Option<ReceiverMode>) -> Self {
        Self { clauses: shared_slice(clauses), receiver }
    }

    /// Returns the runtime clauses in declaration order.
    pub fn clauses(&self) -> &[SymbolOrdinal] { &self.clauses }

    /// Returns the declared receiver mode when this callable has a receiver.
    pub const fn receiver(&self) -> Option<ReceiverMode> { self.receiver }
}

#[cfg(test)]
pub(crate) static EMPTY_DECLARATION_PRECONDITIONS: std::collections::BTreeMap<bray_symbols::CallableDefinitionId, DeclarationPreconditions> = std::collections::BTreeMap::new();
