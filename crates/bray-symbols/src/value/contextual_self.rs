use super::substitution_apply::SemanticSubstitution;
use super::{
    ConstantTermId, GenericSubstitutionId, SelfTypeContext, SemanticValueStore,
    SemanticValueStoreError, TraitApplicationData, TraitApplicationId, TypeData, TypeId,
};

struct ContextualSelfSubstitution {
    context: SelfTypeContext,
    replacement: TypeId,
}

impl SemanticSubstitution for ContextualSelfSubstitution {
    fn replacement_type(
        &self,
        _values: &SemanticValueStore,
        data: &TypeData,
    ) -> Result<Option<TypeId>, SemanticValueStoreError> {
        Ok(
            matches!(data, TypeData::ContextualSelf(context) if *context == self.context)
                .then_some(self.replacement),
        )
    }
}

impl SemanticValueStore {
    /// Replaces one declaration context's `Self` throughout a semantic type and its dependencies.
    pub fn substitute_contextual_self(
        &self,
        ty: TypeId,
        context: SelfTypeContext,
        replacement: TypeId,
    ) -> Result<TypeId, SemanticValueStoreError> {
        self.substitute_type_data(
            ty,
            &ContextualSelfSubstitution {
                context,
                replacement,
            },
        )
    }

    /// Replaces one declaration context's `Self` throughout a checked constant term.
    pub fn substitute_contextual_self_in_constant_term(
        &self,
        term: ConstantTermId,
        context: SelfTypeContext,
        replacement: TypeId,
    ) -> Result<ConstantTermId, SemanticValueStoreError> {
        self.substitute_constant_term_data(
            term,
            &ContextualSelfSubstitution {
                context,
                replacement,
            },
        )
    }

    /// Replaces one declaration context's `Self` throughout a trait application.
    pub fn substitute_contextual_self_in_application(
        &self,
        application: TraitApplicationId,
        context: SelfTypeContext,
        replacement: TypeId,
    ) -> Result<TraitApplicationId, SemanticValueStoreError> {
        let data = self.trait_application_data(application)?;

        let substitution = self.substitute_contextual_self_in_substitution(
            data.substitution(),
            context,
            replacement,
        )?;

        self.intern_trait_application(TraitApplicationData::new(data.definition(), substitution))
    }

    /// Replaces one declaration context's `Self` throughout generic type and constant arguments.
    pub fn substitute_contextual_self_in_substitution(
        &self,
        substitution: GenericSubstitutionId,
        context: SelfTypeContext,
        replacement: TypeId,
    ) -> Result<GenericSubstitutionId, SemanticValueStoreError> {
        self.substitute_generic_substitution_data(
            substitution,
            &ContextualSelfSubstitution {
                context,
                replacement,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::SemanticValueStore;
    use crate::{
        ConstantTermData, FunctionSymbolId, GenericArgument, GenericConstParameterSymbolId,
        GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData, SelfTypeContext,
        SemanticValueStoreError, StructSymbolId, SymbolId, SymbolOrdinal, TraitSymbolId, TypeData,
    };
    use std::sync::Arc;

    #[test]
    fn contextual_self_substitution_reaches_constant_arguments_and_array_lengths() {
        let store = SemanticValueStore::try_new().expect("semantic store");
        let context = SelfTypeContext::Trait(TraitSymbolId::from_symbol_id(SymbolId::new(1)));

        let contextual = store
            .intern_type(TypeData::ContextualSelf(context))
            .expect("Self type");

        let replacement = store
            .intern_type(TypeData::Tuple(Arc::from([])))
            .expect("replacement");

        let argument = store
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .expect("argument");

        let original = store
            .intern_constant_term(ConstantTermData::Typed {
                term: argument,
                ty: contextual,
            })
            .expect("typed argument");

        let expected = store
            .intern_constant_term(ConstantTermData::Typed {
                term: argument,
                ty: replacement,
            })
            .expect("replacement argument");

        assert_eq!(
            store.substitute_contextual_self_in_constant_term(original, context, replacement),
            Ok(expected)
        );

        let array = store
            .intern_type(TypeData::Array {
                element: contextual,
                length: original,
            })
            .expect("array");

        let expected_array = store
            .intern_type(TypeData::Array {
                element: replacement,
                length: expected,
            })
            .expect("replacement array");

        assert_eq!(
            store.substitute_contextual_self(array, context, replacement),
            Ok(expected_array)
        );

        let owner =
            GenericOwnerId::try_new(FunctionSymbolId::from_symbol_id(SymbolId::new(2)).into())
                .expect("generic owner");

        let parameter = GenericParameterSymbolId::Const(
            GenericConstParameterSymbolId::from_symbol_id(SymbolId::new(3)),
        );

        let original = store
            .intern_generic_substitution(
                GenericSubstitutionData::try_new(
                    owner,
                    [parameter],
                    [GenericArgument::Constant(original)],
                )
                .expect("binding"),
            )
            .expect("substitution");

        let expected = store
            .intern_generic_substitution(
                GenericSubstitutionData::try_new(
                    owner,
                    [parameter],
                    [GenericArgument::Constant(expected)],
                )
                .expect("binding"),
            )
            .expect("substitution");

        assert_eq!(
            store.substitute_contextual_self_in_substitution(original, context, replacement),
            Ok(expected)
        );
    }

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

        let data = store
            .type_data(substituted)
            .unwrap_or_else(|error| panic!("substituted type missing: {error:?}"));

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
}
