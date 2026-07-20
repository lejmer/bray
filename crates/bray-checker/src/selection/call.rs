use std::collections::BTreeSet;

use bray_bound_tree::{
    BoundArgument, BoundCallableTarget, BoundGenericArgument, CheckedExpressionTypes,
    ExpressionTypeResult, SelectedArgument, SelectedCall, SelectedImplementationWitness,
};
use bray_symbols::{
    CallableParameterDefaultProviderSymbolId, CallableParameterSignature,
    CallableParameterSymbolId, CallablePosition, CallableSignature, CallableTypeData,
    ImplementationSelection, ReceiverMode, SymbolKey, TypeData,
};

use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

use super::{
    CallableCandidate, CallableCandidateParts, CallableCandidateState, CallableSelectionRequest,
    CandidateSelection, ImplementationSelectionEvidence, ReceiverCapability, SelectionFailure,
};

#[derive(Clone, Copy, Eq, PartialEq)]
enum CallableSelectionMode {
    Direct,
    Overload,
}

pub(super) fn select<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    input: CallableSelectionRequest,
) -> Result<Option<CandidateSelection<SelectedCall>>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if request.is_cancelled() {
        return Ok(None);
    }

    let mode = validate_unit(request, types, &input)?;

    let CallableSelectionRequest {
        expression: _,
        callee_member: _,
        receiver,
        generic_arguments,
        arguments,
        mut candidates,
    } = input;

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
            &generic_arguments,
            &arguments,
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
        _ if has_inaccessible => Ok(Some(CandidateSelection::Failed(
            SelectionFailure::Inaccessible,
        ))),
        _ if has_recovered => Ok(Some(CandidateSelection::Failed(
            SelectionFailure::Recovered,
        ))),
        _ if has_incompatible => Ok(Some(CandidateSelection::Failed(
            SelectionFailure::Incompatible,
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
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    mode: CallableSelectionMode,
    receiver: Option<super::ReceiverSelection>,
    generic_arguments: &[BoundGenericArgument],
    arguments: &[BoundArgument],
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
        implementation_selections,
    } = candidate.into_parts();

    let callable_type = callable_type(request, &signature)?;
    let TypeData::Callable(callable_type) = callable_type.as_ref() else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    if !callable_surface_is_consistent(&resolution, &signature, callable_type, &defaults) {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    if let bray_bound_tree::BoundCallableTarget::Declaration(callable) = resolution.target() {
        request
            .semantic_values()
            .intern_callable_instance(callable)
            .map_err(|_| CheckerInfrastructureError::InvalidSemanticSelectionInput)?;
    }

    match generic_arguments_are_compatible(request, generic_arguments, resolution.target())? {
        Compatibility::No => return Ok(CandidateCheck::Incompatible),
        Compatibility::Recovered => return Ok(CandidateCheck::Recovered),
        Compatibility::Yes => {}
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

    let Some(witnesses) = selected_witnesses(request, &resolution, &implementation_selections)?
    else {
        return Ok(CandidateCheck::Incompatible);
    };

    Ok(CandidateCheck::Applicable {
        key,
        call: SelectedCall::new(resolution, callable_type.abi(), arguments.values, witnesses),
    })
}

fn generic_arguments_are_compatible<C>(
    request: CheckerUnitView<'_, C>,
    arguments: &[BoundGenericArgument],
    target: BoundCallableTarget,
) -> Result<Compatibility, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if arguments.iter().any(|argument| argument.is_recovered()) {
        return Ok(Compatibility::Recovered);
    }

    let expected_count = match target {
        BoundCallableTarget::Declaration(callable) => request
            .semantic_values()
            .generic_substitution_data(callable.substitution())
            .map_err(|_| CheckerInfrastructureError::InvalidSemanticSelectionInput)?
            .bindings()
            .len(),
        BoundCallableTarget::Anonymous(_) | BoundCallableTarget::Indirect(_) => 0,
    };

    Ok(if arguments.len() == expected_count {
        Compatibility::Yes
    } else {
        Compatibility::No
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

    match resolution.result() {
        bray_bound_tree::BoundCallResult::Immediate(result) => result == signature.result(),
        bray_bound_tree::BoundCallResult::LazyFuture(future) => {
            future.completion_type() == signature.result()
        }
    }
}

fn selected_witnesses<C>(
    request: CheckerUnitView<'_, C>,
    resolution: &bray_bound_tree::BoundResolvedCall,
    evidence: &[ImplementationSelectionEvidence],
) -> Result<Option<Vec<SelectedImplementationWitness>>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut evidence = evidence.iter().collect::<Vec<_>>();

    evidence.sort_unstable_by_key(|item| item.requirement());

    if evidence
        .windows(2)
        .any(|pair| pair[0].requirement() == pair[1].requirement())
    {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    let mut selected = Vec::with_capacity(evidence.len());

    for evidence in evidence {
        let ImplementationSelection::Selected(witness) = evidence.selection() else {
            return Ok(None);
        };

        request
            .semantic_values()
            .implementation_instance_data(*witness)
            .map_err(|_| CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

        selected.push(SelectedImplementationWitness::new(
            evidence.requirement(),
            *witness,
        ));
    }

    let mut witness_ids = selected
        .iter()
        .map(|selection| selection.witness())
        .collect::<Vec<_>>();

    witness_ids.sort_unstable();
    witness_ids.dedup();

    if witness_ids != resolution.implementation_witnesses() {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    Ok(Some(selected))
}

fn callable_type<C>(
    request: CheckerUnitView<'_, C>,
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
    arguments: &[BoundArgument],
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
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    input: &CallableSelectionRequest,
) -> Result<CallableSelectionMode, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if types.unit() != request.view().unit() || types.kind() != request.view().kind() {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    let Some(bray_bound_tree::BoundExpression::Call(call)) =
        request.view().expression(input.expression())
    else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    if call.arguments() != input.arguments() {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    if call.generic_arguments() != input.generic_arguments() {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    let Some(callee) = request.view().expression(call.callee()) else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    callable_selection_mode(callee, input)
}

fn callable_selection_mode(
    callee: &bray_bound_tree::BoundExpression,
    input: &CallableSelectionRequest,
) -> Result<CallableSelectionMode, CheckerInfrastructureError> {
    let is_overload = match callee {
        bray_bound_tree::BoundExpression::MemberAccess(member) => {
            member_selection_is_overload(input, member.receiver())?
        }
        bray_bound_tree::BoundExpression::TraitQualifiedMember(member) => {
            member_selection_is_overload(input, member.receiver())?
        }
        bray_bound_tree::BoundExpression::Name(name) => {
            validate_non_member_request(input)?;

            matches!(
                name.target(),
                bray_bound_tree::BoundReferenceTarget::Surface(symbol)
                    if symbol.kind() == bray_symbols::SymbolKind::CallableOverload
            )
        }
        _ => {
            validate_non_member_request(input)?;

            false
        }
    };

    Ok(if is_overload {
        CallableSelectionMode::Overload
    } else {
        CallableSelectionMode::Direct
    })
}

fn member_selection_is_overload(
    input: &CallableSelectionRequest,
    receiver: bray_bound_tree::BoundExpressionId,
) -> Result<bool, CheckerInfrastructureError> {
    let Some(member) = input.callee_member() else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    if input.receiver().map(super::ReceiverSelection::expression) != Some(receiver) {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    Ok(member.member().kind() == bray_symbols::SymbolKind::CallableOverload)
}

fn validate_non_member_request(
    input: &CallableSelectionRequest,
) -> Result<(), CheckerInfrastructureError> {
    if input.callee_member().is_some() || input.receiver().is_some() {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundArgument, BoundCallExpression, BoundCallResult, BoundCallableTarget, BoundExpression,
        BoundExpressionId, BoundGenericArgument, BoundMemberAccessExpression, BoundMemberSelector,
        BoundResolvedCall, BoundUnit, BoundUnitId, CheckedExpressionTypes, ExpressionTypeResult,
        ExpressionTypeStatus, MemberTarget, SelectedArgument, SelectedCall,
    };
    use bray_symbols::{
        CallableAbi, CallableConstness, CallableDefinitionId, CallableDependencyContracts,
        CallableInstanceData, CallableParameterData, CallableParameterDefaultProviderSymbolId,
        CallableParameterMode, CallableParameterName, CallableParameterSignature,
        CallableParameterSymbolId, CallablePosition, CallableSignature, CallableTrust,
        CallableTypeData, DependencyContractTemplateData, FunctionSymbolId, GenericArgument,
        GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData,
        GenericTypeParameterSymbolId, SymbolId, SymbolKind, TypeData, TypeId,
    };

    use crate::test_support::{
        TestCheckerContext, callable_entry, checked_expression_types, declaration_key,
        expression_unit, push_expression, semantic_values, symbol_name, tuple_type,
    };
    use crate::{
        CallableCandidate, CallableCandidateState, CallableSelectionRequest, CandidateSelection,
        CheckerUnitView, DefaultSemanticSelector, ReceiverCapability, ReceiverSelection,
        SelectionFailure, SemanticSelector,
    };

    #[test]
    fn direct_calls_map_named_arguments_before_declaration_order_defaults() {
        let fixture = call_fixture(BoundUnitId::new(70), true);
        let candidate = callable_candidate(1, fixture.value_type, true);

        let input = named_request(&fixture, [candidate]);
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
        let fixture = overload_call_fixture(BoundUnitId::new(71), true);
        let candidate = callable_candidate(1, fixture.value_type, true);

        let input = named_request(&fixture, [candidate]);
        let result = select(&fixture.unit, &fixture.types, input);

        assert!(matches!(
            result.value(),
            CandidateSelection::Failed(SelectionFailure::Incompatible)
        ));
    }

    #[test]
    fn overload_selection_does_not_use_candidate_result_types() {
        let fixture = call_fixture(BoundUnitId::new(77), true);
        let other_result = tuple_type([fixture.value_type]);

        let first = callable_candidate_with_types(
            1,
            fixture.value_type,
            fixture.value_type,
            true,
            CallableCandidateState::Available,
        );

        let second = callable_candidate_with_types(
            2,
            fixture.value_type,
            other_result,
            true,
            CallableCandidateState::Available,
        );

        let input = named_request(&fixture, [first, second]);
        let result = select(&fixture.unit, &fixture.types, input);

        assert!(matches!(
            result.value(),
            CandidateSelection::Failed(SelectionFailure::Ambiguous(_))
        ));
    }

    #[test]
    fn positional_arguments_require_positional_parameter_permission() {
        let fixture = call_fixture(BoundUnitId::new(72), false);
        let candidate = callable_candidate(1, fixture.value_type, false);

        let input = CallableSelectionRequest::new(
            fixture.call,
            None,
            None,
            [],
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
    fn explicit_generic_argument_arity_matches_the_selected_substitution() {
        let fixture = call_fixture(BoundUnitId::new(79), false);
        let generic_argument = BoundGenericArgument::new(fixture.unit.key().source().syntax());

        let target =
            BoundCallableTarget::Declaration(generic_callable_instance(5, fixture.value_type));

        let context = TestCheckerContext::new(false);
        let entry = callable_entry(fixture.unit.key());

        let request = match CheckerUnitView::new(&fixture.unit, &entry, &context) {
            Ok(request) => request,
            Err(error) => panic!("generic call selection request must validate: {error:?}"),
        };

        assert!(matches!(
            super::generic_arguments_are_compatible(request, &[generic_argument], target),
            Ok(super::Compatibility::Yes)
        ));

        assert!(matches!(
            super::generic_arguments_are_compatible(request, &[], target),
            Ok(super::Compatibility::No)
        ));
    }

    #[test]
    fn method_requests_require_the_bound_callee_receiver() {
        let fixture = method_call_fixture(BoundUnitId::new(78));
        let context = TestCheckerContext::new(false);
        let entry = callable_entry(fixture.unit.key());

        let request = match CheckerUnitView::new(&fixture.unit, &entry, &context) {
            Ok(request) => request,
            Err(error) => panic!("method selection request must validate: {error:?}"),
        };

        let member = bray_symbols::FunctionSymbolId::from_symbol_id(SymbolId::new(4));

        let input = CallableSelectionRequest::new(
            fixture.call,
            Some(MemberTarget::new(member.into(), fixture.value_type, [])),
            Some(ReceiverSelection::new(
                fixture.unrelated_receiver,
                ReceiverCapability::Shared,
            )),
            [],
            [],
            [],
        );

        assert_eq!(
            super::select(request, &fixture.types, input),
            Err(crate::CheckerInfrastructureError::InvalidSemanticSelectionInput)
        );
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

        let input = named_request(&fixture, [applicable]);
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

        let input = named_request(&fixture, [incompatible]);
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

        let input = named_request(&fixture, [candidate]);
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

        let input = named_request(&fixture, [candidate]);
        let result = select(&fixture.unit, &fixture.types, input);

        assert!(matches!(
            result.value(),
            CandidateSelection::Failed(SelectionFailure::Recovered)
        ));

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn cancellation_publishes_no_selection_or_diagnostics() {
        let fixture = call_fixture(BoundUnitId::new(76), true);
        let candidate = callable_candidate(1, fixture.value_type, true);
        let input = named_request(&fixture, [candidate]);
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

    struct MethodCallFixture {
        unit: BoundUnit,
        types: CheckedExpressionTypes,
        call: BoundExpressionId,
        unrelated_receiver: BoundExpressionId,
        value_type: TypeId,
    }

    fn named_request(
        fixture: &CallFixture,
        candidates: impl IntoIterator<Item = CallableCandidate>,
    ) -> CallableSelectionRequest {
        CallableSelectionRequest::new(
            fixture.call,
            None,
            None,
            [],
            [BoundArgument::new(
                fixture.argument,
                Some(symbol_name("second")),
                false,
            )],
            candidates,
        )
    }

    fn call_fixture(unit: BoundUnitId, named_argument: bool) -> CallFixture {
        call_fixture_with_kind(unit, named_argument, SymbolKind::Function)
    }

    fn overload_call_fixture(unit: BoundUnitId, named_argument: bool) -> CallFixture {
        call_fixture_with_kind(unit, named_argument, SymbolKind::CallableOverload)
    }

    fn call_fixture_with_kind(
        unit: BoundUnitId,
        named_argument: bool,
        callee_kind: SymbolKind,
    ) -> CallFixture {
        let value_type = tuple_type([]);

        let (unit, expressions) = expression_unit(unit, |tree, origin| {
            let callee = push_expression(tree, callable_name_expression(origin, callee_kind));

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
                    [],
                    [BoundArgument::new(argument, name, false)],
                )),
            );

            vec![callee, argument, call]
        });

        let result = ExpressionTypeResult::new(value_type, ExpressionTypeStatus::Valid);
        let types = checked_expression_types(&unit, expressions.iter().copied(), result);

        CallFixture {
            call: expressions[2],
            argument: expressions[1],
            unit,
            types,
            value_type,
        }
    }

    fn method_call_fixture(unit: BoundUnitId) -> MethodCallFixture {
        let value_type = tuple_type([]);

        let (unit, expressions) = expression_unit(unit, |tree, origin| {
            let receiver = push_expression(
                tree,
                crate::test_support::integer_literal_expression(origin, Some(value_type)),
            );

            let unrelated_receiver = push_expression(
                tree,
                crate::test_support::integer_literal_expression(origin, Some(value_type)),
            );

            let callee = push_expression(
                tree,
                BoundExpression::MemberAccess(BoundMemberAccessExpression::new(
                    origin,
                    receiver,
                    Some(BoundMemberSelector::Name(symbol_name("method"))),
                    None,
                    false,
                )),
            );

            let call = push_expression(
                tree,
                BoundExpression::Call(BoundCallExpression::pending(origin, callee, [], [])),
            );

            vec![receiver, unrelated_receiver, callee, call]
        });

        let result = ExpressionTypeResult::new(value_type, ExpressionTypeStatus::Valid);
        let types = checked_expression_types(&unit, expressions.iter().copied(), result);

        MethodCallFixture {
            call: expressions[3],
            unrelated_receiver: expressions[1],
            unit,
            types,
            value_type,
        }
    }

    fn callable_name_expression(
        origin: bray_bound_tree::BoundNodeOrigin,
        kind: SymbolKind,
    ) -> BoundExpression {
        let target = match kind {
            SymbolKind::CallableOverload => {
                bray_symbols::CallableOverloadSymbolId::from_symbol_id(SymbolId::new(0)).into()
            }
            _ => bray_symbols::FunctionSymbolId::from_symbol_id(SymbolId::new(0)).into(),
        };

        BoundExpression::Name(bray_bound_tree::BoundNameExpression::new(
            origin,
            bray_bound_tree::BoundReferenceTarget::Surface(target),
            None,
            false,
        ))
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
    ) -> bray_diagnostics::DiagnosticResult<CandidateSelection<SelectedCall>> {
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
    ) -> crate::CheckerOutcome<CandidateSelection<SelectedCall>> {
        let entry = callable_entry(unit.key());

        let request = match CheckerUnitView::new(unit, &entry, context) {
            Ok(request) => request,
            Err(error) => panic!("call selection request must be valid: {error:?}"),
        };

        DefaultSemanticSelector.select_callable(request, types, input)
    }

    fn parameter(index: u32) -> CallableParameterSymbolId {
        CallableParameterSymbolId::from_symbol_id(SymbolId::new(index))
    }

    fn generic_callable_instance(declaration: u32, argument: TypeId) -> CallableInstanceData {
        let function = FunctionSymbolId::from_symbol_id(SymbolId::new(declaration));

        let Some(owner) = GenericOwnerId::try_new(function.into()) else {
            panic!("function must support generic substitutions");
        };

        let parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(100));

        let substitution = GenericSubstitutionData::try_new(
            owner,
            [GenericParameterSymbolId::from(parameter)],
            [GenericArgument::Type(argument)],
        );

        let substitution = match substitution {
            Ok(substitution) => substitution,
            Err(error) => panic!("generic callable substitution must validate: {error:?}"),
        };

        let substitution = match semantic_values().intern_generic_substitution(substitution) {
            Ok(substitution) => substitution,
            Err(error) => panic!("generic callable substitution must be interned: {error:?}"),
        };

        let Some(definition) = CallableDefinitionId::try_new(function.into()) else {
            panic!("function must be a callable definition");
        };

        CallableInstanceData::new(definition, substitution)
    }

    fn default_provider(index: u32) -> CallableParameterDefaultProviderSymbolId {
        CallableParameterDefaultProviderSymbolId::from_symbol_id(SymbolId::new(10 + index))
    }
}
