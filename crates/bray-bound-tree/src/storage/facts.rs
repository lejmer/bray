use std::sync::Arc;

use bray_symbols::{
    AnonymousCallableParameterSymbolId, AnyLocalSymbolId, AnySymbolId, CallableParameterSymbolId,
    LocalBindingSymbolId, ReceiverParameterSymbolId, StructFieldSymbolId,
    UnionPayloadFieldSymbolId,
};

use crate::{
    BorrowCapability, BorrowCapabilityId, BoundExpressionId, BoundUnitId, BoundUnitKind,
    StorageAccess, StorageAccessId, StorageIdentity, StorageIdentityId,
};

use super::support::checked_unit_entry;

/// A callable parameter category that has a checked relationship to unit-local storage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageParameter {
    /// An ordinary positional or named callable parameter.
    Callable(CallableParameterSymbolId),
    /// The receiver parameter of a non-static callable.
    Receiver(ReceiverParameterSymbolId),
    /// A parameter declared by an anonymous callable in this bound unit.
    Anonymous(AnonymousCallableParameterSymbolId),
}

impl StorageParameter {
    /// Classifies a compilation-wide parameter symbol that introduces callable storage.
    pub const fn from_surface_symbol(symbol: AnySymbolId) -> Option<Self> {
        match symbol {
            AnySymbolId::CallableParameter(parameter) => Some(Self::Callable(parameter)),
            AnySymbolId::ReceiverParameter(receiver) => Some(Self::Receiver(receiver)),
            _ => None,
        }
    }

    /// Classifies a unit-local parameter symbol that introduces callable storage.
    pub const fn from_local_symbol(symbol: AnyLocalSymbolId) -> Option<Self> {
        match symbol {
            AnyLocalSymbolId::AnonymousCallableParameter(parameter) => {
                Some(Self::Anonymous(parameter))
            }
            _ => None,
        }
    }
}

/// A compilation-wide declaration category that can identify reached storage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SurfaceStorageSymbol {
    /// A field selected from struct storage.
    StructField(StructFieldSymbolId),
    /// A field selected from the active payload of union storage.
    UnionPayloadField(UnionPayloadFieldSymbolId),
}

impl SurfaceStorageSymbol {
    /// Classifies a compilation-wide symbol that can participate in a storage relationship.
    pub const fn from_symbol(symbol: AnySymbolId) -> Option<Self> {
        match symbol {
            AnySymbolId::StructField(field) => Some(Self::StructField(field)),
            AnySymbolId::UnionPayloadField(field) => Some(Self::UnionPayloadField(field)),
            _ => None,
        }
    }
}

/// The exact storage reached through a local or surface relationship.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageReferent {
    /// Persistent storage introduced for the related semantic identity.
    Identity(StorageIdentityId),
    /// An existing evaluated access named by the related semantic identity.
    Access(StorageAccessId),
}

impl StorageReferent {
    pub(super) const fn unit(self) -> BoundUnitId {
        match self {
            Self::Identity(storage) => storage.unit(),
            Self::Access(access) => access.unit(),
        }
    }
}

/// One semantic occurrence that evaluates a checked storage access.
///
/// The variants describe source-shaped use only. They do not select move, copy, borrow,
/// consumption, or MIR addressability policy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageAccessOccurrence {
    /// An expression reads from reached storage.
    Read(BoundExpressionId),
    /// An expression establishes storage that a later operation writes through.
    Write(BoundExpressionId),
    /// A pattern-introduced local names reached storage.
    Binding(LocalBindingSymbolId),
    /// An assignment expression writes through its checked destination.
    Assignment(BoundExpressionId),
    /// An expression evaluates a projected access path.
    Projection(BoundExpressionId),
}

