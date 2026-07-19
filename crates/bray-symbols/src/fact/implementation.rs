use std::{collections::BTreeMap, collections::btree_map::Entry, sync::Arc};

use bray_base::shared_slice;

use crate::{
    ImplementationInstanceId, SymbolKey, TraitApplicationId, TypeExpressionTemplate, TypeId,
};

/// The source type template named by one implementation declaration.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationSubjectTemplate {
    ty: TypeExpressionTemplate,
}

impl ImplementationSubjectTemplate {
    /// Creates an implementation subject template.
    pub const fn new(ty: TypeExpressionTemplate) -> Self {
        Self { ty }
    }

    /// Returns the implemented type template.
    pub const fn ty(&self) -> &TypeExpressionTemplate {
        &self.ty
    }
}

/// The checked subject type implemented by one implementation declaration.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationSubject {
    ty: TypeId,
}

impl ImplementationSubject {
    /// Creates a checked implementation subject.
    pub const fn new(ty: TypeId) -> Self {
        Self { ty }
    }

    /// Returns the implemented semantic type.
    pub const fn ty(self) -> TypeId {
        self.ty
    }
}

/// The stable semantic key under which one implementation participates in coherence.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationCoherenceKey {
    subject: TypeId,
    trait_application: Option<TraitApplicationId>,
}

impl ImplementationCoherenceKey {
    /// Creates a coherence key from the checked implementation surface.
    pub const fn new(subject: TypeId, trait_application: Option<TraitApplicationId>) -> Self {
        Self {
            subject,
            trait_application,
        }
    }

    /// Returns the exact implemented subject type.
    pub const fn subject(self) -> TypeId {
        self.subject
    }

    /// Returns the implemented trait application for a trait implementation.
    pub const fn trait_application(self) -> Option<TraitApplicationId> {
        self.trait_application
    }
}

/// The semantic inputs that select an implementation witness.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationSelectionKey {
    subject: TypeId,
    trait_application: TraitApplicationId,
}

impl ImplementationSelectionKey {
    /// Creates an implementation-selection key.
    pub const fn new(subject: TypeId, trait_application: TraitApplicationId) -> Self {
        Self {
            subject,
            trait_application,
        }
    }

    /// Returns the exact semantic subject type.
    pub const fn subject(self) -> TypeId {
        self.subject
    }

    /// Returns the exact applied trait requirement.
    pub const fn trait_application(self) -> TraitApplicationId {
        self.trait_application
    }
}

/// The deterministic result of selecting an implementation witness.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ImplementationSelection {
    /// One exact implementation witness was selected.
    Selected(ImplementationInstanceId),
    /// No applicable implementation exists.
    Unavailable,
    /// Several equally applicable implementation witnesses remain.
    Ambiguous(ImplementationAmbiguity),
}

impl ImplementationSelection {
    /// Creates an ambiguity normalized by stable semantic symbol key.
    pub fn ambiguous(
        candidates: impl IntoIterator<Item = ImplementationCandidate>,
    ) -> Result<Self, ImplementationAmbiguityError> {
        ImplementationAmbiguity::try_new(candidates).map(Self::Ambiguous)
    }

    /// Returns ambiguous candidates or an empty slice for another selection state.
    pub fn ambiguous_candidates(&self) -> &[ImplementationCandidate] {
        match self {
            Self::Ambiguous(ambiguity) => ambiguity.candidates(),
            Self::Selected(_) | Self::Unavailable => &[],
        }
    }
}

/// One applicable implementation instance and its stable semantic definition key.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationCandidate {
    key: SymbolKey,
    instance: ImplementationInstanceId,
}

impl ImplementationCandidate {
    /// Creates one applicable implementation candidate.
    pub const fn new(key: SymbolKey, instance: ImplementationInstanceId) -> Self {
        Self { key, instance }
    }

    /// Returns the stable semantic definition key used for canonical ordering.
    pub const fn key(&self) -> &SymbolKey {
        &self.key
    }

    /// Returns the exact substituted implementation witness.
    pub const fn instance(&self) -> ImplementationInstanceId {
        self.instance
    }
}

/// Reports candidate sets that cannot represent an implementation ambiguity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImplementationAmbiguityError {
    /// Fewer than two distinct semantic implementation keys remain.
    TooFewCandidates,
    /// One stable key was paired with conflicting implementation instances.
    ConflictingCandidateKey,
}

/// At least two distinct equally applicable implementation witnesses.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationAmbiguity {
    candidates: Arc<[ImplementationCandidate]>,
}

