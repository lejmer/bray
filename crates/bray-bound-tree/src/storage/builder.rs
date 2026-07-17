use std::collections::{BTreeMap, btree_map::Entry};

use bray_symbols::{AnySymbolId, LocalBindingSymbolId};

use super::facts::{
    CheckedStorageArenas, CheckedStorageFacts, CheckedStorageRelationships, LocalStorageFact,
    StorageAccessFact, StorageAccessOccurrence, StorageParameter, StorageParameterFact,
    StorageReferent, SurfaceStorageFact,
};
use super::support::checked_unit_entry;
use crate::{
    BorrowCapability, BorrowCapabilityId, BoundUnitId, BoundUnitKind, StorageAccess,
    StorageAccessId, StorageAccessRoot, StorageIdentity, StorageIdentityId,
};

/// A typed failure while constructing checked storage facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckedStorageFactsBuildError {
    /// A dense storage-fact arena exceeded its compact ID representation.
    CapacityExceeded,
    /// A storage record references an identity owned by another bound unit.
    ForeignUnit,
    /// A storage record references an identity not yet present in its dense arena.
    MissingIdentity,
    /// A storage access references a borrow capability not yet present in its dense arena.
    MissingBorrowCapability,
    /// A borrow capability references an access not yet present in its dense arena.
    MissingAccess,
    /// A parameter relationship does not match the persistent identity's provenance.
    ParameterIdentityMismatch,
    /// A local relationship does not name local-owned storage or an existing access.
    LocalIdentityMismatch,
    /// A typed relationship key was already assigned in this unit.
    DuplicateRelationship,
}

/// Mutable deterministic construction state for immutable [`CheckedStorageFacts`].
#[derive(Debug)]
pub struct CheckedStorageFactsBuilder {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    identities: Vec<StorageIdentity>,
    accesses: Vec<StorageAccess>,
    borrow_capabilities: Vec<BorrowCapability>,
    parameters: BTreeMap<StorageParameter, StorageIdentityId>,
    locals: BTreeMap<LocalBindingSymbolId, StorageReferent>,
    surfaces: BTreeMap<AnySymbolId, StorageReferent>,
    occurrences: BTreeMap<StorageAccessOccurrence, StorageAccessId>,
}

impl CheckedStorageFactsBuilder {
    /// Creates empty task-local storage-fact state for one checked semantic unit.
    pub const fn new(unit: BoundUnitId, kind: BoundUnitKind) -> Self {
        Self {
            unit,
            kind,
            identities: Vec::new(),
            accesses: Vec::new(),
            borrow_capabilities: Vec::new(),
            parameters: BTreeMap::new(),
            locals: BTreeMap::new(),
            surfaces: BTreeMap::new(),
            occurrences: BTreeMap::new(),
        }
    }

    /// Returns the exact bound unit that will own the published facts.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Adds one persistent storage identity in deterministic checker order.
    pub fn push_identity(
        &mut self,
        identity: StorageIdentity,
    ) -> Result<StorageIdentityId, CheckedStorageFactsBuildError> {
        if !identity.is_valid_for(self.unit) {
            return Err(CheckedStorageFactsBuildError::ForeignUnit);
        }

        let slot = next_slot(self.identities.len())?;

        self.identities.push(identity);

        Ok(StorageIdentityId::from_storage_slot(self.unit, slot))
    }

    /// Adds one occurrence-specific checked storage access.
    pub fn push_access(
        &mut self,
        access: StorageAccess,
    ) -> Result<StorageAccessId, CheckedStorageFactsBuildError> {
        if !access.is_valid_for(self.unit) {
            return Err(CheckedStorageFactsBuildError::ForeignUnit);
        }

        self.validate_access_root(access.root())?;

        let slot = next_slot(self.accesses.len())?;

        self.accesses.push(access);

        Ok(StorageAccessId::from_storage_slot(self.unit, slot))
    }

    /// Adds one semantic borrow capability after its access and optional parent exist.
    pub fn push_borrow_capability(
        &mut self,
        capability: BorrowCapability,
    ) -> Result<BorrowCapabilityId, CheckedStorageFactsBuildError> {
        if !capability.is_valid_for(self.unit) {
            return Err(CheckedStorageFactsBuildError::ForeignUnit);
        }

        if self.access(capability.access()).is_none() {
            return Err(CheckedStorageFactsBuildError::MissingAccess);
        }

        if capability
            .derived_from()
            .is_some_and(|parent| self.borrow_capability(parent).is_none())
        {
            return Err(CheckedStorageFactsBuildError::MissingBorrowCapability);
        }

        let slot = next_slot(self.borrow_capabilities.len())?;

        self.borrow_capabilities.push(capability);

        Ok(BorrowCapabilityId::from_storage_slot(self.unit, slot))
    }

