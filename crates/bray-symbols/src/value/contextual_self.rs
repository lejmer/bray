use super::{
    GenericSubstitutionId, SelfTypeContext, SemanticValueStore, SemanticValueStoreError,
    TraitApplicationData, TraitApplicationId, TypeData, TypeId,
};

impl crate::SymbolGraph {
    /// Returns the nearest containing declaration that defines contextual `Self`.
    pub fn enclosing_self_type_context(
        &self,
        symbol: crate::AnySymbolId,
    ) -> Option<SelfTypeContext> {
        let mut current = self.containing_symbol(symbol);

        while let Some(owner) = current {
            if let Some(context) = SelfTypeContext::try_new(owner) {
                return Some(context);
            }

            current = self.containing_symbol(owner);
        }

        None
    }
}

impl SemanticValueStore {
    /// Replaces one declaration context's `Self` throughout a semantic type.
    pub fn substitute_contextual_self(
        &self,
        ty: TypeId,
        context: SelfTypeContext,
        replacement: TypeId,
    ) -> Result<TypeId, SemanticValueStoreError> {
        let contextual = self.intern_type(TypeData::ContextualSelf(context))?;
        let types = std::collections::BTreeMap::from([(contextual, replacement)]);

        self.substitute_type_data(ty, &super::substitution_apply::Substitution::Types(&types))
    }

    /// Replaces one declaration context's `Self` throughout a trait application.
    pub fn substitute_contextual_self_in_application(
        &self,
        application: TraitApplicationId,
        context: SelfTypeContext,
        replacement: TypeId,
    ) -> Result<TraitApplicationId, SemanticValueStoreError> {
        let data = self.trait_application_data(application);

        let substitution = self.substitute_contextual_self_in_substitution(
            data.substitution(),
            context,
            replacement,
        )?;

        self.intern_trait_application(TraitApplicationData::new(data.definition(), substitution))
    }

    /// Replaces one declaration context's `Self` throughout a generic substitution.
    pub fn substitute_contextual_self_in_substitution(
        &self,
        substitution: GenericSubstitutionId,
        context: SelfTypeContext,
        replacement: TypeId,
    ) -> Result<GenericSubstitutionId, SemanticValueStoreError> {
        let contextual = self.intern_type(TypeData::ContextualSelf(context))?;
        let types = std::collections::BTreeMap::from([(contextual, replacement)]);

        self.substitute_generic_substitution_data(
            substitution,
            &super::substitution_apply::Substitution::Types(&types),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::SemanticValueStore;
    use crate::{
        SelfTypeContext, SemanticValueStoreError, StructSymbolId, SymbolId, TraitSymbolId, TypeData,
    };

    #[test]
    fn contextual_self_substitution_reaches_nested_type_layers() {
        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store creation failed: {error:?}"));

        let context = SelfTypeContext::Trait(TraitSymbolId::from_symbol_id(SymbolId::new(1)));

        let replacement = store
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("replacement type interning failed: {error:?}"));

        let contextual = store
            .intern_type(TypeData::ContextualSelf(context))
            .unwrap_or_else(|error| panic!("contextual type interning failed: {error:?}"));

        let subject = store
            .intern_type(TypeData::Nullable(contextual))
            .unwrap_or_else(|error| panic!("subject type interning failed: {error:?}"));

        let substituted = store
            .substitute_contextual_self(subject, context, replacement)
            .unwrap_or_else(|error| panic!("contextual substitution failed: {error:?}"));

        let data = store.type_data(substituted);

        assert_eq!(data.as_ref(), &TypeData::Nullable(replacement));
    }

    #[test]
    fn contextual_self_substitution_preserves_other_contexts() {
        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store creation failed: {error:?}"));

        let source = SelfTypeContext::Trait(TraitSymbolId::from_symbol_id(SymbolId::new(1)));

        let other = SelfTypeContext::NamedType(crate::NamedTypeSymbolId::Struct(
            StructSymbolId::from_symbol_id(SymbolId::new(2)),
        ));

        let subject = store
            .intern_type(TypeData::ContextualSelf(other))
            .unwrap_or_else(|error| panic!("subject type interning failed: {error:?}"));

        let replacement = store
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("replacement type interning failed: {error:?}"));

        let result: Result<_, SemanticValueStoreError> =
            store.substitute_contextual_self(subject, source, replacement);

        assert_eq!(result, Ok(subject));
    }
    #[test]
    fn contextual_cleanup_substitution_reaches_the_complete_dependency_contract() {
        use crate::{
            DependencyContractTemplateData, DependencyMutationKind, DependencyStorageEffect,
            DependencySubject, DependencySubjectRoot,
        };

        let store =
            SemanticValueStore::try_new().expect("fixture semantic value store must be valid");

        let context = SelfTypeContext::Trait(TraitSymbolId::from_symbol_id(SymbolId::new(1)));

        let contextual = store
            .intern_type(TypeData::ContextualSelf(context))
            .expect("fixture type must be interned");

        let replacement = store
            .intern_type(TypeData::tuple([]))
            .expect("fixture type must be interned");

        let nested = store
            .intern_type(TypeData::Nullable(contextual))
            .expect("fixture type must be interned");

        let template = store
            .intern_dependency_contract_template(
                DependencyContractTemplateData::new([])
                    .with_synchronous_cleanup([crate::DependencyRequirement::SynchronousCleanup(
                        nested,
                    )])
                    .with_effects([
                        DependencyStorageEffect::Cleanup(contextual),
                        DependencyStorageEffect::Direct {
                            subject: DependencySubject::root(DependencySubjectRoot::Receiver),
                            kind: DependencyMutationKind::Replace(contextual),
                        },
                    ]),
            )
            .expect("fixture dependency contract must be interned");

        let instantiated = store
            .substitute_contextual_self_in_dependency_contract(template, context, replacement)
            .expect(
                "fixture `substitute_contextual_self_in_dependency_contract` call must succeed",
            );

        let data = store.dependency_contract_template_data(instantiated);

        let expected = store
            .intern_type(TypeData::Nullable(replacement))
            .expect("fixture type must be interned");

        assert_eq!(
            data.synchronous_cleanup(),
            [crate::DependencyRequirement::SynchronousCleanup(expected)]
        );

        assert!(
            data.effects()
                .contains(&DependencyStorageEffect::Cleanup(replacement))
        );

        assert!(data.effects().iter().any(|effect| matches!(effect,
            DependencyStorageEffect::Direct { kind: DependencyMutationKind::Replace(ty), .. }
                if *ty == replacement)));
    }
}