impl ImplementationAmbiguity {
    fn try_new(
        candidates: impl IntoIterator<Item = ImplementationCandidate>,
    ) -> Result<Self, ImplementationAmbiguityError> {
        let mut canonical = BTreeMap::new();

        for candidate in candidates {
            match canonical.entry(candidate.key) {
                Entry::Vacant(entry) => {
                    entry.insert(candidate.instance);
                }
                Entry::Occupied(entry) if *entry.get() == candidate.instance => {}
                Entry::Occupied(_) => {
                    return Err(ImplementationAmbiguityError::ConflictingCandidateKey);
                }
            }
        }

        if canonical.len() < 2 {
            return Err(ImplementationAmbiguityError::TooFewCandidates);
        }

        let candidates = canonical
            .into_iter()
            .map(|(key, instance)| ImplementationCandidate::new(key, instance));

        Ok(Self {
            candidates: shared_slice(candidates),
        })
    }

    /// Returns distinct candidates in stable semantic-key order.
    pub fn candidates(&self) -> &[ImplementationCandidate] {
        &self.candidates
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        GenericOwnerId, GenericSubstitutionData, ImplementationInstanceData,
        InherentImplementationSymbolId, PackageIdentity, SemanticValueStore, SymbolId, SymbolKey,
        SymbolKind,
    };
    use bray_declarations::DeclarationId;

    use super::{
        ImplementationAmbiguity, ImplementationAmbiguityError, ImplementationCandidate,
        ImplementationSelection, ImplementationSelectionKey, ImplementationSubject,
    };

    #[test]
    fn implementation_fact_values_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ImplementationSubject>();
        assert_send_sync::<ImplementationSelectionKey>();
        assert_send_sync::<ImplementationSelection>();
        assert_send_sync::<ImplementationAmbiguity>();
        assert_send_sync::<ImplementationCandidate>();
        assert_send_sync::<ImplementationAmbiguityError>();
    }

    #[test]
    fn ambiguous_implementation_candidates_are_normalized() {
        let Ok(store) = SemanticValueStore::try_new() else {
            panic!("semantic value store identity must be available");
        };

        let first_definition = InherentImplementationSymbolId::from_symbol_id(SymbolId::new(1));
        let second_definition = InherentImplementationSymbolId::from_symbol_id(SymbolId::new(2));

        let Some(first_owner) = GenericOwnerId::try_new(first_definition.into()) else {
            panic!("inherent implementation must be a generic owner");
        };

        let Some(second_owner) = GenericOwnerId::try_new(second_definition.into()) else {
            panic!("inherent implementation must be a generic owner");
        };

        let Ok(first_substitution) = GenericSubstitutionData::try_new(first_owner, [], []) else {
            panic!("empty implementation substitution must be valid");
        };

        let Ok(second_substitution) = GenericSubstitutionData::try_new(second_owner, [], []) else {
            panic!("empty implementation substitution must be valid");
        };

        let Ok(first_substitution) = store.intern_generic_substitution(first_substitution) else {
            panic!("first substitution must be internable");
        };

        let Ok(second_substitution) = store.intern_generic_substitution(second_substitution) else {
            panic!("second substitution must be internable");
        };

        let Ok(first) = store.intern_implementation_instance(ImplementationInstanceData::new(
            first_definition.into(),
            first_substitution,
        )) else {
            panic!("first implementation instance must be valid");
        };

        let Ok(second) = store.intern_implementation_instance(ImplementationInstanceData::new(
            second_definition.into(),
            second_substitution,
        )) else {
            panic!("second implementation instance must be valid");
        };

        let Ok(selection) = ImplementationSelection::ambiguous([
            ImplementationCandidate::new(implementation_key(2), second),
            ImplementationCandidate::new(implementation_key(1), first),
            ImplementationCandidate::new(implementation_key(2), second),
        ]) else {
            panic!("two distinct candidates must form an ambiguity");
        };

        let candidates = selection.ambiguous_candidates();

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].key(), &implementation_key(1));
        assert_eq!(candidates[0].instance(), first);
        assert_eq!(candidates[1].key(), &implementation_key(2));
        assert_eq!(candidates[1].instance(), second);

        assert_eq!(
            ImplementationSelection::ambiguous([ImplementationCandidate::new(
                implementation_key(1),
                first,
            )]),
            Err(ImplementationAmbiguityError::TooFewCandidates)
        );

        assert_eq!(
            ImplementationSelection::ambiguous([
                ImplementationCandidate::new(implementation_key(1), first),
                ImplementationCandidate::new(implementation_key(1), second),
            ]),
            Err(ImplementationAmbiguityError::ConflictingCandidateKey)
        );
    }

    fn implementation_key(declaration: u32) -> SymbolKey {
        let Some(package) = PackageIdentity::try_new("example.package") else {
            panic!("package identity must be valid");
        };

        let owner = SymbolKey::package(package);

        let Some(key) = SymbolKey::source_declaration(
            owner,
            SymbolKind::InherentImplementation,
            DeclarationId::new(declaration),
        ) else {
            panic!("implementation kind must support source declaration keys");
        };

        key
    }
}
