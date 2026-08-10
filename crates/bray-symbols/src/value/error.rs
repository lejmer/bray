use super::{GenericOwnerId, SemanticValueKind, SemanticValueStoreId};

/// Reports failure to allocate a process-unique semantic-store identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SemanticValueStoreCreateError {
    /// The process-local semantic-store identity space is exhausted.
    IdentitySpaceExhausted,
}

/// Reports a typed semantic-store construction or access failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticValueStoreError {
    /// An ID issued by another store was supplied to this store.
    ForeignId {
        /// The store receiving the operation.
        expected: SemanticValueStoreId,
        /// The store that issued the supplied ID.
        actual: SemanticValueStoreId,
    },
    /// An ID does not address an entry of its exact category.
    UnknownId {
        /// The table addressed by the invalid ID.
        kind: SemanticValueKind,
    },
    /// One canonical table can no longer issue compact slots.
    CapacityExhausted {
        /// The exhausted table.
        kind: SemanticValueKind,
    },
    /// An application used a substitution belonging to another generic owner.
    GenericOwnerMismatch {
        /// The owner required by the application.
        expected: GenericOwnerId,
        /// The owner retained by the substitution.
        actual: GenericOwnerId,
    },
    /// An open or recovery substitution was requested as a concrete substitution.
    OpenSubstitution,
}
