use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_symbols::{
    CapturedValueOutcome, DependencyBorrowRelation, DependencyStorageEffect, DependencyStoredValuePath, DependencySubject, TypeId,
};

use crate::StorageReferencePath;

/// A checked input-to-output reference relation of a selected call.
/// Address formation locates storage without equating a pointer with the borrowed value.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoundCallReference {
    relation: bray_symbols::DependencyBorrowRelation,
    route: ReferenceOutputRoute,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ReferenceOutputRoute {
    preserves_value: bool,
    addressed_type: Option<bray_symbols::TypeId>,
    exposes_borrow: bool,
    reads: Option<StorageReferencePath>,
    target: Option<StorageReferencePath>,
}

impl BoundCallReference {
    /// Records whether the reference forwards an input value or forms an address.
    pub const fn new(
        relation: bray_symbols::DependencyBorrowRelation,
        preserves_value: bool,
        addressed_type: Option<bray_symbols::TypeId>,
    ) -> Self {
        Self {
            relation,
            route: ReferenceOutputRoute { preserves_value, addressed_type, exposes_borrow: false, reads: None, target: None },
        }
    }

    /// Attaches the typed reads from the actual source and from its reached target.
    pub fn with_routes(mut self, reads: Option<StorageReferencePath>, target: Option<StorageReferencePath>, exposes_borrow: bool) -> Self {
        self.route.reads = reads;
        self.route.target = target;
        self.route.exposes_borrow = exposes_borrow;

        self
    }

    /// Returns the checked formal reference relation.
    pub const fn relation(&self) -> &bray_symbols::DependencyBorrowRelation {
        &self.relation
    }

    /// The reached storage type when forming an address or a bounded reference.
    pub const fn addressed_type(&self) -> Option<bray_symbols::TypeId> {
        self.route.addressed_type
    }

    /// Whether this output exposes an existing language borrow as a raw address.
    pub const fn exposes_borrow(&self) -> bool {
        self.route.exposes_borrow
    }

    /// Whether the output retains the input reference value, rather than only its target.
    pub const fn preserves_value(&self) -> bool {
        self.route.preserves_value && self.relation.is_exact()
    }

    /// Descriptor reads before the source supplies the returned reference.
    pub fn reads(&self) -> Option<&StorageReferencePath> { self.route.reads.as_ref() }

    /// The checked selections and reads after the source's reached address.
    pub fn target_path(&self) -> Option<&StorageReferencePath> { self.route.target.as_ref() }
}

/// Typed routes shared by the effects and outcomes of an invocation.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct BoundCallStorage {
    subjects: BTreeMap<DependencySubject, (TypeId, StorageReferencePath)>,
    reference_types: BTreeMap<DependencyBorrowRelation, ReferenceOutputRoute>,
    stored_paths: BTreeMap<DependencyStoredValuePath, (TypeId, StorageReferencePath)>,
    effect_paths: BTreeMap<DependencyStorageEffect, StorageReferencePath>,
    write_paths: BTreeMap<DependencySubject, (TypeId, StorageReferencePath)>,
    returned_values: BTreeMap<bray_symbols::ConstantTermId,
        BTreeMap<Arc<[bray_symbols::ConstantProjectionKind]>, crate::BoundReturnedValue>>,
}

impl BoundCallStorage {
    /// Retains the checked routes and reference metadata from the selected signature.
    pub fn new(
        subjects: BTreeMap<DependencySubject, (TypeId, StorageReferencePath)>,
        references: impl IntoIterator<Item = BoundCallReference>,
        stored_paths: BTreeMap<DependencyStoredValuePath, (TypeId, StorageReferencePath)>,
    ) -> Self {
        let reference_types = references.into_iter().map(|reference| (
            reference.relation.with_exactness(false),
            reference.route,
        )).collect();

        Self { subjects, reference_types, stored_paths, effect_paths: Default::default(), write_paths: Default::default(), returned_values: Default::default() }
    }

