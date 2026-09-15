use std::collections::{BTreeMap, BTreeSet};

use bray_symbols::{
    CallableTypeData, GenericArgument, GenericParameterSymbolId, GenericSubstitutionData,
    GenericSubstitutionId, GenericSubstitutionShapeError, ImplementationSymbolId,
    SemanticValueStore, SemanticValueStoreError, TraitApplicationId, TypeData, TypeId,
};

use super::super::index::ImplementationHeader;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ImplementationMatchError {
    InvalidSubstitution(GenericSubstitutionShapeError),
    SemanticValue(SemanticValueStoreError),
}

impl From<SemanticValueStoreError> for ImplementationMatchError {
    fn from(error: SemanticValueStoreError) -> Self {
        Self::SemanticValue(error)
    }
}

pub(in crate::compilation::implementation) fn match_implementation_header(
    header: &ImplementationHeader,
    subject: TypeId,
    trait_application: TraitApplicationId,
    values: &SemanticValueStore,
) -> Result<Option<GenericSubstitutionId>, ImplementationMatchError> {
    let mut matcher = HeaderMatcher::new(header.parameters(), values);

    if !matcher.match_type(header.subject(), subject)?
        || !matcher.match_trait_application(header.trait_application(), trait_application)?
    {
        return Ok(None);
    }

    matcher.finish(header.implementation(), header.parameters())
}

pub(in crate::compilation) fn match_implementation_subject(
    implementation: ImplementationSymbolId,
    parameters: &[GenericParameterSymbolId],
    pattern: TypeId,
    actual: TypeId,
    values: &SemanticValueStore,
) -> Result<Option<GenericSubstitutionId>, ImplementationMatchError> {
    let mut matcher = HeaderMatcher::new(parameters, values);

    if !matcher.match_type(pattern, actual)? {
        return Ok(None);
    }

    matcher.finish(implementation, parameters)
}

pub(super) struct HeaderMatcher<'values> {
    pub(super) parameters: BTreeSet<GenericParameterSymbolId>,
    pub(super) arguments: BTreeMap<GenericParameterSymbolId, GenericArgument>,
    pub(super) values: &'values SemanticValueStore,
}

impl<'values> HeaderMatcher<'values> {
    pub(super) fn new(
        parameters: &[GenericParameterSymbolId],
        values: &'values SemanticValueStore,
    ) -> Self {
        Self {
            parameters: parameters.iter().copied().collect(),
            arguments: BTreeMap::new(),
            values,
        }
    }

    fn finish(
        mut self,
        implementation: ImplementationSymbolId,
        parameters: &[GenericParameterSymbolId],
    ) -> Result<Option<GenericSubstitutionId>, ImplementationMatchError> {
        for parameter in parameters {
            if self.arguments.contains_key(parameter) {
                continue;
            }

            let argument = self
                .values
                .intern_generic_parameter_argument(*parameter)
                .map_err(ImplementationMatchError::SemanticValue)?;

            self.arguments.insert(*parameter, argument);
        }

        let Some(arguments) = parameters
            .iter()
            .map(|parameter| self.arguments.get(parameter).copied())
            .collect::<Option<Vec<_>>>()
        else {
            return Ok(None);
        };

        let Some(owner) = bray_symbols::GenericOwnerId::try_new(implementation.into_any()) else {
            return Ok(None);
        };

        let substitution =
            GenericSubstitutionData::try_new(owner, parameters.iter().copied(), arguments)
                .map_err(ImplementationMatchError::InvalidSubstitution)?;

        self.values
            .intern_generic_substitution(substitution)
            .map(Some)
            .map_err(ImplementationMatchError::SemanticValue)
    }

    pub(super) fn match_type(
        &mut self,
        pattern: TypeId,
        actual: TypeId,
    ) -> Result<bool, SemanticValueStoreError> {
        let pattern_id = pattern;
        let actual_id = actual;
        let pattern = self.values.type_data(pattern);

        if let TypeData::TypeParameter(parameter) = pattern.as_ref() {
            let parameter = GenericParameterSymbolId::Type(*parameter);

            if self.parameters.contains(&parameter) {
                return Ok(self.bind(parameter, GenericArgument::Type(actual_id)));
            }
        }

        if pattern_id == actual {
            return Ok(true);
        }

        let actual = self.values.type_data(actual);

        self.match_type_data(pattern.as_ref(), actual.as_ref())
    }

