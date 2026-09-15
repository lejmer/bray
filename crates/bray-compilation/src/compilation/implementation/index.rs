use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_base::shared_slice;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    BorrowKind, GenericConstraintTemplate, GenericDeclarationTemplate, GenericParameterSymbolId,
    ImplementationSymbolId, NamedTypeSymbolId, SemanticValueStore, SymbolKey,
    TargetPropertyDependency, TraitApplicationId, TraitSymbolId, TypeData, TypeId,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum SubjectBucket {
    Generic,
    Named(NamedTypeSymbolId),
    BorrowGeneric(BorrowKind),
    BorrowNamed(BorrowKind, NamedTypeSymbolId),
    Exact(TypeId),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum ImplementationFamilySubject {
    Named(NamedTypeSymbolId),
    Borrowed(BorrowKind, NamedTypeSymbolId),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct ImplementationFamilyKey {
    subject: ImplementationFamilySubject,
    trait_definition: TraitSymbolId,
}

impl ImplementationFamilyKey {
    pub(super) const fn new(
        subject: ImplementationFamilySubject,
        trait_definition: TraitSymbolId,
    ) -> Self {
        Self {
            subject,
            trait_definition,
        }
    }

    pub(super) const fn subject(self) -> ImplementationFamilySubject {
        self.subject
    }

    pub(super) const fn trait_definition(self) -> TraitSymbolId {
        self.trait_definition
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct HeaderBucket {
    trait_definition: TraitSymbolId,
    subject: SubjectBucket,
}

#[derive(Clone, Debug, Hash)]
pub(in crate::compilation) struct ImplementationHeader {
    key: SymbolKey,
    implementation: ImplementationSymbolId,
    subject: TypeId,
    trait_application: TraitApplicationId,
    generic: GenericDeclarationTemplate,
    target_dependencies: Arc<[TargetPropertyDependency]>,
    diagnostics: DiagnosticBag,
}

impl ImplementationHeader {
    pub(super) fn new(
        key: SymbolKey,
        implementation: ImplementationSymbolId,
        subject: TypeId,
        trait_application: TraitApplicationId,
        generic: GenericDeclarationTemplate,
        target_dependencies: impl IntoIterator<Item = TargetPropertyDependency>,
        diagnostics: DiagnosticBag,
    ) -> Self {
        Self {
            key,
            implementation,
            subject,
            trait_application,
            generic,
            target_dependencies: shared_slice(target_dependencies),
            diagnostics,
        }
    }

    pub(super) const fn key(&self) -> &SymbolKey {
        &self.key
    }

    pub(in crate::compilation) const fn implementation(&self) -> ImplementationSymbolId {
        self.implementation
    }

    pub(in crate::compilation) const fn subject(&self) -> TypeId {
        self.subject
    }

    pub(in crate::compilation) const fn trait_application(&self) -> TraitApplicationId {
        self.trait_application
    }

    pub(in crate::compilation) fn parameters(&self) -> &[GenericParameterSymbolId] {
        self.generic.parameters()
    }

    pub(super) const fn generic(&self) -> &GenericDeclarationTemplate {
        &self.generic
    }

    pub(super) fn constraints(&self) -> &[GenericConstraintTemplate] {
        self.generic.constraints()
    }

    pub(super) fn target_dependencies(&self) -> &[TargetPropertyDependency] {
        &self.target_dependencies
    }

    pub(super) const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    pub(super) fn family_key(
        &self,
        values: &SemanticValueStore,
    ) -> Option<ImplementationFamilyKey> {
        let application = values.trait_application_data(self.trait_application);

        family_subject(self.subject, values)
            .map(|subject| ImplementationFamilyKey::new(subject, application.definition()))
    }
}

#[derive(Debug, Hash)]
pub(in crate::compilation) struct ImplementationHeaderIndex {
    buckets: BTreeMap<HeaderBucket, Arc<[ImplementationHeader]>>,
    positions: BTreeMap<ImplementationSymbolId, (HeaderBucket, usize)>,
}

impl ImplementationHeaderIndex {
    pub(super) fn new(
        headers: impl IntoIterator<Item = ImplementationHeader>,
        values: &SemanticValueStore,
    ) -> Self {
        let mut buckets: BTreeMap<HeaderBucket, Vec<ImplementationHeader>> = BTreeMap::new();

        for header in headers {
            let application = values.trait_application_data(header.trait_application);
            let subject = subject_bucket(&header, values);

            let bucket = HeaderBucket {
                trait_definition: application.definition(),
                subject,
            };

            buckets.entry(bucket).or_default().push(header);
        }

        let buckets: BTreeMap<_, Arc<[ImplementationHeader]>> = buckets
            .into_iter()
            .map(|(key, mut headers)| {
                headers.sort_by(|left, right| left.key.cmp(&right.key));

                (key, Arc::from(headers))
            })
            .collect();

        let positions = buckets
            .iter()
            .flat_map(|(bucket, headers)| {
                headers
                    .iter()
                    .enumerate()
                    .map(|(index, header)| (header.implementation(), (*bucket, index)))
            })
            .collect();

        Self { buckets, positions }
    }

    pub(super) fn compatible_headers(
        &self,
        subject: TypeId,
        trait_definition: TraitSymbolId,
        values: &SemanticValueStore,
    ) -> Vec<&ImplementationHeader> {
        let buckets = query_buckets(subject, values);

        merge_headers(
            buckets
                .into_iter()
                .map(|subject| self.bucket(trait_definition, subject)),
        )
    }

    pub(in crate::compilation) fn headers(&self) -> Vec<&ImplementationHeader> {
        merge_headers(self.buckets.values().map(Arc::as_ref))
    }

    pub(in crate::compilation) fn header(
        &self,
        implementation: ImplementationSymbolId,
    ) -> Option<&ImplementationHeader> {
        let (bucket, index) = self.positions.get(&implementation)?;

        self.buckets.get(bucket)?.get(*index)
    }

    fn bucket(
        &self,
        trait_definition: TraitSymbolId,
        subject: SubjectBucket,
    ) -> &[ImplementationHeader] {
        self.buckets
            .get(&HeaderBucket {
                trait_definition,
                subject,
            })
            .map(Arc::as_ref)
            .unwrap_or(&[])
    }
}

fn family_subject(
    subject: TypeId,
    values: &SemanticValueStore,
) -> Option<ImplementationFamilySubject> {
    let subject = values.type_data(subject);

    match subject.as_ref() {
        TypeData::Named { definition, .. } => Some(ImplementationFamilySubject::Named(*definition)),
        TypeData::Borrow { kind, target } => {
            let target = values.type_data(*target);

            match target.as_ref() {
                TypeData::Named { definition, .. } => {
                    Some(ImplementationFamilySubject::Borrowed(*kind, *definition))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn merge_headers<'index>(
    buckets: impl IntoIterator<Item = &'index [ImplementationHeader]>,
) -> Vec<&'index ImplementationHeader> {
    let mut merged = buckets
        .into_iter()
        .flat_map(<[ImplementationHeader]>::iter)
        .collect::<Vec<_>>();

    merged.sort_by(|left, right| left.key.cmp(&right.key));

    merged
}

fn subject_bucket(header: &ImplementationHeader, values: &SemanticValueStore) -> SubjectBucket {
    let parameters = header.parameters().iter().copied().collect::<BTreeSet<_>>();

    classify_subject(header.subject, &parameters, values)
}

fn classify_subject(
    subject: TypeId,
    parameters: &BTreeSet<GenericParameterSymbolId>,
    values: &SemanticValueStore,
) -> SubjectBucket {
    let data = values.type_data(subject);

    match data.as_ref() {
        TypeData::TypeParameter(parameter)
            if parameters.contains(&GenericParameterSymbolId::Type(*parameter)) =>
        {
            SubjectBucket::Generic
        }
        TypeData::Named { definition, .. } => SubjectBucket::Named(*definition),
        TypeData::Borrow { kind, target } => {
            let target = values.type_data(*target);

            match target.as_ref() {
                TypeData::TypeParameter(parameter)
                    if parameters.contains(&GenericParameterSymbolId::Type(*parameter)) =>
                {
                    SubjectBucket::BorrowGeneric(*kind)
                }
                TypeData::Named { definition, .. } => {
                    SubjectBucket::BorrowNamed(*kind, *definition)
                }
                _ => SubjectBucket::Exact(subject),
            }
        }
        _ => SubjectBucket::Exact(subject),
    }
}

fn query_buckets(subject: TypeId, values: &SemanticValueStore) -> Vec<SubjectBucket> {
    let data = values.type_data(subject);

    let exact = match data.as_ref() {
        TypeData::Named { definition, .. } => SubjectBucket::Named(*definition),
        TypeData::Borrow { kind, target } => {
            let target = values.type_data(*target);

            match target.as_ref() {
                TypeData::Named { definition, .. } => {
                    SubjectBucket::BorrowNamed(*kind, *definition)
                }
                _ => SubjectBucket::Exact(subject),
            }
        }
        _ => SubjectBucket::Exact(subject),
    };

    let mut buckets = vec![exact, SubjectBucket::Generic];

    if let TypeData::Borrow { kind, .. } = data.as_ref() {
        buckets.push(SubjectBucket::BorrowGeneric(*kind));
    }

    buckets
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_symbols::{
        BorrowKind, GenericOwnerId, GenericSubstitutionData, NamedTypeSymbolId, SemanticValueStore,
        StructSymbolId, SymbolId, TypeData,
    };

    use super::{SubjectBucket, query_buckets};

    #[test]
    fn borrowed_requirements_search_unwrapped_and_borrow_generic_headers() {
        let values = semantic_values();

        let structure = StructSymbolId::from_symbol_id(SymbolId::new(1));

        let substitution = empty_substitution(&values, structure);

        let named = values
            .intern_type(TypeData::Named {
                definition: NamedTypeSymbolId::Struct(structure),
                substitution,
            })
            .unwrap_or_else(|error| panic!("named type must be valid: {error:?}"));

        let named_borrow = values
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Shared,
                target: named,
            })
            .unwrap_or_else(|error| panic!("named borrow must be valid: {error:?}"));

        assert_eq!(
            query_buckets(named_borrow, &values),
            vec![
                SubjectBucket::BorrowNamed(
                    BorrowKind::Shared,
                    NamedTypeSymbolId::Struct(structure),
                ),
                SubjectBucket::Generic,
                SubjectBucket::BorrowGeneric(BorrowKind::Shared),
            ]
        );

        let tuple = values
            .intern_type(TypeData::Tuple(Arc::from([])))
            .unwrap_or_else(|error| panic!("tuple type must be valid: {error:?}"));

        let tuple_borrow = values
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target: tuple,
            })
            .unwrap_or_else(|error| panic!("tuple borrow must be valid: {error:?}"));

        assert_eq!(
            query_buckets(tuple_borrow, &values),
            vec![
                SubjectBucket::Exact(tuple_borrow),
                SubjectBucket::Generic,
                SubjectBucket::BorrowGeneric(BorrowKind::Mutable),
            ]
        );
    }

    fn semantic_values() -> SemanticValueStore {
        SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic value store must build: {error:?}"))
    }

    fn empty_substitution(
        values: &SemanticValueStore,
        owner: StructSymbolId,
    ) -> bray_symbols::GenericSubstitutionId {
        let owner = GenericOwnerId::try_new(owner.into())
            .unwrap_or_else(|| panic!("structure must support generic substitution"));

        let substitution = GenericSubstitutionData::try_new(owner, [], [])
            .unwrap_or_else(|error| panic!("empty substitution must be valid: {error:?}"));

        values
            .intern_generic_substitution(substitution)
            .unwrap_or_else(|error| panic!("empty substitution must be interned: {error:?}"))
    }
}
