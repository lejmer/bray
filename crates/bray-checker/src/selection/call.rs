use std::collections::BTreeSet;

use bray_bound_tree::{CheckedExpressionTypes, ExpressionTypeResult};
use bray_symbols::{
    CallableParameterDefaultProviderSymbolId, CallableParameterSignature,
    CallableParameterSymbolId, CallablePosition, CallableSignature, CallableTypeData, ReceiverMode,
    SymbolKey, TypeData,
};

use crate::{CheckerInfrastructureError, CheckerRequestContext, UnitCheckRequest};

use super::{
    CallableCandidate, CallableCandidateParts, CallableCandidateState, CallableSelectionMode,
    CallableSelectionRequest, CandidateSelection, ReceiverCapability, SelectedArgument,
    SelectedCall, SelectionFailure,
};

pub(super) fn select<C>(
    request: UnitCheckRequest<'_, C>,
    types: &CheckedExpressionTypes,
    input: CallableSelectionRequest,
) -> Result<Option<CandidateSelection<SelectedCall>>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if request.is_cancelled() {
        return Ok(None);
    }

    validate_unit(request, types, &input)?;

    let CallableSelectionRequest {
        expression,
        mode,
        receiver,
        arguments,
        mut candidates,
    } = input;

    let expression_type = expression_type(types, expression)?;

    if expression_type.is_recovered() {
        return Ok(Some(CandidateSelection::Failed(
            SelectionFailure::Recovered,
        )));
    }

    if !super::order::canonicalize_by_key(&mut candidates, CallableCandidate::key) {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    let mut applicable = Vec::new();
    let mut has_inaccessible = false;
    let mut has_incompatible = false;
    let mut has_recovered = false;

    for candidate in candidates {
        if request.is_cancelled() {
            return Ok(None);
        }

        let state = candidate.state();

        match state {
            CallableCandidateState::Unavailable => continue,
            CallableCandidateState::Inaccessible | CallableCandidateState::Available => {}
            CallableCandidateState::Recovered => {
                has_recovered = true;
                continue;
            }
        }

        match check_candidate(
            request,
            types,
            mode,
            receiver,
            &arguments,
            expression_type.ty(),
            candidate,
        )? {
            CandidateCheck::Applicable { .. } if state == CallableCandidateState::Inaccessible => {
                has_inaccessible = true;
            }
            CandidateCheck::Applicable { key, call } => applicable.push((key, call)),
            CandidateCheck::Incompatible => has_incompatible = true,
            CandidateCheck::Recovered => has_recovered = true,
        }
    }

    match applicable.len() {
        1 => Ok(Some(CandidateSelection::Selected(applicable.remove(0).1))),
        count if count > 1 => Ok(Some(CandidateSelection::Failed(
            SelectionFailure::Ambiguous(
                applicable.into_iter().map(|(key, _)| key.into()).collect(),
            ),
        ))),
        _ if has_recovered => Ok(Some(CandidateSelection::Failed(
            SelectionFailure::Recovered,
        ))),
        _ if has_incompatible => Ok(Some(CandidateSelection::Failed(
            SelectionFailure::Incompatible,
        ))),
        _ if has_inaccessible => Ok(Some(CandidateSelection::Failed(
            SelectionFailure::Inaccessible,
        ))),
        _ => Ok(Some(CandidateSelection::Failed(
            SelectionFailure::Unavailable,
        ))),
    }
}

enum CandidateCheck {
    Applicable { key: SymbolKey, call: SelectedCall },
    Incompatible,
    Recovered,
}

