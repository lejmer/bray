use crate::{
    AnySymbolId, ExternalSymbolKey, ImportedInterfaceId, InterfaceSymbolId, PackageIdentity,
    SymbolKind, SymbolName, SymbolRelationshipKind,
};

/// Describes why validated interface surfaces cannot form one imported symbol skeleton.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImportedSymbolSkeletonBuildError {
    /// Two inputs use the same loaded-interface handle.
    DuplicateInterface(ImportedInterfaceId),
    /// Two inputs define the same package identity.
    DuplicatePackage(PackageIdentity),
    /// Compilation-local symbol identity allocation exceeded its compact representation.
    SymbolCapacityExceeded {
        /// Total number of compilation-local symbol identities required.
        actual: u64,
        /// Greatest number of identities representable by the compact ID domain.
        maximum: u64,
    },
    /// Two interface symbols define the same stable external identity.
    DuplicateExternalKey {
        /// Stable external identity defined more than once.
        key: ExternalSymbolKey,
        /// Interface containing the first definition.
        first: ImportedInterfaceId,
        /// Interface containing the repeated definition.
        duplicate: ImportedInterfaceId,
    },
    /// A relationship references an interface-local symbol that does not exist.
    RelationshipSymbolOutOfBounds {
        /// Interface containing the malformed relationship.
        interface: ImportedInterfaceId,
        /// Missing interface-local symbol.
        symbol: InterfaceSymbolId,
    },
    /// A relationship category does not accept the supplied owner and member kinds.
    InvalidRelationshipKinds {
        /// Relationship category.
        relationship: SymbolRelationshipKind,
        /// Actual owner kind.
        owner: SymbolKind,
        /// Actual member kind.
        member: SymbolKind,
    },
    /// A containment relationship disagrees with the member identity record.
    RelationshipContainmentMismatch {
        /// Remapped relationship owner.
        owner: AnySymbolId,
        /// Remapped relationship member.
        member: AnySymbolId,
    },
    /// Typed owner-relative relationship positions are not dense from zero.
    NonCanonicalRelationshipOrdinal {
        /// Relationship category.
        relationship: SymbolRelationshipKind,
        /// Remapped relationship owner.
        owner: AnySymbolId,
        /// Required next ordinal.
        expected: u64,
        /// Supplied ordinal.
        actual: u32,
    },
    /// A non-root identity has no unique typed containment relationship.
    MissingContainment(AnySymbolId),
    /// More than one typed relationship claims the same contained member.
    DuplicateContainment(AnySymbolId),
    /// A lookup owner is missing from its interface surface.
    LookupOwnerOutOfBounds {
        /// Interface containing the malformed lookup.
        interface: ImportedInterfaceId,
        /// Missing interface-local owner.
        owner: InterfaceSymbolId,
    },
    /// A non-package, non-module symbol owns an exported ordinary-name surface.
    InvalidLookupOwner(AnySymbolId),
    /// A lookup target does not resolve through the complete external-key map.
    MissingLookupTarget(ExternalSymbolKey),
    /// One owner exports the same ordinary name more than once.
    DuplicateLookupName {
        /// Remapped package or module owner.
        owner: AnySymbolId,
        /// Duplicate ordinary name.
        name: SymbolName,
    },
    /// An identity kind cannot be represented by a compilation-wide symbol ID.
    UnsupportedSymbolKind(SymbolKind),
    /// A typed record could not be built from the supplied relationship graph.
    InvalidRecordRelationships(AnySymbolId),
}