impl StorageAccessOccurrence {
    pub(super) fn is_valid_for(self, unit: BoundUnitId) -> bool {
        match self {
            Self::Read(expression)
            | Self::Write(expression)
            | Self::Assignment(expression)
            | Self::Projection(expression) => expression.unit() == unit,
            Self::Binding(binding) => binding.region().raw() == unit.raw(),
        }
    }
}

/// A checked parameter-to-storage relationship.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageParameterFact {
    parameter: StorageParameter,
    storage: StorageIdentityId,
}

impl StorageParameterFact {
    pub(super) const fn new(parameter: StorageParameter, storage: StorageIdentityId) -> Self {
        Self { parameter, storage }
    }

    /// Returns the exact parameter category and identity.
    pub const fn parameter(self) -> StorageParameter {
        self.parameter
    }

    /// Returns the persistent storage supplied through the parameter.
    pub const fn storage(self) -> StorageIdentityId {
        self.storage
    }
}

/// A checked relationship from one local binding to storage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LocalStorageFact {
    local: LocalBindingSymbolId,
    referent: StorageReferent,
}

impl LocalStorageFact {
    pub(super) const fn new(local: LocalBindingSymbolId, referent: StorageReferent) -> Self {
        Self { local, referent }
    }

    /// Returns the local binding that names the storage relationship.
    pub const fn local(self) -> LocalBindingSymbolId {
        self.local
    }

    /// Returns the exact persistent storage or evaluated access named by the local.
    pub const fn referent(self) -> StorageReferent {
        self.referent
    }
}

/// A checked relationship from one compilation-wide surface symbol to storage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SurfaceStorageFact {
    surface: SurfaceStorageSymbol,
    referent: StorageReferent,
}

impl SurfaceStorageFact {
    pub(super) const fn new(surface: SurfaceStorageSymbol, referent: StorageReferent) -> Self {
        Self { surface, referent }
    }

    /// Returns the surface symbol that names the storage relationship.
    pub const fn surface(self) -> SurfaceStorageSymbol {
        self.surface
    }

    /// Returns the exact persistent storage or evaluated access named by the symbol.
    pub const fn referent(self) -> StorageReferent {
        self.referent
    }
}

/// A checked occurrence-to-access relationship.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageAccessFact {
    occurrence: StorageAccessOccurrence,
    access: StorageAccessId,
}

impl StorageAccessFact {
    pub(super) const fn new(occurrence: StorageAccessOccurrence, access: StorageAccessId) -> Self {
        Self { occurrence, access }
    }

    /// Returns the exact source-semantic occurrence that evaluated the access.
    pub const fn occurrence(self) -> StorageAccessOccurrence {
        self.occurrence
    }

    /// Returns the occurrence-specific checked storage access.
    pub const fn access(self) -> StorageAccessId {
        self.access
    }
}

/// Durable storage facts for one exact checked semantic unit.
///
/// Identity and access sequences follow semantic evaluation order. Relationships follow their
/// typed semantic-key order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedStorageFacts {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    identities: Arc<[StorageIdentity]>,
    accesses: Arc<[StorageAccess]>,
    borrow_capabilities: Arc<[BorrowCapability]>,
    parameters: Arc<[StorageParameterFact]>,
    locals: Arc<[LocalStorageFact]>,
    surfaces: Arc<[SurfaceStorageFact]>,
    occurrences: Arc<[StorageAccessFact]>,
}

pub(super) struct CheckedStorageArenas {
    pub(super) identities: Vec<StorageIdentity>,
    pub(super) accesses: Vec<StorageAccess>,
    pub(super) borrow_capabilities: Vec<BorrowCapability>,
}

pub(super) struct CheckedStorageRelationships {
    pub(super) parameters: Vec<StorageParameterFact>,
    pub(super) locals: Vec<LocalStorageFact>,
    pub(super) surfaces: Vec<SurfaceStorageFact>,
    pub(super) occurrences: Vec<StorageAccessFact>,
}

