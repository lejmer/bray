use std::collections::{BTreeMap, BTreeSet};

use bray_symbols::{
    CallableTypeData, ConstantTermData, ConstantTermId, GenericArgument, GenericParameterSymbolId,
    GenericSubstitutionData, GenericSubstitutionId, SemanticValueStore, SemanticValueStoreError,
    TraitApplicationId, TypeData, TypeId,
};

use super::index::ImplementationHeader;

#[derive(Debug)]
pub(super) enum ImplementationMatchError {
    InvalidSubstitution,
    SemanticValue,
}

impl From<SemanticValueStoreError> for ImplementationMatchError {
    fn from(_error: SemanticValueStoreError) -> Self {
        Self::SemanticValue
    }
}

pub(super) fn match_implementation_header(
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

    matcher.finish(header)
}

struct HeaderMatcher<'values> {
    parameters: BTreeSet<GenericParameterSymbolId>,
    arguments: BTreeMap<GenericParameterSymbolId, GenericArgument>,
    values: &'values SemanticValueStore,
}

impl<'values> HeaderMatcher<'values> {
    fn new(parameters: &[GenericParameterSymbolId], values: &'values SemanticValueStore) -> Self {
        Self {
            parameters: parameters.iter().copied().collect(),
            arguments: BTreeMap::new(),
            values,
        }
    }

    fn finish(
        self,
        header: &ImplementationHeader,
    ) -> Result<Option<GenericSubstitutionId>, ImplementationMatchError> {
        let Some(arguments) = header
            .parameters()
            .iter()
            .map(|parameter| self.arguments.get(parameter).copied())
            .collect::<Option<Vec<_>>>()
        else {
            return Ok(None);
        };

        let Some(owner) = bray_symbols::GenericOwnerId::try_new(header.implementation().into_any())
        else {
            return Ok(None);
        };

        let substitution =
            GenericSubstitutionData::try_new(owner, header.parameters().iter().copied(), arguments)
                .map_err(|_| ImplementationMatchError::InvalidSubstitution)?;

        self.values
            .intern_generic_substitution(substitution)
            .map(Some)
            .map_err(|_| ImplementationMatchError::SemanticValue)
    }

    fn match_type(
        &mut self,
        pattern: TypeId,
        actual: TypeId,
    ) -> Result<bool, SemanticValueStoreError> {
        if pattern == actual {
            return Ok(true);
        }

        let actual_id = actual;
        let pattern = self.values.type_data(pattern)?;
        let actual = self.values.type_data(actual)?;

        if let TypeData::TypeParameter(parameter) = pattern.as_ref() {
            let parameter = GenericParameterSymbolId::Type(*parameter);

            if self.parameters.contains(&parameter) {
                return Ok(self.bind(parameter, GenericArgument::Type(actual_id)));
            }
        }

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
                TypeData::AssociatedTypeProjection {
                    subject: pattern_subject,
                    application: pattern_application,
                    member: pattern_member,
                },
                TypeData::AssociatedTypeProjection {
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
            || pattern.dependency_contracts() != actual.dependency_contracts()
            || pattern.parameters().len() != actual.parameters().len()
        {
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

    fn match_trait_application(
        &mut self,
        pattern: TraitApplicationId,
        actual: TraitApplicationId,
    ) -> Result<bool, SemanticValueStoreError> {
        if pattern == actual {
            return Ok(true);
        }

        let pattern = self.values.trait_application_data(pattern)?;
        let actual = self.values.trait_application_data(actual)?;

        if pattern.definition() != actual.definition() {
            return Ok(false);
        }

        self.match_substitution(pattern.substitution(), actual.substitution())
    }

    fn match_substitution(
        &mut self,
        pattern: GenericSubstitutionId,
        actual: GenericSubstitutionId,
    ) -> Result<bool, SemanticValueStoreError> {
        if pattern == actual {
            return Ok(true);
        }

        let pattern = self.values.generic_substitution_data(pattern)?;
        let actual = self.values.generic_substitution_data(actual)?;

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

    fn match_constant(
        &mut self,
        pattern: ConstantTermId,
        actual: ConstantTermId,
    ) -> Result<bool, SemanticValueStoreError> {
        if pattern == actual {
            return Ok(true);
        }

        let pattern_data = self.values.constant_term_data(pattern)?;

        if let ConstantTermData::Parameter(parameter) = pattern_data.as_ref() {
            let parameter = GenericParameterSymbolId::Const(*parameter);

            if self.parameters.contains(&parameter) {
                return Ok(self.bind(parameter, GenericArgument::Constant(actual)));
            }
        }

        let actual_data = self.values.constant_term_data(actual)?;

        match (pattern_data.as_ref(), actual_data.as_ref()) {
            (
                ConstantTermData::Unary {
                    operation: pattern_operation,
                    operand: pattern_operand,
                },
                ConstantTermData::Unary {
                    operation: actual_operation,
                    operand: actual_operand,
                },
            ) => {
                if pattern_operation != actual_operation {
                    return Ok(false);
                }

                self.match_constant(*pattern_operand, *actual_operand)
            }
            (
                ConstantTermData::Binary {
                    operation: pattern_operation,
                    left: pattern_left,
                    right: pattern_right,
                },
                ConstantTermData::Binary {
                    operation: actual_operation,
                    left: actual_left,
                    right: actual_right,
                },
            ) => {
                if pattern_operation != actual_operation
                    || !self.match_constant(*pattern_left, *actual_left)?
                {
                    return Ok(false);
                }

                self.match_constant(*pattern_right, *actual_right)
            }
            (
                ConstantTermData::DefinitionApplication {
                    definition: pattern_definition,
                    substitution: pattern_substitution,
                    selected_implementation: pattern_implementation,
                },
                ConstantTermData::DefinitionApplication {
                    definition: actual_definition,
                    substitution: actual_substitution,
                    selected_implementation: actual_implementation,
                },
            ) => {
                if pattern_definition != actual_definition
                    || pattern_implementation != actual_implementation
                {
                    return Ok(false);
                }

                self.match_substitution(*pattern_substitution, *actual_substitution)
            }
            _ => Ok(false),
        }
    }

    fn bind(&mut self, parameter: GenericParameterSymbolId, argument: GenericArgument) -> bool {
        match self.arguments.get(&parameter) {
            Some(previous) => *previous == argument,
            None => {
                self.arguments.insert(parameter, argument);
                true
            }
        }
    }
}
