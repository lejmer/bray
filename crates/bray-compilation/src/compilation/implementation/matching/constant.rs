use bray_symbols::{
    CallableInstanceId, ConstantProjection, ConstantProjectionKind, ConstantTermData,
    ConstantTermId, ConstantValueId, ConstantValueKind, GenericArgument, GenericParameterSymbolId,
    ImplementationInstanceId, SemanticValueStoreError,
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

        if let ConstantTermData::Value(actual) = actual_data.as_ref() {
            return self.match_closed_constant(pattern_data.as_ref(), *actual);
        }

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
                ConstantTermData::Conversion {
                    operand: pattern_operand,
                    target: pattern_target,
                },
                ConstantTermData::Conversion {
                    operand: actual_operand,
                    target: actual_target,
                },
            ) => {
                if !self.match_type(*pattern_target, *actual_target)? {
                    return Ok(false);
                }

                self.match_constant(*pattern_operand, *actual_operand)
            }
            (
                ConstantTermData::NullablePresent(pattern),
                ConstantTermData::NullablePresent(actual),
            ) => self.match_constant(*pattern, *actual),
            (ConstantTermData::Tuple(pattern), ConstantTermData::Tuple(actual))
            | (ConstantTermData::Array(pattern), ConstantTermData::Array(actual)) => {
                self.match_constants(pattern, actual)
            }
            (ConstantTermData::Product(pattern), ConstantTermData::Product(actual)) => {
                if pattern.len() != actual.len() {
                    return Ok(false);
                }

                for (pattern, actual) in pattern.iter().zip(actual.iter()) {
                    if pattern.field() != actual.field()
                        || !self.match_constant(*pattern.value(), *actual.value())?
                    {
                        return Ok(false);
                    }
                }

                Ok(true)
            }
            (
                ConstantTermData::Union {
                    variant: pattern_variant,
                    fields: pattern_fields,
                },
                ConstantTermData::Union {
                    variant: actual_variant,
                    fields: actual_fields,
                },
            ) => {
                if pattern_variant != actual_variant || pattern_fields.len() != actual_fields.len()
                {
                    return Ok(false);
                }

                for (pattern, actual) in pattern_fields.iter().zip(actual_fields.iter()) {
                    if pattern.field() != actual.field()
                        || !self.match_constant(*pattern.value(), *actual.value())?
                    {
                        return Ok(false);
                    }
                }

                Ok(true)
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
                    selected_implementation: pattern_implementation,
                    arguments: pattern_arguments,
                },
                ConstantTermData::Call {
                    callable: actual_callable,
                    selected_implementation: actual_implementation,
                    arguments: actual_arguments,
                },
            ) => {
                if !self.match_callable_instance(*pattern_callable, *actual_callable)? {
                    return Ok(false);
                }

                if !self.match_optional_implementation(
                    *pattern_implementation,
                    *actual_implementation,
                )? {
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

    fn match_closed_constant(
        &mut self,
        pattern: &ConstantTermData,
        actual: ConstantValueId,
    ) -> Result<bool, SemanticValueStoreError> {
        let actual_id = actual;
        let actual = self.values.constant_value_data(actual_id)?;

        match (pattern, actual.kind()) {
            (ConstantTermData::Value(pattern), _) => Ok(*pattern == actual_id),
            (
                ConstantTermData::NullablePresent(pattern),
                ConstantValueKind::NullablePresent(actual),
            ) => self.match_constant_to_value(*pattern, *actual),
            (ConstantTermData::Tuple(pattern), ConstantValueKind::Tuple(actual))
            | (ConstantTermData::Array(pattern), ConstantValueKind::Array(actual)) => {
                self.match_constants_to_values(pattern, actual)
            }
            (ConstantTermData::Product(pattern), ConstantValueKind::Product(actual)) => {
                if pattern.len() != actual.len() {
                    return Ok(false);
                }

                for (pattern, actual) in pattern.iter().zip(actual.iter()) {
                    if pattern.field() != actual.field()
                        || !self.match_constant_to_value(*pattern.value(), *actual.value())?
                    {
                        return Ok(false);
                    }
                }

                Ok(true)
            }
            (
                ConstantTermData::Union {
                    variant: pattern_variant,
                    fields: pattern_fields,
                },
                ConstantValueKind::Union {
                    variant: actual_variant,
                    fields: actual_fields,
                },
            ) => {
                if pattern_variant != actual_variant || pattern_fields.len() != actual_fields.len()
                {
                    return Ok(false);
                }

                for (pattern, actual) in pattern_fields.iter().zip(actual_fields.iter()) {
                    if pattern.field() != actual.field()
                        || !self.match_constant_to_value(*pattern.value(), *actual.value())?
                    {
                        return Ok(false);
                    }
                }

                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn match_constant_to_value(
        &mut self,
        pattern: ConstantTermId,
        actual: ConstantValueId,
    ) -> Result<bool, SemanticValueStoreError> {
        let actual = self
            .values
            .intern_constant_term(ConstantTermData::Value(actual))?;

        self.match_constant(pattern, actual)
    }

    fn match_constants_to_values(
        &mut self,
        pattern: &[ConstantTermId],
        actual: &[ConstantValueId],
    ) -> Result<bool, SemanticValueStoreError> {
        if pattern.len() != actual.len() {
            return Ok(false);
        }

        for (pattern, actual) in pattern.iter().zip(actual) {
            if !self.match_constant_to_value(*pattern, *actual)? {
                return Ok(false);
            }
        }

        Ok(true)
    }

    pub(super) fn match_callable_instance(
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
            (
                ConstantProjectionKind::ArraySlice {
                    lower: pattern_lower,
                    upper: pattern_upper,
                },
                ConstantProjectionKind::ArraySlice {
                    lower: actual_lower,
                    upper: actual_upper,
                },
            ) => {
                for (pattern, actual) in
                    [(pattern_lower, actual_lower), (pattern_upper, actual_upper)]
                {
                    match (pattern, actual) {
                        (Some(pattern), Some(actual))
                            if self.match_constant(pattern, actual)? => {}
                        (None, None) => {}
                        _ => return Ok(false),
                    }
                }

                Ok(true)
            }
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
            .intern_constant_term(ConstantTermData::call(callable, None, [parameter_term]))
            .unwrap_or_else(|error| panic!("pattern call must be valid: {error:?}"));

        let actual_call = values
            .intern_constant_term(ConstantTermData::call(callable, None, [value_term]))
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

        for (pattern_kind, actual_kind) in [
            (
                ConstantProjectionKind::ArrayElement(parameter_term),
                ConstantProjectionKind::ArrayElement(value_term),
            ),
            (
                ConstantProjectionKind::ArraySlice {
                    lower: Some(parameter_term),
                    upper: None,
                },
                ConstantProjectionKind::ArraySlice {
                    lower: Some(value_term),
                    upper: None,
                },
            ),
            (
                ConstantProjectionKind::ArraySlice {
                    lower: None,
                    upper: Some(parameter_term),
                },
                ConstantProjectionKind::ArraySlice {
                    lower: None,
                    upper: Some(value_term),
                },
            ),
        ] {
            let pattern_projection = values
                .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
                    value_term,
                    pattern_kind,
                )))
                .unwrap_or_else(|error| panic!("pattern projection must be valid: {error:?}"));

            let actual_projection = values
                .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
                    value_term,
                    actual_kind,
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
    }

    #[test]
    fn open_aggregate_patterns_match_closed_aggregate_values() {
        let values = semantic_values();
        let parameter = GenericConstParameterSymbolId::from_symbol_id(SymbolId::new(1));
        let parameter_id = GenericParameterSymbolId::Const(parameter);

        let parameter_term = values
            .intern_constant_term(ConstantTermData::Parameter(parameter))
            .unwrap_or_else(|error| panic!("parameter term must be valid: {error:?}"));

        let pattern = values
            .intern_constant_term(ConstantTermData::array([parameter_term]))
            .unwrap_or_else(|error| panic!("aggregate pattern must be valid: {error:?}"));

        let child_term = constant_value_term(&values);

        let child = values
            .constant_term_data(child_term)
            .unwrap_or_else(|error| panic!("closed child term must resolve: {error:?}"));

        let ConstantTermData::Value(child) = child.as_ref() else {
            panic!("test child must be a closed value");
        };

        let child_data = values
            .constant_value_data(*child)
            .unwrap_or_else(|error| panic!("closed child value must resolve: {error:?}"));

        let aggregate = values
            .intern_constant_value(ConstantValueData::new(
                child_data.ty(),
                ConstantValueKind::array([*child]),
            ))
            .unwrap_or_else(|error| panic!("closed aggregate value must be valid: {error:?}"));

        let actual = values
            .intern_constant_term(ConstantTermData::Value(aggregate))
            .unwrap_or_else(|error| panic!("closed aggregate term must be valid: {error:?}"));

        let mut matcher = HeaderMatcher::new(&[parameter_id], &values);

        assert_eq!(matcher.match_constant(pattern, actual), Ok(true));

        assert_eq!(
            matcher.arguments.get(&parameter_id),
            Some(&bray_symbols::GenericArgument::Constant(child_term))
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