impl CheckedStorageFacts {
    pub(super) fn new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        arenas: CheckedStorageArenas,
        relationships: CheckedStorageRelationships,
    ) -> Self {
        Self {
            unit,
            kind,
            identities: arenas.identities.into(),
            accesses: arenas.accesses.into(),
            borrow_capabilities: arenas.borrow_capabilities.into(),
            parameters: relationships.parameters.into(),
            locals: relationships.locals.into(),
            surfaces: relationships.surfaces.into(),
            occurrences: relationships.occurrences.into(),
        }
    }

    /// Returns the exact bound unit these facts describe.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the semantic category of the checked bound unit.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns persistent storage identities in semantic evaluation order.
    pub fn identities(&self) -> &[StorageIdentity] {
        &self.identities
    }

    /// Returns occurrence-specific storage accesses in deterministic evaluation order.
    pub fn accesses(&self) -> &[StorageAccess] {
        &self.accesses
    }

    /// Returns borrow capabilities in semantic evaluation order.
    pub fn borrow_capabilities(&self) -> &[BorrowCapability] {
        &self.borrow_capabilities
    }

    /// Returns canonical parameter-to-storage relationships.
    pub fn parameters(&self) -> &[StorageParameterFact] {
        &self.parameters
    }

    /// Returns canonical local-to-storage relationships.
    pub fn locals(&self) -> &[LocalStorageFact] {
        &self.locals
    }

    /// Returns canonical surface-symbol-to-storage relationships.
    pub fn surfaces(&self) -> &[SurfaceStorageFact] {
        &self.surfaces
    }

    /// Returns canonical source-occurrence-to-access relationships.
    pub fn occurrences(&self) -> &[StorageAccessFact] {
        &self.occurrences
    }

    /// Returns one persistent storage identity through checked typed access.
    pub fn identity(&self, id: StorageIdentityId) -> Option<StorageIdentity> {
        checked_unit_entry(self.unit, id.unit(), id.storage_index(), &self.identities).copied()
    }

    /// Returns one evaluated storage access through checked typed access.
    pub fn access(&self, id: StorageAccessId) -> Option<&StorageAccess> {
        checked_unit_entry(self.unit, id.unit(), id.storage_index(), &self.accesses)
    }

    /// Returns one borrow capability through checked typed access.
    pub fn borrow_capability(&self, id: BorrowCapabilityId) -> Option<BorrowCapability> {
        checked_unit_entry(
            self.unit,
            id.unit(),
            id.storage_index(),
            &self.borrow_capabilities,
        )
        .copied()
    }

    /// Returns the persistent storage supplied through a callable parameter.
    pub fn parameter_storage(&self, parameter: StorageParameter) -> Option<StorageIdentityId> {
        relationship(&self.parameters, parameter, |fact| fact.parameter())
            .map(|fact| fact.storage())
    }

    /// Returns the exact storage named by a local binding.
    pub fn local_storage(&self, local: LocalBindingSymbolId) -> Option<StorageReferent> {
        relationship(&self.locals, local, |fact| fact.local()).map(|fact| fact.referent())
    }

    /// Returns the exact storage named by a compilation-wide surface symbol.
    pub fn surface_storage(&self, surface: SurfaceStorageSymbol) -> Option<StorageReferent> {
        relationship(&self.surfaces, surface, |fact| fact.surface()).map(|fact| fact.referent())
    }

    /// Returns the checked storage access evaluated by one source-semantic occurrence.
    pub fn occurrence_access(
        &self,
        occurrence: StorageAccessOccurrence,
    ) -> Option<StorageAccessId> {
        relationship(&self.occurrences, occurrence, |fact| fact.occurrence())
            .map(|fact| fact.access())
    }
}

fn relationship<T, K: Ord + Copy>(facts: &[T], key: K, fact_key: impl Fn(&T) -> K) -> Option<&T> {
    facts
        .binary_search_by_key(&key, fact_key)
        .ok()
        .and_then(|index| facts.get(index))
}
