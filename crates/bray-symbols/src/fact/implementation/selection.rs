use std::{collections::BTreeMap, collections::btree_map::Entry, sync::Arc};

use bray_base::shared_slice;

use crate::{ImplementationInstanceId, SymbolKey};

/// The deterministic result of selecting an implementation witness.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ImplementationSelection {
    /// One exact implementation witness was selected.
    Selected(ImplementationInstanceId),
    /// Applicability cannot yet be decided from the available semantic facts.
    Deferred,
    /// No applicable implementation exists.
    Unavailable,
    /// Several equally applicable implementation witnesses remain.
    Ambiguous(ImplementationAmbiguity),
}

impl ImplementationSelection {
    /// Creates an ambiguity normalized by stable semantic symbol key.
    pub fn ambiguous(
        candidates: impl IntoIterator<Item = ImplementationSelectionCandidate>,
    ) -> Result<Self, ImplementationAmbiguityError> {
        ImplementationAmbiguity::try_new(candidates).map(Self::Ambiguous)
    }

    /// Returns ambiguous candidates or an empty slice for another selection state.
    pub fn ambiguous_candidates(&self) -> &[ImplementationSelectionCandidate] {
        match self {
            Self::Ambiguous(ambiguity) => ambiguity.candidates(),
            Self::Selected(_) | Self::Deferred | Self::Unavailable => &[],
        }
    }
}

/// One applicable implementation witness and its stable semantic definition key.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationSelectionCandidate {
    key: SymbolKey,
    instance: ImplementationInstanceId,
}

impl ImplementationSelectionCandidate {
    /// Creates one applicable implementation witness candidate.
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
    candidates: Arc<[ImplementationSelectionCandidate]>,
}

impl ImplementationAmbiguity {
    fn try_new(
        candidates: impl IntoIterator<Item = ImplementationSelectionCandidate>,
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
            .map(|(key, instance)| ImplementationSelectionCandidate::new(key, instance));

        Ok(Self {
            candidates: shared_slice(candidates),
        })
    }

    /// Returns distinct candidates in stable semantic-key order.
    pub fn candidates(&self) -> &[ImplementationSelectionCandidate] {
        &self.candidates
    }
}

#[cfg(test)]
mod tests {
    use bray_declarations::DeclarationId;

    use crate::{
        GenericOwnerId, GenericSubstitutionData, ImplementationInstanceData,
        InherentImplementationSymbolId, PackageIdentity, SemanticValueStore, SymbolId, SymbolKey,
        SymbolKind,
    };

    use super::{
        ImplementationAmbiguity, ImplementationAmbiguityError, ImplementationSelection,
        ImplementationSelectionCandidate,
    };

    #[test]
    fn implementation_selection_contracts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ImplementationSelection>();
        assert_send_sync::<ImplementationAmbiguity>();
        assert_send_sync::<ImplementationSelectionCandidate>();
        assert_send_sync::<ImplementationAmbiguityError>();
    }

    #[test]
    fn ambiguous_implementation_witnesses_are_normalized() {
        let Ok(store) = SemanticValueStore::try_new() else {
            panic!("semantic value store identity must be available");
        };

        let first_definition = InherentImplementationSymbolId::from_symbol_id(SymbolId::new(1));
        let second_definition = InherentImplementationSymbolId::from_symbol_id(SymbolId::new(2));
        let first = implementation_instance(&store, first_definition);
        let second = implementation_instance(&store, second_definition);

        let Ok(selection) = ImplementationSelection::ambiguous([
            ImplementationSelectionCandidate::new(implementation_key(2), second),
            ImplementationSelectionCandidate::new(implementation_key(1), first),
            ImplementationSelectionCandidate::new(implementation_key(2), second),
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
            ImplementationSelection::ambiguous([ImplementationSelectionCandidate::new(
                implementation_key(1),
                first,
            )]),
            Err(ImplementationAmbiguityError::TooFewCandidates)
        );

        assert_eq!(
            ImplementationSelection::ambiguous([
                ImplementationSelectionCandidate::new(implementation_key(1), first),
                ImplementationSelectionCandidate::new(implementation_key(1), second),
            ]),
            Err(ImplementationAmbiguityError::ConflictingCandidateKey)
        );
    }

    fn implementation_instance(
        store: &SemanticValueStore,
        definition: InherentImplementationSymbolId,
    ) -> crate::ImplementationInstanceId {
        let Some(owner) = GenericOwnerId::try_new(definition.into()) else {
            panic!("inherent implementation must be a generic owner");
        };

        let Ok(substitution) = GenericSubstitutionData::try_new(owner, [], []) else {
            panic!("empty implementation substitution must be valid");
        };

        let Ok(substitution) = store.intern_generic_substitution(substitution) else {
            panic!("implementation substitution must be internable");
        };

        let Ok(instance) = store.intern_implementation_instance(ImplementationInstanceData::new(
            definition.into(),
            substitution,
        )) else {
            panic!("implementation instance must be valid");
        };

        instance
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
