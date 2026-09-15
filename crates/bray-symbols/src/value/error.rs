use super::SemanticValueKind;

/// Reports failure to allocate a process-unique semantic-store identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SemanticValueStoreCreateError {
    /// The process-local semantic-store identity space is exhausted.
    IdentitySpaceExhausted,
}

/// Reports exhausted capacity while interning semantic values.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SemanticValueStoreError {
    /// One canonical table can no longer issue compact slots.
    CapacityExhausted {
        /// The exhausted table.
        kind: SemanticValueKind,
    },
}
