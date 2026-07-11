use std::{collections::BTreeSet, sync::Arc};

use bray_base::shared_slice;

use crate::{ImplementationInstanceId, TraitApplicationId, TypeId};

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
    /// Creates an ambiguous result when at least two distinct candidates remain.
    pub fn ambiguous(
        candidates: impl IntoIterator<Item = ImplementationInstanceId>,
    ) -> Option<Self> {
        ImplementationAmbiguity::try_new(candidates).map(Self::Ambiguous)
    }

    /// Returns ambiguous candidates or an empty slice for another selection state.
    pub fn ambiguous_candidates(&self) -> &[ImplementationInstanceId] {
        match self {
            Self::Ambiguous(ambiguity) => ambiguity.candidates(),
            Self::Selected(_) | Self::Unavailable => &[],
        }
    }
}

/// At least two distinct equally applicable implementation witnesses.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationAmbiguity {
    candidates: Arc<[ImplementationInstanceId]>,
}

impl ImplementationAmbiguity {
    fn try_new(candidates: impl IntoIterator<Item = ImplementationInstanceId>) -> Option<Self> {
        let mut seen = BTreeSet::new();
        let candidates: Vec<_> = candidates
            .into_iter()
            .filter(|candidate| seen.insert(*candidate))
            .collect();

        if candidates.len() < 2 {
            return None;
        }

        Some(Self {
            candidates: shared_slice(candidates),
        })
    }

    /// Returns distinct candidates in the selector's canonical order.
    pub fn candidates(&self) -> &[ImplementationInstanceId] {
        &self.candidates
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        GenericOwnerId, GenericSubstitutionData, ImplementationInstanceData,
        InherentImplementationSymbolId, SemanticValueStore, SymbolId,
    };

    use super::{
        ImplementationAmbiguity, ImplementationSelection, ImplementationSelectionKey,
        ImplementationSubject,
    };

    #[test]
    fn implementation_fact_values_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ImplementationSubject>();
        assert_send_sync::<ImplementationSelectionKey>();
        assert_send_sync::<ImplementationSelection>();
        assert_send_sync::<ImplementationAmbiguity>();
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

        let Some(selection) = ImplementationSelection::ambiguous([second, first, second]) else {
            panic!("two distinct candidates must form an ambiguity");
        };

        assert_eq!(selection.ambiguous_candidates(), &[second, first]);

        assert!(ImplementationSelection::ambiguous([first]).is_none());
    }
}