    /// Records the persistent storage supplied through one callable parameter.
    pub fn record_parameter_storage(
        &mut self,
        parameter: StorageParameter,
        storage: StorageIdentityId,
    ) -> Result<(), CheckedStorageFactsBuildError> {
        let Some(identity) = self.identity(storage) else {
            return self.missing_identity(storage);
        };

        let is_matching = matches!(
            (parameter, identity),
            (StorageParameter::Callable(expected), StorageIdentity::Parameter(actual))
                if expected == actual
        ) || matches!(
            (parameter, identity),
            (StorageParameter::Receiver(expected), StorageIdentity::Receiver(actual))
                if expected == actual
        );

        if !is_matching {
            return Err(CheckedStorageFactsBuildError::ParameterIdentityMismatch);
        }

        insert_unique(&mut self.parameters, parameter, storage)
    }

    /// Records the exact persistent storage or existing access named by a local binding.
    pub fn record_local_storage(
        &mut self,
        local: LocalBindingSymbolId,
        referent: StorageReferent,
    ) -> Result<(), CheckedStorageFactsBuildError> {
        if local.region().raw() != self.unit.raw() || referent.unit() != self.unit {
            return Err(CheckedStorageFactsBuildError::ForeignUnit);
        }

        match referent {
            StorageReferent::Identity(storage) => {
                let Some(identity) = self.identity(storage) else {
                    return self.missing_identity(storage);
                };

                if !matches!(identity, StorageIdentity::LocalOwned(_)) {
                    return Err(CheckedStorageFactsBuildError::LocalIdentityMismatch);
                }
            }
            StorageReferent::Access(access) if self.access(access).is_none() => {
                return Err(CheckedStorageFactsBuildError::MissingAccess);
            }
            StorageReferent::Access(_) => {}
        }

        insert_unique(&mut self.locals, local, referent)
    }

    /// Records the exact persistent storage or access named by a surface symbol.
    pub fn record_surface_storage(
        &mut self,
        surface: AnySymbolId,
        referent: StorageReferent,
    ) -> Result<(), CheckedStorageFactsBuildError> {
        self.validate_referent(referent)?;

        insert_unique(&mut self.surfaces, surface, referent)
    }

    /// Records the occurrence-specific access evaluated by one source-semantic operation.
    pub fn record_occurrence_access(
        &mut self,
        occurrence: StorageAccessOccurrence,
        access: StorageAccessId,
    ) -> Result<(), CheckedStorageFactsBuildError> {
        if !occurrence.is_valid_for(self.unit) || access.unit() != self.unit {
            return Err(CheckedStorageFactsBuildError::ForeignUnit);
        }

        if self.access(access).is_none() {
            return Err(CheckedStorageFactsBuildError::MissingAccess);
        }

        insert_unique(&mut self.occurrences, occurrence, access)
    }

    /// Freezes every arena and canonical relationship table into immutable checked facts.
    pub fn finish(self) -> CheckedStorageFacts {
        CheckedStorageFacts::new(
            self.unit,
            self.kind,
            CheckedStorageArenas {
                identities: self.identities,
                accesses: self.accesses,
                borrow_capabilities: self.borrow_capabilities,
            },
            CheckedStorageRelationships {
                parameters: self
                    .parameters
                    .into_iter()
                    .map(|(parameter, storage)| StorageParameterFact::new(parameter, storage))
                    .collect(),
                locals: self
                    .locals
                    .into_iter()
                    .map(|(local, referent)| LocalStorageFact::new(local, referent))
                    .collect(),
                surfaces: self
                    .surfaces
                    .into_iter()
                    .map(|(surface, referent)| SurfaceStorageFact::new(surface, referent))
                    .collect(),
                occurrences: self
                    .occurrences
                    .into_iter()
                    .map(|(occurrence, access)| StorageAccessFact::new(occurrence, access))
                    .collect(),
            },
        )
    }

