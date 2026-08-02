use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{BoundExpression, BoundExpressionId, CheckedExpressionTypes};
use bray_symbols::{
    ConstantTermData, ConstantTermId, GenericArgument, GenericParameterSymbolId,
    GenericSubstitutionId, TypeData, TypeId,
};

use crate::{
    CallableCandidate, CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView,
};

pub(super) fn infer_call_generic_arguments<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    candidate: &CallableCandidate,
    parameters: &[GenericParameterSymbolId],
    types: &CheckedExpressionTypes,
) -> Result<Option<Vec<GenericArgument>>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(BoundExpression::Call(call)) = request.view().expression(expression) else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let callable = request
        .semantic_values()
        .type_data(candidate.callable_type())
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let TypeData::Callable(callable) = callable.as_ref() else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let Some(parameter_indices) =
        crate::selection::map_argument_parameter_indices(call.arguments(), callable.parameters())
    else {
        return Ok(None);
    };

    let inferable = parameters.iter().copied().collect::<BTreeSet<_>>();
    let mut inference = GenericArgumentInference::new(request, inferable);

    for (argument, parameter_index) in call.arguments().iter().zip(parameter_indices) {
        let Some(actual) = types.expression(argument.expression()) else {
            continue;
        };

        if actual.is_recovered() {
            continue;
        }

        let Some(parameter) = callable.parameters().get(parameter_index) else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        if !inference.infer_type(parameter.ty(), actual.ty())? {
            return Ok(None);
        }
    }

    let arguments = parameters
        .iter()
        .map(|parameter| inference.arguments.get(parameter).copied())
        .collect::<Option<Vec<_>>>();

    Ok(arguments)
}

struct GenericArgumentInference<'request, C>
where
    C: CheckerRequestContext + ?Sized,
{
    request: CheckerUnitView<'request, C>,
    inferable: BTreeSet<GenericParameterSymbolId>,
    arguments: BTreeMap<GenericParameterSymbolId, GenericArgument>,
}