fn check_candidate<C>(
    request: UnitCheckRequest<'_, C>,
    types: &CheckedExpressionTypes,
    mode: CallableSelectionMode,
    receiver: Option<super::ReceiverSelection>,
    arguments: &[bray_bound_tree::BoundArgument],
    result_type: bray_symbols::TypeId,
    candidate: CallableCandidate,
) -> Result<CandidateCheck, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let CallableCandidateParts {
        key,
        resolution,
        signature,
        defaults,
    } = candidate.into_parts();

    let callable_type = callable_type(request, &signature)?;
    let TypeData::Callable(callable_type) = callable_type.as_ref() else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    if !callable_surface_is_consistent(
        &resolution,
        &signature,
        callable_type,
        &defaults,
        result_type,
    ) {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    match receiver_is_compatible(types, receiver, signature.receiver())? {
        Compatibility::No => return Ok(CandidateCheck::Incompatible),
        Compatibility::Recovered => return Ok(CandidateCheck::Recovered),
        Compatibility::Yes => {}
    }

    let Some(arguments) = map_arguments(
        types,
        mode,
        arguments,
        callable_type,
        signature.parameters(),
        &defaults,
    )?
    else {
        return Ok(CandidateCheck::Incompatible);
    };

    if arguments.recovered {
        return Ok(CandidateCheck::Recovered);
    }

    Ok(CandidateCheck::Applicable {
        key,
        call: SelectedCall::new(resolution, callable_type.abi(), arguments.values),
    })
}

fn callable_surface_is_consistent(
    resolution: &bray_bound_tree::BoundResolvedCall,
    signature: &CallableSignature,
    callable: &CallableTypeData,
    defaults: &[(
        CallableParameterSymbolId,
        CallableParameterDefaultProviderSymbolId,
    )],
    result_type: bray_symbols::TypeId,
) -> bool {
    if callable.parameters().len() != signature.parameters().len()
        || callable.result() != signature.result()
        || !callable
            .parameters()
            .iter()
            .zip(signature.parameters())
            .all(|(parameter, signature)| parameter.ty() == signature.ty())
        || defaults.windows(2).any(|pair| pair[0].0 == pair[1].0)
        || defaults.iter().any(|(parameter, _)| {
            !signature
                .parameters()
                .iter()
                .any(|item| item.parameter() == *parameter)
        })
    {
        return false;
    }

    resolution.result().ty() == result_type
        && match resolution.result() {
            bray_bound_tree::BoundCallResult::Immediate(result) => result == signature.result(),
            bray_bound_tree::BoundCallResult::LazyFuture(future) => {
                future.completion_type() == signature.result()
            }
        }
}

fn callable_type<C>(
    request: UnitCheckRequest<'_, C>,
    signature: &CallableSignature,
) -> Result<std::sync::Arc<TypeData>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let data = request
        .semantic_values()
        .type_data(signature.callable_type())
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let TypeData::Callable(_) = data.as_ref() else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    Ok(data)
}

fn receiver_is_compatible(
    types: &CheckedExpressionTypes,
    actual: Option<super::ReceiverSelection>,
    expected: Option<bray_symbols::ReceiverParameterSignature>,
) -> Result<Compatibility, CheckerInfrastructureError> {
    let (Some(actual), Some(expected)) = (actual, expected) else {
        return Ok(if actual.is_none() && expected.is_none() {
            Compatibility::Yes
        } else {
            Compatibility::No
        });
    };

    let actual_type = expression_type(types, actual.expression())?;

    if actual_type.is_recovered() {
        return Ok(Compatibility::Recovered);
    }

    if actual_type.ty() != expected.ty()
        || !receiver_capability_supports(actual.capability(), expected.mode())
    {
        return Ok(Compatibility::No);
    }

    Ok(Compatibility::Yes)
}

const fn receiver_capability_supports(actual: ReceiverCapability, expected: ReceiverMode) -> bool {
    match expected {
        ReceiverMode::Shared => true,
        ReceiverMode::Mutable => {
            matches!(
                actual,
                ReceiverCapability::Mutable | ReceiverCapability::OwnedMutable
            )
        }
        ReceiverMode::Consuming => {
            matches!(
                actual,
                ReceiverCapability::Owned | ReceiverCapability::OwnedMutable
            )
        }
        ReceiverMode::ConsumingMutable => matches!(actual, ReceiverCapability::OwnedMutable),
    }
}