    fn validate_access_root(
        &self,
        root: StorageAccessRoot,
    ) -> Result<(), CheckedStorageFactsBuildError> {
        match root {
            StorageAccessRoot::Storage(storage) | StorageAccessRoot::Recovery(storage) => {
                if self.identity(storage).is_none() {
                    return self.missing_identity(storage);
                }
            }
            StorageAccessRoot::Borrow(capability) => {
                if self.borrow_capability(capability).is_none() {
                    return Err(CheckedStorageFactsBuildError::MissingBorrowCapability);
                }
            }
            StorageAccessRoot::OwnedIndirection { storage, .. } => {
                if self.identity(storage).is_none() {
                    return self.missing_identity(storage);
                }
            }
        }

        Ok(())
    }

    fn validate_referent(
        &self,
        referent: StorageReferent,
    ) -> Result<(), CheckedStorageFactsBuildError> {
        if referent.unit() != self.unit {
            return Err(CheckedStorageFactsBuildError::ForeignUnit);
        }

        match referent {
            StorageReferent::Identity(storage) if self.identity(storage).is_none() => {
                self.missing_identity(storage)
            }
            StorageReferent::Access(access) if self.access(access).is_none() => {
                Err(CheckedStorageFactsBuildError::MissingAccess)
            }
            StorageReferent::Identity(_) | StorageReferent::Access(_) => Ok(()),
        }
    }

    fn identity(&self, id: StorageIdentityId) -> Option<StorageIdentity> {
        checked_unit_entry(self.unit, id.unit(), id.storage_index(), &self.identities).copied()
    }

    fn access(&self, id: StorageAccessId) -> Option<&StorageAccess> {
        checked_unit_entry(self.unit, id.unit(), id.storage_index(), &self.accesses)
    }

    fn borrow_capability(&self, id: BorrowCapabilityId) -> Option<BorrowCapability> {
        checked_unit_entry(
            self.unit,
            id.unit(),
            id.storage_index(),
            &self.borrow_capabilities,
        )
        .copied()
    }

    fn missing_identity<T>(
        &self,
        storage: StorageIdentityId,
    ) -> Result<T, CheckedStorageFactsBuildError> {
        if storage.unit() == self.unit {
            Err(CheckedStorageFactsBuildError::MissingIdentity)
        } else {
            Err(CheckedStorageFactsBuildError::ForeignUnit)
        }
    }
}

fn next_slot(length: usize) -> Result<u32, CheckedStorageFactsBuildError> {
    u32::try_from(length).map_err(|_| CheckedStorageFactsBuildError::CapacityExceeded)
}

