use std::collections::{BTreeMap, BTreeSet};

use bray_symbols::{
    CapturedValueOutcome, DependencyBorrowRelation, DependencyStoredValuePath, DependencySubject, TypeId,
};

use crate::StorageReferencePath;

/// A checked input-to-output reference relation of a selected call.
/// Address formation locates storage without equating a pointer with the borrowed value.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoundCallReference {
    relation: bray_symbols::DependencyBorrowRelation,
    preserves_value: bool,
    addressed_type: Option<bray_symbols::TypeId>,
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
            preserves_value,
            addressed_type,
        }
    }

    /// Returns the checked formal reference relation.
    pub const fn relation(&self) -> &bray_symbols::DependencyBorrowRelation {
        &self.relation
    }

    /// The reached storage type when forming an address or a bounded reference.
    pub const fn addressed_type(&self) -> Option<bray_symbols::TypeId> {
        self.addressed_type
    }

    /// Whether the output retains the input reference value, rather than only its target.
    pub const fn preserves_value(&self) -> bool {
        self.preserves_value && self.relation.is_exact()
    }
}

/// Typed routes shared by the effects and outcomes of an invocation.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct BoundCallStorage {
    subjects: BTreeMap<DependencySubject, (TypeId, StorageReferencePath)>,
    reference_types: BTreeMap<DependencyBorrowRelation, (bool, Option<TypeId>)>,
    stored_paths: BTreeMap<DependencyStoredValuePath, (TypeId, StorageReferencePath)>,
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
            (reference.preserves_value, reference.addressed_type),
        )).collect();

        Self { subjects, reference_types, stored_paths }
    }

    /// Subjects with checked routes from an actual input, static or result root.
    pub fn subjects(&self) -> impl Iterator<Item = (&DependencySubject, TypeId, &StorageReferencePath)> {
        self.subjects.iter().map(|(subject, (ty, path))| (subject, *ty, path))
    }

    /// The reached type and route of a subject from its root.
    pub fn subject(&self, subject: &DependencySubject) -> Option<(TypeId, &StorageReferencePath)> {
        self.subjects.get(subject).map(|(ty, path)| (*ty, path))
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
            if let Some(&(forwards_value, addressed_type)) = self.reference_types.get(&relation.clone().with_exactness(false)) {
                references.push(BoundCallReference::new(relation, forwards_value, addressed_type));
            } else {
                unavailable.insert(relation.exit().clone());
            }
        }

        references.retain(|reference| !unavailable.contains(reference.relation().exit()));

        references
    }
}