    fn match_type_data(
        &mut self,
        pattern: &TypeData,
        actual: &TypeData,
    ) -> Result<bool, SemanticValueStoreError> {
        match (pattern, actual) {
            (
                TypeData::Named {
                    definition: pattern_definition,
                    substitution: pattern_substitution,
                },
                TypeData::Named {
                    definition: actual_definition,
                    substitution: actual_substitution,
                },
            ) => {
                if pattern_definition != actual_definition {
                    return Ok(false);
                }

                self.match_substitution(*pattern_substitution, *actual_substitution)
            }
            (
                TypeData::TypeValuedMemberProjection {
                    subject: pattern_subject,
                    application: pattern_application,
                    member: pattern_member,
                },
                TypeData::TypeValuedMemberProjection {
                    subject: actual_subject,
                    application: actual_application,
                    member: actual_member,
                },
            ) => {
                if pattern_member != actual_member
                    || !self.match_type(*pattern_subject, *actual_subject)?
                {
                    return Ok(false);
                }

                self.match_trait_application(*pattern_application, *actual_application)
            }
            (TypeData::Tuple(pattern), TypeData::Tuple(actual)) => {
                self.match_types(pattern, actual)
            }
            (
                TypeData::Array {
                    element: pattern_element,
                    length: pattern_length,
                },
                TypeData::Array {
                    element: actual_element,
                    length: actual_length,
                },
            ) => {
                if !self.match_type(*pattern_element, *actual_element)? {
                    return Ok(false);
                }

                self.match_constant(*pattern_length, *actual_length)
            }
            (TypeData::Slice(pattern), TypeData::Slice(actual))
            | (TypeData::Nullable(pattern), TypeData::Nullable(actual)) => {
                self.match_type(*pattern, *actual)
            }
            (
                TypeData::Borrow {
                    kind: pattern_kind,
                    target: pattern_target,
                },
                TypeData::Borrow {
                    kind: actual_kind,
                    target: actual_target,
                },
            ) => {
                if pattern_kind != actual_kind {
                    return Ok(false);
                }

                self.match_type(*pattern_target, *actual_target)
            }
            (TypeData::TraitView(pattern), TypeData::TraitView(actual)) => {
                self.match_trait_application(*pattern, *actual)
            }
            (
                TypeData::OwnedIndirection {
                    storage: pattern_storage,
                    target: pattern_target,
                },
                TypeData::OwnedIndirection {
                    storage: actual_storage,
                    target: actual_target,
                },
            ) => {
                if !self.match_type(*pattern_storage, *actual_storage)? {
                    return Ok(false);
                }

                self.match_type(*pattern_target, *actual_target)
            }
            (TypeData::Callable(pattern), TypeData::Callable(actual)) => {
                self.match_callable_type(pattern, actual)
            }
            _ => Ok(false),
        }
    }

    fn match_callable_type(
        &mut self,
        pattern: &CallableTypeData,
        actual: &CallableTypeData,
    ) -> Result<bool, SemanticValueStoreError> {
        if pattern.constness() != actual.constness()
            || pattern.execution() != actual.execution()
            || pattern.trust() != actual.trust()
            || pattern.abi() != actual.abi()
            || pattern.parameters().len() != actual.parameters().len()
        {
            return Ok(false);
        }

        if !self.match_dependency_contracts(
            pattern.dependency_contracts(),
            actual.dependency_contracts(),
        )? {
            return Ok(false);
        }

        for (pattern, actual) in pattern.parameters().iter().zip(actual.parameters()) {
            if pattern.name() != actual.name()
                || pattern.position() != actual.position()
                || pattern.mode() != actual.mode()
                || !self.match_type(pattern.ty(), actual.ty())?
            {
                return Ok(false);
            }
        }

        self.match_type(pattern.result(), actual.result())
    }

    fn match_types(
        &mut self,
        pattern: &[TypeId],
        actual: &[TypeId],
    ) -> Result<bool, SemanticValueStoreError> {
        if pattern.len() != actual.len() {
            return Ok(false);
        }

        for (pattern, actual) in pattern.iter().zip(actual) {
            if !self.match_type(*pattern, *actual)? {
                return Ok(false);
            }
        }

        Ok(true)
    }

    pub(super) fn match_trait_application(
        &mut self,
        pattern: TraitApplicationId,
        actual: TraitApplicationId,
    ) -> Result<bool, SemanticValueStoreError> {
        if pattern == actual {
            return Ok(true);
        }

        let pattern = self.values.trait_application_data(pattern);
        let actual = self.values.trait_application_data(actual);

        if pattern.definition() != actual.definition() {
            return Ok(false);
        }

        self.match_substitution(pattern.substitution(), actual.substitution())
    }

