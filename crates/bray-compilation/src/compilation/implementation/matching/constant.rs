use bray_symbols::{
    CallableInstanceId, ConstantProjection, ConstantProjectionKind, ConstantTermData,
    ConstantTermId, GenericArgument, GenericParameterSymbolId, ImplementationInstanceId,
    SemanticValueStoreError,
};

use super::header::HeaderMatcher;

impl HeaderMatcher<'_> {
    pub(super) fn match_constant(
        &mut self,
        pattern: ConstantTermId,
        actual: ConstantTermId,
    ) -> Result<bool, SemanticValueStoreError> {
        let pattern_data = self.values.constant_term_data(pattern)?;

        if let ConstantTermData::Parameter(parameter) = pattern_data.as_ref() {
            let parameter = GenericParameterSymbolId::Const(*parameter);

            if self.parameters.contains(&parameter) {
                return Ok(self.bind(parameter, GenericArgument::Constant(actual)));
            }
        }

        if pattern == actual {
            return Ok(true);
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
                if pattern_definition != actual_definition {
                    return Ok(false);
                }

                if !self.match_substitution(*pattern_substitution, *actual_substitution)? {
                    return Ok(false);
                }

                self.match_optional_implementation(*pattern_implementation, *actual_implementation)
            }
            (
                ConstantTermData::Call {
                    callable: pattern_callable,
                    arguments: pattern_arguments,
                },
                ConstantTermData::Call {
                    callable: actual_callable,
                    arguments: actual_arguments,
                },
            ) => {
                if !self.match_callable_instance(*pattern_callable, *actual_callable)? {
                    return Ok(false);
                }

                self.match_constants(pattern_arguments, actual_arguments)
            }
            (ConstantTermData::Projection(pattern), ConstantTermData::Projection(actual)) => {
                self.match_projection(*pattern, *actual)
            }
            _ => Ok(false),
        }
    }

    fn match_callable_instance(
        &mut self,
        pattern: CallableInstanceId,
        actual: CallableInstanceId,
    ) -> Result<bool, SemanticValueStoreError> {
        if pattern == actual {
            return Ok(true);
        }

        let pattern = self.values.callable_instance_data(pattern)?;
        let actual = self.values.callable_instance_data(actual)?;

        if pattern.definition() != actual.definition() {
            return Ok(false);
        }

        self.match_substitution(pattern.substitution(), actual.substitution())
    }

    fn match_optional_implementation(
        &mut self,
        pattern: Option<ImplementationInstanceId>,
        actual: Option<ImplementationInstanceId>,
    ) -> Result<bool, SemanticValueStoreError> {
        match (pattern, actual) {
            (Some(pattern), Some(actual)) => self.match_implementation_instance(pattern, actual),
            (None, None) => Ok(true),
            (Some(_), None) | (None, Some(_)) => Ok(false),
        }
    }

    pub(super) fn match_implementation_instance(
        &mut self,
        pattern: ImplementationInstanceId,
        actual: ImplementationInstanceId,
    ) -> Result<bool, SemanticValueStoreError> {
        if pattern == actual {
            return Ok(true);
        }

        let pattern = self.values.implementation_instance_data(pattern)?;
        let actual = self.values.implementation_instance_data(actual)?;

        if pattern.definition() != actual.definition() {
            return Ok(false);
        }

        self.match_substitution(pattern.substitution(), actual.substitution())
    }

    fn match_constants(
        &mut self,
        pattern: &[ConstantTermId],
        actual: &[ConstantTermId],
    ) -> Result<bool, SemanticValueStoreError> {
        if pattern.len() != actual.len() {
            return Ok(false);
        }

        for (pattern, actual) in pattern.iter().zip(actual) {
            if !self.match_constant(*pattern, *actual)? {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn match_projection(
        &mut self,
        pattern: ConstantProjection,
        actual: ConstantProjection,
    ) -> Result<bool, SemanticValueStoreError> {
        if !self.match_constant(pattern.subject(), actual.subject())? {
            return Ok(false);
        }

        match (pattern.kind(), actual.kind()) {
            (
                ConstantProjectionKind::ArrayElement(pattern),
                ConstantProjectionKind::ArrayElement(actual),
            ) => self.match_constant(pattern, actual),
            (pattern, actual) => Ok(pattern == actual),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_symbols::{
        CallableDefinitionId, CallableInstanceData, ConstantProjection, ConstantProjectionKind,
        ConstantTermData, ConstantValueData, ConstantValueKind, FunctionSymbolId,
        GenericConstParameterSymbolId, GenericOwnerId, GenericParameterSymbolId,
        GenericSubstitutionData, NamedTypeSymbolId, SemanticValueStore, StructSymbolId, SymbolId,
        TypeData,
    };

    use super::HeaderMatcher;

    #[test]
    fn constant_calls_and_projections_infer_nested_parameters() {
        let values = semantic_values();
        let parameter = GenericConstParameterSymbolId::from_symbol_id(SymbolId::new(1));
        let parameter_id = GenericParameterSymbolId::Const(parameter);

        let parameter_term = values
            .intern_constant_term(ConstantTermData::Parameter(parameter))
            .unwrap_or_else(|error| panic!("parameter term must be valid: {error:?}"));

        let value_term = constant_value_term(&values);
        let callable = callable_instance(&values);

        let pattern_call = values
            .intern_constant_term(ConstantTermData::call(callable, [parameter_term]))
            .unwrap_or_else(|error| panic!("pattern call must be valid: {error:?}"));

        let actual_call = values
            .intern_constant_term(ConstantTermData::call(callable, [value_term]))
            .unwrap_or_else(|error| panic!("actual call must be valid: {error:?}"));

        let mut call_matcher = HeaderMatcher::new(&[parameter_id], &values);

        assert_eq!(
            call_matcher.match_constant(pattern_call, actual_call),
            Ok(true)
        );

        assert_eq!(
            call_matcher.arguments.get(&parameter_id),
            Some(&bray_symbols::GenericArgument::Constant(value_term))
        );

        let pattern_projection = values
            .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
                pattern_call,
                ConstantProjectionKind::ArrayElement(parameter_term),
            )))
            .unwrap_or_else(|error| panic!("pattern projection must be valid: {error:?}"));

        let actual_projection = values
            .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
                actual_call,
                ConstantProjectionKind::ArrayElement(value_term),
            )))
            .unwrap_or_else(|error| panic!("actual projection must be valid: {error:?}"));

        let mut projection_matcher = HeaderMatcher::new(&[parameter_id], &values);

        assert_eq!(
            projection_matcher.match_constant(pattern_projection, actual_projection),
            Ok(true)
        );

        assert_eq!(
            projection_matcher.arguments.get(&parameter_id),
            Some(&bray_symbols::GenericArgument::Constant(value_term))
        );
    }

    fn semantic_values() -> SemanticValueStore {
        SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic value store must build: {error:?}"))
    }

    fn constant_value_term(values: &SemanticValueStore) -> bray_symbols::ConstantTermId {
        let structure = StructSymbolId::from_symbol_id(SymbolId::new(2));

        let substitution = empty_substitution(values, structure.into());

        let ty = values
            .intern_type(TypeData::Named {
                definition: NamedTypeSymbolId::Struct(structure),
                substitution,
            })
            .unwrap_or_else(|error| panic!("constant type must be valid: {error:?}"));

        let value = values
            .intern_constant_value(ConstantValueData::new(
                ty,
                ConstantValueKind::Tuple(Arc::from([])),
            ))
            .unwrap_or_else(|error| panic!("constant value must be valid: {error:?}"));

        values
            .intern_constant_term(ConstantTermData::Value(value))
            .unwrap_or_else(|error| panic!("constant value term must be valid: {error:?}"))
    }

    fn callable_instance(values: &SemanticValueStore) -> bray_symbols::CallableInstanceId {
        let function = FunctionSymbolId::from_symbol_id(SymbolId::new(3));

        let definition = CallableDefinitionId::try_new(function.into())
            .unwrap_or_else(|| panic!("function must be callable"));

        let substitution = empty_substitution(values, function.into());

        values
            .intern_callable_instance(CallableInstanceData::new(definition, substitution))
            .unwrap_or_else(|error| panic!("callable instance must be valid: {error:?}"))
    }

    fn empty_substitution(
        values: &SemanticValueStore,
        owner: bray_symbols::AnySymbolId,
    ) -> bray_symbols::GenericSubstitutionId {
        let owner = GenericOwnerId::try_new(owner)
            .unwrap_or_else(|| panic!("test owner must support generic substitution"));

        let substitution = GenericSubstitutionData::try_new(owner, [], [])
            .unwrap_or_else(|error| panic!("empty substitution must be valid: {error:?}"));

        values
            .intern_generic_substitution(substitution)
            .unwrap_or_else(|error| panic!("empty substitution must be interned: {error:?}"))
    }
}