struct MappedArguments {
    values: Vec<SelectedArgument>,
    recovered: bool,
}

fn map_arguments(
    types: &CheckedExpressionTypes,
    mode: CallableSelectionMode,
    arguments: &[bray_bound_tree::BoundArgument],
    callable: &CallableTypeData,
    signatures: &[CallableParameterSignature],
    defaults: &[(
        CallableParameterSymbolId,
        CallableParameterDefaultProviderSymbolId,
    )],
) -> Result<Option<MappedArguments>, CheckerInfrastructureError> {
    let parameters = callable.parameters();
    let mut supplied = vec![None; parameters.len()];
    let mut values = Vec::with_capacity(parameters.len());

    let mut positional_index = 0;

    let mut saw_named = false;
    let mut recovered = false;

    for argument in arguments {
        let parameter_index = match argument.name() {
            Some(name) => {
                saw_named = true;

                parameters
                    .iter()
                    .position(|parameter| parameter.name().as_str() == name.as_str())
            }
            None if saw_named => return Ok(None),
            None => {
                let index = positional_index;
                positional_index += 1;

                parameters.get(index).and_then(|parameter| {
                    (parameter.position() == CallablePosition::PositionalOrNamed).then_some(index)
                })
            }
        };

        let Some(parameter_index) = parameter_index else {
            return Ok(None);
        };

        if supplied[parameter_index].is_some() {
            return Ok(None);
        }

        let actual = expression_type(types, argument.expression())?;

        recovered |= argument.is_recovered() || actual.is_recovered();

        if !actual.is_recovered() && actual.ty() != parameters[parameter_index].ty() {
            return Ok(None);
        }

        supplied[parameter_index] = Some(argument.expression());
        values.push(SelectedArgument::Explicit {
            expression: argument.expression(),
            parameter: signatures[parameter_index].parameter(),
        });
    }

    let mut seen_parameters = BTreeSet::new();

    for (index, signature) in signatures.iter().copied().enumerate() {
        if !seen_parameters.insert(signature.parameter()) {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        }

        if supplied[index].is_some() {
            continue;
        }

        if mode == CallableSelectionMode::Overload {
            return Ok(None);
        }

        let Ok(default_index) =
            defaults.binary_search_by_key(&signature.parameter(), |(parameter, _)| *parameter)
        else {
            return Ok(None);
        };

        let Some((_, provider)) = defaults.get(default_index).copied() else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        values.push(SelectedArgument::Default {
            parameter: signature.parameter(),
            provider,
        });
    }

    Ok(Some(MappedArguments { values, recovered }))
}

#[derive(Clone, Copy)]
enum Compatibility {
    Yes,
    No,
    Recovered,
}

fn expression_type(
    types: &CheckedExpressionTypes,
    expression: bray_bound_tree::BoundExpressionId,
) -> Result<ExpressionTypeResult, CheckerInfrastructureError> {
    types
        .expression(expression)
        .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)
}