    /// Shares the checked shape of each possible input transfer with direct and deferred calls.
    pub fn with_returned_values(
        mut self,
        values: impl IntoIterator<Item = (bray_symbols::DependencyOwnedTransfer, crate::BoundReturnedValue)>,
    ) -> Self {
        for (transfer, route) in values {
            self.returned_values.entry(transfer.input()).or_default().insert(transfer.output().into(), route);
        }

        self
    }

    /// Returns the prepared route and role of the selected transfer.
    pub fn returned_value(&self, transfer: &bray_symbols::DependencyOwnedTransfer) -> &crate::BoundReturnedValue {
        self.returned_values.get(&transfer.input()).and_then(|paths| paths.get(transfer.output()))
            .expect("selected owned transfer retains its checked value description")
    }

    /// Retains the typed suffix followed after the first admitted descriptor.
    pub fn with_effect_paths(mut self, paths: BTreeMap<DependencyStorageEffect, StorageReferencePath>) -> Self {
        self.effect_paths = paths;

        self
    }

    /// Reads the checked continuation of this effect's admitted reference.
    pub fn effect_path(&self, effect: &DependencyStorageEffect) -> Option<&StorageReferencePath> {
        self.effect_paths.get(effect.retirement_parts().0)
    }

    /// Prepared effect continuations and the descriptor that supplies their first address.
    pub fn effect_target_paths(&self) -> impl Iterator<Item = (TypeId, &StorageReferencePath)> {
        self.effect_paths.iter().filter_map(|(effect, path)| {
            let (ty, _) = self.subject(effect.subjects().next()?)?;

            Some((ty, path))
        })
    }

    /// Subjects with checked routes from an actual input, static or result root.
    pub fn subjects(&self) -> impl Iterator<Item = (&DependencySubject, TypeId, &StorageReferencePath)> {
        self.subjects.iter().map(|(subject, (ty, path))| (subject, *ty, path))
    }

    /// The reached type and route of a subject from its root.
    pub fn subject(&self, subject: &DependencySubject) -> Option<(TypeId, &StorageReferencePath)> {
        self.subjects.get(subject).map(|(ty, path)| (*ty, path))
    }

    /// Prepared result routes after following the entry reference and any intermediate loans.
    pub fn reference_target_paths(&self) -> impl Iterator<Item = (&DependencySubject, TypeId, &StorageReferencePath)> {
        self.reference_types.iter().filter_map(|(relation, route)| {
            let entry = relation.entry()?;

            let (ty, _) = self.subject(entry)?;

            let source = route.reads.as_ref().and_then(|path| path.reads().next_back())
                .map_or(ty, |(_, _, target, _)| target);

            Some((entry, source, route.target.as_ref()?))
        })
    }

    /// Shares the checked routes for writing into incoming loans.
    pub fn with_write_paths(mut self, paths: BTreeMap<DependencySubject, (TypeId, StorageReferencePath)>) -> Self {
        self.write_paths = paths;

        self
    }

    /// A committed write follows its checked mutable loan, including stored loans.
    pub fn mutable_write(&self, subject: &DependencySubject) -> Option<(TypeId, &StorageReferencePath)> {
        self.write_paths.get(subject).map(|(ty, path)| (*ty, path))
    }

    /// The reached type and route of an endpoint, including terminal loads.
    pub fn stored_value_path(&self, endpoint: &DependencyStoredValuePath) -> Option<(TypeId, &StorageReferencePath)> {
        self.stored_paths.get(endpoint).map(|(ty, path)| (*ty, path))
    }

    /// The typed reference writes established by the selected normal outcomes.
    pub fn reference_outputs<'a>(
        &self,
        outcomes: impl IntoIterator<Item = &'a CapturedValueOutcome>,
    ) -> Vec<BoundCallReference> {
        let relations = CapturedValueOutcome::reference_origins(outcomes, DependencySubject::contains);
        let mut unavailable = BTreeSet::new();
        let mut references = Vec::with_capacity(relations.len());

        for relation in relations.iter().cloned() {
            if let Some(route) = self.reference_types.get(&relation.clone().with_exactness(false)) {
                references.push(BoundCallReference { relation, route: route.clone() });
            } else {
                unavailable.insert(relation.exit().clone());
            }
        }

        references.retain(|reference| !unavailable.contains(reference.relation().exit()));

        references
    }
}
