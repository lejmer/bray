use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{
    BorrowKind, GenericConstraintTemplate, GenericParameterSymbolId, ImplementationSymbolId,
    NamedTypeSymbolId, SemanticValueStore, SemanticValueStoreError, SymbolKey,
    TargetFactDependency, TraitApplicationId, TraitSymbolId, TypeData, TypeId,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SubjectBucket {
    Generic,
    Named(NamedTypeSymbolId),
    BorrowGeneric(BorrowKind),
    BorrowNamed(BorrowKind, NamedTypeSymbolId),
    Exact(TypeId),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct HeaderBucket {
    trait_definition: TraitSymbolId,
    subject: SubjectBucket,
}

#[derive(Clone, Debug)]
pub(super) struct ImplementationHeader {
    key: SymbolKey,
    implementation: ImplementationSymbolId,
    subject: TypeId,
    trait_application: TraitApplicationId,
    parameters: Arc<[GenericParameterSymbolId]>,
    constraints: Arc<[GenericConstraintTemplate]>,
    target_dependencies: Arc<[TargetFactDependency]>,
}

impl ImplementationHeader {
    pub(super) fn new(
        key: SymbolKey,
        implementation: ImplementationSymbolId,
        subject: TypeId,
        trait_application: TraitApplicationId,
        parameters: impl IntoIterator<Item = GenericParameterSymbolId>,
        constraints: impl IntoIterator<Item = GenericConstraintTemplate>,
        target_dependencies: impl IntoIterator<Item = TargetFactDependency>,
    ) -> Self {
        Self {
            key,
            implementation,
            subject,
            trait_application,
            parameters: shared_slice(parameters),
            constraints: shared_slice(constraints),
            target_dependencies: shared_slice(target_dependencies),
        }
    }

    pub(super) const fn key(&self) -> &SymbolKey {
        &self.key
    }

    pub(super) const fn implementation(&self) -> ImplementationSymbolId {
        self.implementation
    }

    pub(super) const fn subject(&self) -> TypeId {
        self.subject
    }

    pub(super) const fn trait_application(&self) -> TraitApplicationId {
        self.trait_application
    }

    pub(super) fn parameters(&self) -> &[GenericParameterSymbolId] {
        &self.parameters
    }

    pub(super) fn constraints(&self) -> &[GenericConstraintTemplate] {
        &self.constraints
    }

    pub(super) fn target_dependencies(&self) -> &[TargetFactDependency] {
        &self.target_dependencies
    }
}

#[derive(Debug)]
pub(in crate::compilation) struct ImplementationHeaderIndex {
    buckets: BTreeMap<HeaderBucket, Arc<[ImplementationHeader]>>,
}

impl ImplementationHeaderIndex {
    pub(super) fn try_new(
        headers: impl IntoIterator<Item = ImplementationHeader>,
        values: &SemanticValueStore,
    ) -> Result<Self, SemanticValueStoreError> {
        let mut buckets: BTreeMap<HeaderBucket, Vec<ImplementationHeader>> = BTreeMap::new();

        for header in headers {
            let application = values.trait_application_data(header.trait_application)?;
            let subject = subject_bucket(&header, values)?;

            let bucket = HeaderBucket {
                trait_definition: application.definition(),
                subject,
            };

            buckets.entry(bucket).or_default().push(header);
        }

        let buckets = buckets
            .into_iter()
            .map(|(key, mut headers)| {
                headers.sort_by(|left, right| left.key.cmp(&right.key));

                (key, Arc::from(headers))
            })
            .collect();

        Ok(Self { buckets })
    }

    pub(super) fn compatible_headers(
        &self,
        subject: TypeId,
        trait_definition: TraitSymbolId,
        values: &SemanticValueStore,
    ) -> Result<Vec<&ImplementationHeader>, SemanticValueStoreError> {
        let [exact, generic] = query_buckets(subject, values)?;
        let exact = self.bucket(trait_definition, exact);
        let generic = self.bucket(trait_definition, generic);

        Ok(merge_headers(exact, generic))
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

fn merge_headers<'index>(
    first: &'index [ImplementationHeader],
    second: &'index [ImplementationHeader],
) -> Vec<&'index ImplementationHeader> {
    let mut merged = Vec::with_capacity(first.len() + second.len());
    let mut first = first.iter().peekable();
    let mut second = second.iter().peekable();

    while first.peek().is_some() || second.peek().is_some() {
        let take_first = match (first.peek(), second.peek()) {
            (Some(first), Some(second)) => first.key <= second.key,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };

        let header = if take_first {
            first.next()
        } else {
            second.next()
        };

        if let Some(header) = header {
            merged.push(header);
        }
    }

    merged
}

fn subject_bucket(
    header: &ImplementationHeader,
    values: &SemanticValueStore,
) -> Result<SubjectBucket, SemanticValueStoreError> {
    let parameters = header.parameters.iter().copied().collect::<BTreeSet<_>>();

    classify_subject(header.subject, &parameters, values)
}

fn classify_subject(
    subject: TypeId,
    parameters: &BTreeSet<GenericParameterSymbolId>,
    values: &SemanticValueStore,
) -> Result<SubjectBucket, SemanticValueStoreError> {
    let data = values.type_data(subject)?;

    match data.as_ref() {
        TypeData::TypeParameter(parameter)
            if parameters.contains(&GenericParameterSymbolId::Type(*parameter)) =>
        {
            Ok(SubjectBucket::Generic)
        }
        TypeData::Named { definition, .. } => Ok(SubjectBucket::Named(*definition)),
        TypeData::Borrow { kind, target } => {
            let target = values.type_data(*target)?;

            match target.as_ref() {
                TypeData::TypeParameter(parameter)
                    if parameters.contains(&GenericParameterSymbolId::Type(*parameter)) =>
                {
                    Ok(SubjectBucket::BorrowGeneric(*kind))
                }
                TypeData::Named { definition, .. } => {
                    Ok(SubjectBucket::BorrowNamed(*kind, *definition))
                }
                _ => Ok(SubjectBucket::Exact(subject)),
            }
        }
        _ => Ok(SubjectBucket::Exact(subject)),
    }
}

fn query_buckets(
    subject: TypeId,
    values: &SemanticValueStore,
) -> Result<[SubjectBucket; 2], SemanticValueStoreError> {
    let data = values.type_data(subject)?;

    let exact = match data.as_ref() {
        TypeData::Named { definition, .. } => SubjectBucket::Named(*definition),
        TypeData::Borrow { kind, target } => {
            let target = values.type_data(*target)?;

            match target.as_ref() {
                TypeData::Named { definition, .. } => {
                    SubjectBucket::BorrowNamed(*kind, *definition)
                }
                _ => SubjectBucket::Exact(subject),
            }
        }
        _ => SubjectBucket::Exact(subject),
    };

    let generic = match exact {
        SubjectBucket::BorrowNamed(kind, _) => SubjectBucket::BorrowGeneric(kind),
        _ => SubjectBucket::Generic,
    };

    Ok([exact, generic])
}