fn insert_unique<K: Ord, V>(
    relationships: &mut BTreeMap<K, V>,
    key: K,
    value: V,
) -> Result<(), CheckedStorageFactsBuildError> {
    match relationships.entry(key) {
        Entry::Vacant(entry) => {
            entry.insert(value);
            Ok(())
        }
        Entry::Occupied(_) => Err(CheckedStorageFactsBuildError::DuplicateRelationship),
    }
}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;
    use bray_symbols::{
        BorrowKind, CallableParameterSymbolId, FunctionSymbolId, LocalScopeBoundary,
        LocalSymbolRegionId, LocalSymbolRegionKey, LocalSymbolRegionRole,
        LocalSymbolSnapshotBuilder, ReceiverParameterSymbolId, SymbolId, SymbolKind, SymbolName,
    };

    use super::{CheckedStorageFactsBuildError, CheckedStorageFactsBuilder};
    use crate::test_support::{error_type, source_anchor, symbol_key};
    use crate::{
        BorrowCapability, BoundDependencyContractId, BoundExpressionId, BoundUnitId, BoundUnitKind,
        StorageAccess, StorageAccessOccurrence, StorageAccessRoot, StorageIdentity,
        StorageParameter, StorageReferent,
    };

    #[test]
    fn facts_publish_dense_occurrence_specific_accesses_and_typed_relationships() {
        let unit = BoundUnitId::new(7);
        let parameter = CallableParameterSymbolId::from_symbol_id(SymbolId::new(4));
        let receiver = ReceiverParameterSymbolId::from_symbol_id(SymbolId::new(3));
        let surface = FunctionSymbolId::from_symbol_id(SymbolId::new(2));
        let read = BoundExpressionId::from_slot(unit, 0);
        let projection = BoundExpressionId::from_slot(unit, 1);
        let borrow_read = BoundExpressionId::from_slot(unit, 2);
        let write = BoundExpressionId::from_slot(unit, 3);
        let assignment = BoundExpressionId::from_slot(unit, 4);
        let local = local_binding(unit);
        let mut builder = CheckedStorageFactsBuilder::new(unit, BoundUnitKind::CallableBody);

        let parameter_storage = push_identity(&mut builder, StorageIdentity::Parameter(parameter));
        let receiver_storage = push_identity(&mut builder, StorageIdentity::Receiver(receiver));
        let first_access = push_access(&mut builder, parameter_storage, read);
        let second_access = push_access(&mut builder, parameter_storage, projection);
        let local_storage = push_identity(&mut builder, StorageIdentity::LocalOwned(read.into()));
        let local_access = push_access(&mut builder, local_storage, read);
        let write_access = push_access(&mut builder, parameter_storage, write);
        let assignment_access = push_access(&mut builder, local_storage, assignment);
        let capability = match builder.push_borrow_capability(BorrowCapability::new(
            BorrowKind::Shared,
            first_access,
            source_anchor(),
            BoundDependencyContractId::from_slot(unit, 0),
            None,
        )) {
            Ok(capability) => capability,
            Err(error) => panic!("test borrow capability must build: {error:?}"),
        };

        let borrowed_access = match builder.push_access(StorageAccess::new(
            StorageAccessRoot::Borrow(capability),
            [],
            error_type(),
            source_anchor(),
            false,
        )) {
            Ok(access) => access,
            Err(error) => panic!("test borrow-derived access must build: {error:?}"),
        };

        assert_eq!(
            builder.record_parameter_storage(
                StorageParameter::Callable(parameter),
                parameter_storage,
            ),
            Ok(())
        );
        assert_eq!(
            builder
                .record_parameter_storage(StorageParameter::Receiver(receiver), receiver_storage,),
            Ok(())
        );
        assert_eq!(
            builder.record_surface_storage(surface.into(), StorageReferent::Access(second_access)),
            Ok(())
        );
        assert_eq!(
            builder.record_local_storage(local, StorageReferent::Identity(local_storage)),
            Ok(())
        );
        assert_eq!(
            builder.record_occurrence_access(
                StorageAccessOccurrence::Projection(projection),
                second_access,
            ),
            Ok(())
        );
        assert_eq!(
            builder.record_occurrence_access(StorageAccessOccurrence::Read(read), first_access),
            Ok(())
        );
        assert_eq!(
            builder
                .record_occurrence_access(StorageAccessOccurrence::Binding(local), local_access,),
            Ok(())
        );
        assert_eq!(
            builder.record_occurrence_access(
                StorageAccessOccurrence::Read(borrow_read),
                borrowed_access,
            ),
            Ok(())
        );
        assert_eq!(
            builder.record_occurrence_access(StorageAccessOccurrence::Write(write), write_access),
            Ok(())
        );
        assert_eq!(
            builder.record_occurrence_access(
                StorageAccessOccurrence::Assignment(assignment),
                assignment_access,
            ),
            Ok(())
        );

        let facts = builder.finish();

        assert_eq!(facts.unit(), unit);
        assert_eq!(facts.kind(), BoundUnitKind::CallableBody);
        assert_eq!(facts.identities().len(), 3);
        assert_eq!(facts.accesses().len(), 6);
        assert_eq!(
            facts.borrow_capabilities(),
            &[BorrowCapability::new(
                BorrowKind::Shared,
                first_access,
                source_anchor(),
                BoundDependencyContractId::from_slot(unit, 0),
                None,
            )]
        );
        assert_ne!(first_access, second_access);
        assert_eq!(
            facts.access(first_access).map(StorageAccess::root),
            facts.access(second_access).map(StorageAccess::root)
        );
        assert_eq!(
            facts.parameter_storage(StorageParameter::Callable(parameter)),
            Some(parameter_storage)
        );
        assert_eq!(
            facts.surface_storage(surface.into()),
            Some(StorageReferent::Access(second_access))
        );
        assert_eq!(
            facts.local_storage(local),
            Some(StorageReferent::Identity(local_storage))
        );
        assert_eq!(
            facts.occurrence_access(StorageAccessOccurrence::Read(read)),
            Some(first_access)
        );
        assert_eq!(
            facts.occurrence_access(StorageAccessOccurrence::Write(write)),
            Some(write_access)
        );
        assert_eq!(
            facts.occurrence_access(StorageAccessOccurrence::Assignment(assignment)),
            Some(assignment_access)
        );
        assert_eq!(
            facts.occurrences()[0].occurrence(),
            StorageAccessOccurrence::Read(read)
        );
    }

    #[test]
    fn construction_rejects_foreign_missing_duplicate_and_incoherent_relationships() {
        let unit = BoundUnitId::new(8);
        let parameter = CallableParameterSymbolId::from_symbol_id(SymbolId::new(1));
        let receiver = ReceiverParameterSymbolId::from_symbol_id(SymbolId::new(2));
        let expression = BoundExpressionId::from_slot(unit, 0);
        let mut builder = CheckedStorageFactsBuilder::new(unit, BoundUnitKind::CallableBody);
        let storage = push_identity(&mut builder, StorageIdentity::Parameter(parameter));
        let access = push_access(&mut builder, storage, expression);
        let replacement_access =
            push_access(&mut builder, storage, BoundExpressionId::from_slot(unit, 1));

        assert_eq!(
            builder.record_parameter_storage(StorageParameter::Receiver(receiver), storage),
            Err(CheckedStorageFactsBuildError::ParameterIdentityMismatch)
        );
        assert_eq!(
            builder.record_occurrence_access(StorageAccessOccurrence::Read(expression), access),
            Ok(())
        );
        assert_eq!(
            builder.record_occurrence_access(
                StorageAccessOccurrence::Read(expression),
                replacement_access,
            ),
            Err(CheckedStorageFactsBuildError::DuplicateRelationship)
        );

        let foreign_expression = BoundExpressionId::from_slot(BoundUnitId::new(9), 0);
        let recovered_access = StorageAccess::new(
            StorageAccessRoot::Storage(storage),
            [],
            error_type(),
            source_anchor(),
            true,
        );

        assert_eq!(
            builder.record_occurrence_access(
                StorageAccessOccurrence::Write(foreign_expression),
                access,
            ),
            Err(CheckedStorageFactsBuildError::ForeignUnit)
        );

        let recovered = match builder.push_access(recovered_access) {
            Ok(access) => access,
            Err(error) => panic!("recovered access must remain publishable: {error:?}"),
        };

        let facts = builder.finish();

        assert_eq!(
            facts.occurrence_access(StorageAccessOccurrence::Read(expression)),
            Some(access)
        );
        assert!(
            facts
                .access(recovered)
                .is_some_and(StorageAccess::is_recovered)
        );
    }

    #[test]
    fn published_facts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<crate::CheckedStorageFacts>();
    }

    fn push_identity(
        builder: &mut CheckedStorageFactsBuilder,
        identity: StorageIdentity,
    ) -> crate::StorageIdentityId {
        match builder.push_identity(identity) {
            Ok(storage) => storage,
            Err(error) => panic!("test storage identity must build: {error:?}"),
        }
    }

    fn push_access(
        builder: &mut CheckedStorageFactsBuilder,
        storage: crate::StorageIdentityId,
        expression: BoundExpressionId,
    ) -> crate::StorageAccessId {
        let access = StorageAccess::new(
            StorageAccessRoot::Storage(storage),
            [],
            error_type(),
            source_anchor(),
            false,
        );

        match builder.push_access(access) {
            Ok(access) => {
                assert_eq!(access.unit(), expression.unit());
                access
            }
            Err(error) => panic!("test storage access must build: {error:?}"),
        }
    }

    fn local_binding(unit: BoundUnitId) -> bray_symbols::LocalBindingSymbolId {
        let source = source_anchor().syntax();
        let Some(region_key) = LocalSymbolRegionKey::try_new(
            symbol_key(SymbolKind::Function, 0),
            LocalSymbolRegionRole::CallableBody,
            [source],
            None,
        ) else {
            panic!("test local region key must build");
        };

        let mut locals =
            LocalSymbolSnapshotBuilder::new(LocalSymbolRegionId::new(unit.raw()), region_key);

        let scope = match locals.push_scope(None, LocalScopeBoundary::Root, source, TextSize::ZERO)
        {
            Ok(scope) => scope,
            Err(error) => panic!("test local scope must build: {error:?}"),
        };

        let Some(name) = SymbolName::try_new("local") else {
            panic!("test local name must build");
        };

        match locals.push_binding(scope, name, [source], None, false) {
            Ok(local) => local,
            Err(error) => panic!("test local binding must build: {error:?}"),
        }
    }
}