    pub(super) fn match_substitution(
        &mut self,
        pattern: GenericSubstitutionId,
        actual: GenericSubstitutionId,
    ) -> Result<bool, SemanticValueStoreError> {
        if pattern == actual {
            return Ok(true);
        }

        let pattern = self.values.generic_substitution_data(pattern);
        let actual = self.values.generic_substitution_data(actual);

        if pattern.owner() != actual.owner() || pattern.bindings().len() != actual.bindings().len()
        {
            return Ok(false);
        }

        for (pattern, actual) in pattern.bindings().iter().zip(actual.bindings()) {
            if pattern.parameter() != actual.parameter()
                || !self.match_argument(pattern.argument(), actual.argument())?
            {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn match_argument(
        &mut self,
        pattern: GenericArgument,
        actual: GenericArgument,
    ) -> Result<bool, SemanticValueStoreError> {
        match (pattern, actual) {
            (GenericArgument::Type(pattern), GenericArgument::Type(actual)) => {
                self.match_type(pattern, actual)
            }
            (GenericArgument::Constant(pattern), GenericArgument::Constant(actual)) => {
                self.match_constant(pattern, actual)
            }
            _ => Ok(false),
        }
    }

    pub(super) fn bind(
        &mut self,
        parameter: GenericParameterSymbolId,
        argument: GenericArgument,
    ) -> bool {
        match self.arguments.get(&parameter) {
            Some(previous) => *previous == argument,
            None => {
                self.arguments.insert(parameter, argument);

                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::{
        ConstantTermData, GenericArgument, GenericConstParameterSymbolId, GenericParameterSymbolId,
        GenericTypeParameterSymbolId, ImplementationSymbolId, InherentImplementationSymbolId,
        SemanticValueStore, SymbolId, TypeData,
    };

    use super::{HeaderMatcher, match_implementation_subject};

    #[test]
    fn equal_generic_parameters_are_recorded_as_substitution_arguments() {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic value store must build: {error:?}"));

        let type_parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(1));
        let type_parameter_id = GenericParameterSymbolId::Type(type_parameter);

        let ty = values
            .intern_type(TypeData::TypeParameter(type_parameter))
            .unwrap_or_else(|error| panic!("parameter type must be valid: {error:?}"));

        let mut type_matcher = HeaderMatcher::new(&[type_parameter_id], &values);

        assert_eq!(type_matcher.match_type(ty, ty), Ok(true));

        assert_eq!(
            type_matcher.arguments.get(&type_parameter_id),
            Some(&GenericArgument::Type(ty))
        );

        let const_parameter = GenericConstParameterSymbolId::from_symbol_id(SymbolId::new(2));
        let const_parameter_id = GenericParameterSymbolId::Const(const_parameter);

        let term = values
            .intern_constant_term(ConstantTermData::Parameter(const_parameter))
            .unwrap_or_else(|error| panic!("parameter term must be valid: {error:?}"));

        let mut const_matcher = HeaderMatcher::new(&[const_parameter_id], &values);

        assert_eq!(const_matcher.match_constant(term, term), Ok(true));

        assert_eq!(
            const_matcher.arguments.get(&const_parameter_id),
            Some(&GenericArgument::Constant(term))
        );
    }

    #[test]
    fn equal_open_subjects_produce_identity_substitutions() {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic value store must build: {error:?}"));

        let implementation = ImplementationSymbolId::Inherent(
            InherentImplementationSymbolId::from_symbol_id(SymbolId::new(1)),
        );

        let parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(2));
        let parameter = GenericParameterSymbolId::Type(parameter);

        let argument = values
            .intern_generic_parameter_argument(parameter)
            .unwrap_or_else(|error| panic!("parameter argument must be valid: {error:?}"));

        let matcher = HeaderMatcher::new(&[parameter], &values);

        assert_eq!(matcher.arguments.get(&parameter), None);

        let substitution = matcher
            .finish(implementation, &[parameter])
            .unwrap_or_else(|error| panic!("identity substitution must build: {error:?}"))
            .unwrap_or_else(|| panic!("identity substitution must be present"));

        let substitution = values.generic_substitution_data(substitution);

        assert_eq!(substitution.bindings()[0].argument(), argument);
    }

    #[test]
    fn implementation_matching_retains_foreign_semantic_value_ids() {
        let first = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("first semantic store must build: {error:?}"));

        let second = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("second semantic store must build: {error:?}"));

        let pattern = first
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("pattern type must build: {error:?}"));

        let implementation = ImplementationSymbolId::Inherent(
            InherentImplementationSymbolId::from_symbol_id(SymbolId::new(1)),
        );

        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                match_implementation_subject(implementation, &[], pattern, pattern, &second)
            }))
            .is_err()
        );
    }
}