fn validate_unit<C>(
    request: UnitCheckRequest<'_, C>,
    types: &CheckedExpressionTypes,
    input: &CallableSelectionRequest,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if types.unit() != request.view().unit()
        || types.kind() != request.view().kind()
        || !matches!(
            request.view().expression(input.expression()),
            Some(bray_bound_tree::BoundExpression::Call(call))
                if call.arguments() == input.arguments()
        )
    {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundArgument, BoundCallExpression, BoundCallResult, BoundCallableTarget, BoundExpression,
        BoundExpressionId, BoundResolvedCall, BoundUnit, BoundUnitId, CheckedExpressionTypes,
        ExpressionTypeEntry, ExpressionTypeResult, ExpressionTypeStatus,
    };
    use bray_symbols::{
        CallableAbi, CallableConstness, CallableDependencyContracts, CallableParameterData,
        CallableParameterDefaultProviderSymbolId, CallableParameterMode, CallableParameterName,
        CallableParameterSignature, CallableParameterSymbolId, CallablePosition, CallableSignature,
        CallableTrust, CallableTypeData, DependencyContractTemplateData, SymbolId, SymbolKind,
        TypeData, TypeId,
    };

    use crate::test_support::{
        TestCheckerContext, callable_entry, declaration_key, expression_unit, push_expression,
        semantic_values, symbol_name, tuple_type, unselected_name_expression,
    };
    use crate::{
        CallableCandidate, CallableCandidateState, CallableSelectionMode, CallableSelectionRequest,
        CandidateSelection, DefaultSemanticSelector, SelectedArgument, SelectionFailure,
        SemanticSelector, UnitCheckRequest,
    };

    #[test]
    fn direct_calls_map_named_arguments_before_declaration_order_defaults() {
        let fixture = call_fixture(BoundUnitId::new(70), true);
        let candidate = callable_candidate(1, fixture.value_type, true);

        let input = named_request(&fixture, CallableSelectionMode::Direct, [candidate]);

        let result = select(&fixture.unit, &fixture.types, input);

        assert!(result.diagnostics().is_empty());

        let CandidateSelection::Selected(call) = result.value() else {
            panic!("one direct callable must be selected");
        };

        assert_eq!(call.abi(), CallableAbi::C);

        assert_eq!(
            call.arguments(),
            [
                SelectedArgument::Explicit {
                    expression: fixture.argument,
                    parameter: parameter(2),
                },
                SelectedArgument::Default {
                    parameter: parameter(1),
                    provider: default_provider(1),
                },
            ]
        );
    }

    #[test]
    fn overload_selection_does_not_use_omitted_parameter_defaults() {
        let fixture = call_fixture(BoundUnitId::new(71), true);
        let candidate = callable_candidate(1, fixture.value_type, true);

        let input = named_request(&fixture, CallableSelectionMode::Overload, [candidate]);

        let result = select(&fixture.unit, &fixture.types, input);

        assert!(matches!(
            result.value(),
            CandidateSelection::Failed(SelectionFailure::Incompatible)
        ));
    }

    #[test]
    fn positional_arguments_require_positional_parameter_permission() {
        let fixture = call_fixture(BoundUnitId::new(72), false);
        let candidate = callable_candidate(1, fixture.value_type, false);

        let input = CallableSelectionRequest::new(
            fixture.call,
            CallableSelectionMode::Direct,
            None,
            [BoundArgument::new(fixture.argument, None, false)],
            [candidate],
        );

        let result = select(&fixture.unit, &fixture.types, input);

        assert!(matches!(
            result.value(),
            CandidateSelection::Failed(SelectionFailure::Incompatible)
        ));
    }

    #[test]
    fn inaccessible_candidates_are_reported_only_when_otherwise_applicable() {
        let fixture = call_fixture(BoundUnitId::new(73), true);

        let applicable = callable_candidate_with_state(
            1,
            fixture.value_type,
            true,
            CallableCandidateState::Inaccessible,
        );

        let input = named_request(&fixture, CallableSelectionMode::Direct, [applicable]);

        let result = select(&fixture.unit, &fixture.types, input);

        assert!(matches!(
            result.value(),
            CandidateSelection::Failed(SelectionFailure::Inaccessible)
        ));

        let incompatible_type = tuple_type([fixture.value_type]);

        let incompatible = callable_candidate_with_types(
            2,
            incompatible_type,
            fixture.value_type,
            true,
            CallableCandidateState::Inaccessible,
        );

        let input = named_request(&fixture, CallableSelectionMode::Direct, [incompatible]);

        let result = select(&fixture.unit, &fixture.types, input);

        assert!(matches!(
            result.value(),
            CandidateSelection::Failed(SelectionFailure::Incompatible)
        ));
    }

    #[test]
    fn unavailable_candidates_do_not_participate() {
        let fixture = call_fixture(BoundUnitId::new(74), true);
        let candidate = callable_candidate_with_state(
            1,
            fixture.value_type,
            true,
            CallableCandidateState::Unavailable,
        );

        let input = named_request(&fixture, CallableSelectionMode::Direct, [candidate]);
        let result = select(&fixture.unit, &fixture.types, input);

        assert!(matches!(
            result.value(),
            CandidateSelection::Failed(SelectionFailure::Unavailable)
        ));
    }

    #[test]
    fn recovered_candidates_preserve_recovery() {
        let fixture = call_fixture(BoundUnitId::new(75), true);
        let candidate = callable_candidate_with_state(
            1,
            fixture.value_type,
            true,
            CallableCandidateState::Recovered,
        );

        let input = named_request(&fixture, CallableSelectionMode::Direct, [candidate]);
        let result = select(&fixture.unit, &fixture.types, input);

        assert!(matches!(
            result.value(),
            CandidateSelection::Failed(SelectionFailure::Recovered)
        ));
    }

    #[test]
    fn cancellation_publishes_no_selection_or_diagnostics() {
        let fixture = call_fixture(BoundUnitId::new(76), true);
        let candidate = callable_candidate(1, fixture.value_type, true);
        let input = named_request(&fixture, CallableSelectionMode::Direct, [candidate]);
        let context = TestCheckerContext::new(true);

        let outcome = select_outcome(&fixture.unit, &fixture.types, input, &context);

        assert_eq!(outcome, crate::CheckerOutcome::Cancelled);
    }

    struct CallFixture {
        unit: BoundUnit,
        types: CheckedExpressionTypes,
        call: BoundExpressionId,
        argument: BoundExpressionId,
        value_type: TypeId,
    }

    fn named_request(
        fixture: &CallFixture,
        mode: CallableSelectionMode,
        candidates: impl IntoIterator<Item = CallableCandidate>,
    ) -> CallableSelectionRequest {
        CallableSelectionRequest::new(
            fixture.call,
            mode,
            None,
            [BoundArgument::new(
                fixture.argument,
                Some(symbol_name("second")),
                false,
            )],
            candidates,
        )
    }

    fn call_fixture(unit: BoundUnitId, named_argument: bool) -> CallFixture {
        let value_type = tuple_type([]);

        let (unit, expressions) = expression_unit(unit, |tree, origin| {
            let callee = push_expression(tree, unselected_name_expression(origin));

            let argument = push_expression(
                tree,
                crate::test_support::integer_literal_expression(origin, Some(value_type)),
            );

            let name = named_argument.then(|| symbol_name("second"));

            let call = push_expression(
                tree,
                BoundExpression::Call(BoundCallExpression::pending(
                    origin,
                    callee,
                    [BoundArgument::new(argument, name, false)],
                )),
            );

            vec![callee, argument, call]
        });

        let result = ExpressionTypeResult::new(value_type, ExpressionTypeStatus::Valid);

        let types = CheckedExpressionTypes::new(
            unit.unit(),
            unit.key().kind(),
            expressions
                .iter()
                .copied()
                .map(|expression| ExpressionTypeEntry::new(expression, result)),
        );

        CallFixture {
            call: expressions[2],
            argument: expressions[1],
            unit,
            types,
            value_type,
        }
    }

    fn callable_candidate(
        declaration: u32,
        value_type: TypeId,
        two_parameters: bool,
    ) -> CallableCandidate {
        callable_candidate_with_state(
            declaration,
            value_type,
            two_parameters,
            CallableCandidateState::Available,
        )
    }

    fn callable_candidate_with_state(
        declaration: u32,
        value_type: TypeId,
        two_parameters: bool,
        state: CallableCandidateState,
    ) -> CallableCandidate {
        callable_candidate_with_types(declaration, value_type, value_type, two_parameters, state)
    }

    fn callable_candidate_with_types(
        declaration: u32,
        parameter_type: TypeId,
        result_type: TypeId,
        two_parameters: bool,
        state: CallableCandidateState,
    ) -> CallableCandidate {
        let parameters = if two_parameters {
            vec![
                parameter_data("first", CallablePosition::NamedOnly, parameter_type),
                parameter_data("second", CallablePosition::NamedOnly, parameter_type),
            ]
        } else {
            vec![parameter_data(
                "first",
                CallablePosition::NamedOnly,
                parameter_type,
            )]
        };

        let dependency = match semantic_values()
            .intern_dependency_contract_template(DependencyContractTemplateData::new([]))
        {
            Ok(dependency) => dependency,
            Err(error) => panic!("empty dependency contract must be valid: {error:?}"),
        };

        let callable_type = CallableTypeData::new(
            parameters,
            result_type,
            CallableConstness::Runtime,
            CallableTrust::Safe,
            CallableAbi::C,
            CallableDependencyContracts::synchronous(dependency),
        );

        let callable_type = match semantic_values().intern_type(TypeData::Callable(callable_type)) {
            Ok(callable_type) => callable_type,
            Err(error) => panic!("callable test type must be valid: {error:?}"),
        };

        let signatures = if two_parameters {
            vec![
                CallableParameterSignature::new(parameter(1), parameter_type),
                CallableParameterSignature::new(parameter(2), parameter_type),
            ]
        } else {
            vec![CallableParameterSignature::new(
                parameter(1),
                parameter_type,
            )]
        };

        let signature = CallableSignature::new(callable_type, None, signatures, result_type);

        let defaults = two_parameters
            .then_some((parameter(1), default_provider(1)))
            .into_iter();

        let resolution = BoundResolvedCall::new(
            BoundCallableTarget::Indirect(callable_type),
            [],
            BoundCallResult::Immediate(result_type),
        );

        CallableCandidate::new(
            declaration_key(SymbolKind::Function, declaration),
            resolution,
            signature,
            defaults,
            state,
        )
    }

    fn parameter_data(name: &str, position: CallablePosition, ty: TypeId) -> CallableParameterData {
        let Some(name) = CallableParameterName::try_new(name) else {
            panic!("test parameter name must be valid");
        };

        CallableParameterData::new(name, position, CallableParameterMode::Immutable, ty)
    }

    fn select(
        unit: &BoundUnit,
        types: &CheckedExpressionTypes,
        input: CallableSelectionRequest,
    ) -> bray_diagnostics::DiagnosticResult<CandidateSelection<crate::SelectedCall>> {
        let context = TestCheckerContext::new(false);

        match select_outcome(unit, types, input, &context) {
            crate::CheckerOutcome::Complete(result) => result,
            other => panic!("call selection must complete: {other:?}"),
        }
    }

    fn select_outcome<'unit>(
        unit: &'unit BoundUnit,
        types: &CheckedExpressionTypes,
        input: CallableSelectionRequest,
        context: &'unit TestCheckerContext,
    ) -> crate::CheckerOutcome<CandidateSelection<crate::SelectedCall>> {
        let entry = callable_entry(unit.key());

        let request = match UnitCheckRequest::new(unit, &entry, context) {
            Ok(request) => request,
            Err(error) => panic!("call selection request must be valid: {error:?}"),
        };

        DefaultSemanticSelector.select_callable(request, types, input)
    }

    fn parameter(index: u32) -> CallableParameterSymbolId {
        CallableParameterSymbolId::from_symbol_id(SymbolId::new(index))
    }

    fn default_provider(index: u32) -> CallableParameterDefaultProviderSymbolId {
        CallableParameterDefaultProviderSymbolId::from_symbol_id(SymbolId::new(10 + index))
    }
}