impl<'request, C> GenericArgumentInference<'request, C>
where
    C: CheckerRequestContext + ?Sized,
{
    fn new(
        request: CheckerUnitView<'request, C>,
        inferable: BTreeSet<GenericParameterSymbolId>,
    ) -> Self {
        Self {
            request,
            inferable,
            arguments: BTreeMap::new(),
        }
    }

    fn infer_type(
        &mut self,
        expected: TypeId,
        actual: TypeId,
    ) -> Result<bool, CheckerInfrastructureError> {
        let expected_data = self.type_data(expected)?;

        if let TypeData::TypeParameter(parameter) = expected_data.as_ref() {
            let parameter = GenericParameterSymbolId::Type(*parameter);

            if self.inferable.contains(&parameter) {
                return Ok(self.record(parameter, GenericArgument::Type(actual)));
            }
        }

        if expected == actual {
            return Ok(true);
        }

        let actual_data = self.type_data(actual)?;

        match (expected_data.as_ref(), actual_data.as_ref()) {
            (
                TypeData::Named {
                    definition: expected_definition,
                    substitution: expected_substitution,
                },
                TypeData::Named {
                    definition: actual_definition,
                    substitution: actual_substitution,
                },
            ) if expected_definition == actual_definition => {
                self.infer_substitution(*expected_substitution, *actual_substitution)
            }
            (TypeData::Tuple(expected), TypeData::Tuple(actual))
                if expected.len() == actual.len() =>
            {
                self.infer_types(expected, actual)
            }
            (
                TypeData::Array {
                    element: expected_element,
                    length: expected_length,
                },
                TypeData::Array {
                    element: actual_element,
                    length: actual_length,
                },
            ) => Ok(self.infer_type(*expected_element, *actual_element)?
                && self.infer_constant(*expected_length, *actual_length)?),
            (TypeData::Slice(expected), TypeData::Slice(actual))
            | (TypeData::Generator(expected), TypeData::Generator(actual))
            | (TypeData::Nullable(expected), TypeData::Nullable(actual)) => {
                self.infer_type(*expected, *actual)
            }
            (
                TypeData::Borrow {
                    kind: expected_kind,
                    target: expected_target,
                },
                TypeData::Borrow {
                    kind: actual_kind,
                    target: actual_target,
                },
            ) if expected_kind == actual_kind => self.infer_type(*expected_target, *actual_target),
            (
                TypeData::OwnedIndirection {
                    storage: expected_storage,
                    target: expected_target,
                },
                TypeData::OwnedIndirection {
                    storage: actual_storage,
                    target: actual_target,
                },
            ) => Ok(self.infer_type(*expected_storage, *actual_storage)?
                && self.infer_type(*expected_target, *actual_target)?),
            (TypeData::Callable(expected), TypeData::Callable(actual))
                if expected.parameters().len() == actual.parameters().len()
                    && expected.constness() == actual.constness()
                    && expected.execution() == actual.execution()
                    && expected.trust() == actual.trust()
                    && expected.abi() == actual.abi() =>
            {
                for (expected, actual) in expected.parameters().iter().zip(actual.parameters()) {
                    if expected.name() != actual.name()
                        || expected.position() != actual.position()
                        || expected.mode() != actual.mode()
                        || !self.infer_type(expected.ty(), actual.ty())?
                    {
                        return Ok(false);
                    }
                }

                self.infer_type(expected.result(), actual.result())
            }
            _ => Ok(false),
        }
    }

    fn infer_types(
        &mut self,
        expected: &[TypeId],
        actual: &[TypeId],
    ) -> Result<bool, CheckerInfrastructureError> {
        for (expected, actual) in expected.iter().copied().zip(actual.iter().copied()) {
            if !self.infer_type(expected, actual)? {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn infer_substitution(
        &mut self,
        expected: GenericSubstitutionId,
        actual: GenericSubstitutionId,
    ) -> Result<bool, CheckerInfrastructureError> {
        let expected = self.substitution(expected)?;
        let actual = self.substitution(actual)?;

        if expected.bindings().len() != actual.bindings().len() {
            return Ok(false);
        }

        for (expected, actual) in expected.bindings().iter().zip(actual.bindings()) {
            let matches = match (expected.argument(), actual.argument()) {
                (GenericArgument::Type(expected), GenericArgument::Type(actual)) => {
                    self.infer_type(expected, actual)?
                }
                (GenericArgument::Constant(expected), GenericArgument::Constant(actual)) => {
                    self.infer_constant(expected, actual)?
                }
                _ => false,
            };

            if !matches {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn infer_constant(
        &mut self,
        expected: ConstantTermId,
        actual: ConstantTermId,
    ) -> Result<bool, CheckerInfrastructureError> {
        let expected_data = self
            .request
            .semantic_values()
            .constant_term_data(expected)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        if let ConstantTermData::Parameter(parameter) = expected_data.as_ref() {
            let parameter = GenericParameterSymbolId::Const(*parameter);

            if self.inferable.contains(&parameter) {
                return Ok(self.record(parameter, GenericArgument::Constant(actual)));
            }
        }

        Ok(expected == actual)
    }

    fn record(&mut self, parameter: GenericParameterSymbolId, argument: GenericArgument) -> bool {
        match self.arguments.get(&parameter) {
            Some(current) => *current == argument,
            None => {
                self.arguments.insert(parameter, argument);

                true
            }
        }
    }

    fn type_data(
        &self,
        ty: TypeId,
    ) -> Result<std::sync::Arc<TypeData>, CheckerInfrastructureError> {
        self.request
            .semantic_values()
            .type_data(ty)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
    }

    fn substitution(
        &self,
        substitution: GenericSubstitutionId,
    ) -> Result<std::sync::Arc<bray_symbols::GenericSubstitutionData>, CheckerInfrastructureError>
    {
        self.request
            .semantic_values()
            .generic_substitution_data(substitution)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use bray_bound_tree::BoundUnitId;
    use bray_symbols::{
        GenericArgument, GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData,
        GenericTypeParameterSymbolId, NamedTypeSymbolId, StructSymbolId, SymbolId, TypeData,
    };

    use super::GenericArgumentInference;
    use crate::CheckerUnitView;
    use crate::test_support::{
        TestCheckerContext, callable_entry, expression_unit, semantic_values,
    };

    #[test]
    fn nested_named_types_infer_open_type_arguments() {
        let (unit, _) = expression_unit(BoundUnitId::new(507), |_, _| Vec::new());

        let semantic_context = callable_entry(unit.key());
        let context = TestCheckerContext::new(false);

        let request = CheckerUnitView::new(&unit, &semantic_context, &context)
            .unwrap_or_else(|error| panic!("test checker unit view must be valid: {error:?}"));

        let values = semantic_values();

        let target_parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(1_001));

        let source_parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(1_002));

        let named_parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(1_003));

        let definition =
            NamedTypeSymbolId::Struct(StructSymbolId::from_symbol_id(SymbolId::new(1_004)));

        let expected_argument = values
            .intern_type(TypeData::TypeParameter(target_parameter))
            .unwrap_or_else(|error| panic!("target parameter type must intern: {error:?}"));

        let actual_argument = values
            .intern_type(TypeData::TypeParameter(source_parameter))
            .unwrap_or_else(|error| panic!("source parameter type must intern: {error:?}"));

        let owner = GenericOwnerId::try_new(definition.into_any())
            .unwrap_or_else(|| panic!("named type must be a generic owner"));

        let expected_substitution = GenericSubstitutionData::try_new(
            owner,
            [GenericParameterSymbolId::Type(named_parameter)],
            [GenericArgument::Type(expected_argument)],
        )
        .unwrap_or_else(|error| panic!("expected substitution must validate: {error:?}"));

        let expected_substitution = values
            .intern_generic_substitution(expected_substitution)
            .unwrap_or_else(|error| panic!("expected substitution must intern: {error:?}"));

        let actual_substitution = GenericSubstitutionData::try_new(
            owner,
            [GenericParameterSymbolId::Type(named_parameter)],
            [GenericArgument::Type(actual_argument)],
        )
        .unwrap_or_else(|error| panic!("actual substitution must validate: {error:?}"));

        let actual_substitution = values
            .intern_generic_substitution(actual_substitution)
            .unwrap_or_else(|error| panic!("actual substitution must intern: {error:?}"));

        let expected = values
            .intern_type(TypeData::Named {
                definition,
                substitution: expected_substitution,
            })
            .unwrap_or_else(|error| panic!("expected named type must intern: {error:?}"));

        let actual = values
            .intern_type(TypeData::Named {
                definition,
                substitution: actual_substitution,
            })
            .unwrap_or_else(|error| panic!("actual named type must intern: {error:?}"));

        let target = GenericParameterSymbolId::Type(target_parameter);
        let mut inference = GenericArgumentInference::new(request, BTreeSet::from([target]));

        assert_eq!(inference.infer_type(expected, actual), Ok(true));

        assert_eq!(
            inference.arguments.get(&target),
            Some(&GenericArgument::Type(actual_argument))
        );
    }
}
