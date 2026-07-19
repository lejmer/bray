use std::collections::BTreeSet;

use bray_bound_tree::{CheckedExpressionTypes, SelectedArgument, SelectedCall};
use bray_symbols::{
    CallableParameterDefaultProviderSymbolId, CallableParameterSignature,
    CallableParameterSymbolId, CallableTypeData, SymbolKey, TypeData,
};

use crate::{CheckerInfrastructureError, CheckerRequestContext, UnitCheckRequest};

use super::super::model::map_explicit_argument_indices;
use super::super::{
    CallableCandidate, CallableCandidateParts, CallableCandidateState, CallableSelectionRequest,
    CandidateSelection, SelectionFailure,
};
use super::validation::{
    CallableSelectionMode, Compatibility, callable_surface_is_consistent, callable_type,
    expression_type, identity_conversion, receiver_is_compatible, selected_receiver,
    selected_witnesses, validate_unit,
};

pub(in crate::selection) fn select<C>(
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

    let mode = validate_unit(request, types, &input)?;

    let CallableSelectionRequest {
        expression: _,
        callee_member: _,
        receiver,
        arguments,
        mut candidates,
    } = input;

    if !super::super::order::canonicalize_by_key(&mut candidates, CallableCandidate::key) {
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

        let checked = check_candidate(request, types, mode, receiver, &arguments, candidate)?;

        match (checked.compatibility, checked.applicable) {
            (CandidateCompatibility::Applicable, Some(_))
                if state == CallableCandidateState::Inaccessible =>
            {
                has_inaccessible = true;
            }
            (CandidateCompatibility::Applicable, Some(applicable_candidate)) => {
                applicable.push(applicable_candidate)
            }
            (CandidateCompatibility::Incompatible, None) => has_incompatible = true,
            (CandidateCompatibility::Recovered, None) => has_recovered = true,
            _ => return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput),
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

struct CandidateCheck {
    compatibility: CandidateCompatibility,
    applicable: Option<(SymbolKey, SelectedCall)>,
}

enum CandidateCompatibility {
    Applicable,
    Incompatible,
    Recovered,
}

fn check_candidate<C>(
    request: UnitCheckRequest<'_, C>,
    types: &CheckedExpressionTypes,
    mode: CallableSelectionMode,
    receiver: Option<super::super::ReceiverSelection>,
    arguments: &[bray_bound_tree::BoundArgument],
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

    match receiver_is_compatible(types, receiver, signature.receiver())? {
        Compatibility::No => return Ok(CandidateCheck::incompatible()),
        Compatibility::Recovered => return Ok(CandidateCheck::recovered()),
        Compatibility::Yes => {}
    }

    let receiver = selected_receiver(types, receiver, signature.receiver())?;

    let Some(arguments) = map_arguments(
        types,
        mode,
        arguments,
        callable_type,
        signature.parameters(),
        &defaults,
    )?
    else {
        return Ok(CandidateCheck::incompatible());
    };

    if arguments.recovered {
        return Ok(CandidateCheck::recovered());
    }

    let Some(witnesses) = selected_witnesses(request, &resolution, &implementation_selections)?
    else {
        return Ok(CandidateCheck::incompatible());
    };

    Ok(CandidateCheck::applicable(
        key,
        SelectedCall::new(
            resolution,
            callable_type.abi(),
            receiver,
            arguments.values,
            witnesses,
        ),
    ))
}

impl CandidateCheck {
    const fn applicable(key: SymbolKey, call: SelectedCall) -> Self {
        Self {
            compatibility: CandidateCompatibility::Applicable,
            applicable: Some((key, call)),
        }
    }

    const fn incompatible() -> Self {
        Self {
            compatibility: CandidateCompatibility::Incompatible,
            applicable: None,
        }
    }

    const fn recovered() -> Self {
        Self {
            compatibility: CandidateCompatibility::Recovered,
            applicable: None,
        }
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
    let Some(argument_indices) = map_explicit_argument_indices(arguments, parameters) else {
        return Ok(None);
    };

    let mut supplied = vec![None; parameters.len()];
    let mut values = Vec::with_capacity(parameters.len());
    let mut recovered = false;

    for (argument, parameter_index) in arguments.iter().zip(argument_indices) {
        let actual = expression_type(types, argument.expression())?;

        recovered |= argument.is_recovered() || actual.is_recovered();

        if !actual.is_recovered() && actual.ty() != parameters[parameter_index].ty() {
            return Ok(None);
        }

        supplied[parameter_index] = Some(argument.expression());
        values.push(SelectedArgument::Explicit {
            expression: argument.expression(),
            parameter: signatures[parameter_index].parameter(),
            conversion: identity_conversion(actual.ty(), parameters[parameter_index].ty()),
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

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundArgument, BoundCallExpression, BoundCallResult, BoundCallableTarget, BoundExpression,
        BoundExpressionId, BoundMemberAccessExpression, BoundMemberSelector, BoundResolvedCall,
        BoundUnit, BoundUnitId, CheckedExpressionTypes, ConversionTarget, ExpressionTypeResult,
        ExpressionTypeStatus, MemberTarget, SelectedArgument, SelectedCall, SelectedConversion,
    };
    use bray_symbols::{
        CallableAbi, CallableConstness, CallableDependencyContracts, CallableParameterData,
        CallableParameterDefaultProviderSymbolId, CallableParameterMode, CallableParameterName,
        CallableParameterSignature, CallableParameterSymbolId, CallablePosition, CallableSignature,
        CallableTrust, CallableTypeData, DependencyContractTemplateData, ReceiverMode,
        ReceiverParameterSignature, ReceiverParameterSymbolId, SymbolId, SymbolKind, TypeData,
        TypeId,
    };

    use crate::test_support::{
        TestCheckerContext, callable_entry, checked_expression_types, declaration_key,
        expression_unit, push_expression, semantic_values, symbol_name, tuple_type,
    };
    use crate::{
        CallableCandidate, CallableCandidateState, CallableSelectionRequest, CandidateSelection,
        DefaultSemanticSelector, ReceiverCapability, ReceiverSelection, SelectionFailure,
        SemanticSelector, UnitCheckRequest,
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
                    conversion: SelectedConversion::new(
                        fixture.value_type,
                        fixture.value_type,
                        ConversionTarget::Identity,
                    ),
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
    fn method_requests_require_the_bound_callee_receiver() {
        let fixture = method_call_fixture(BoundUnitId::new(78));
        let context = TestCheckerContext::new(false);
        let entry = callable_entry(fixture.unit.key());

        let request = match UnitCheckRequest::new(&fixture.unit, &entry, &context) {
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
        );

        assert_eq!(
            super::select(request, &fixture.types, input),
            Err(crate::CheckerInfrastructureError::InvalidSemanticSelectionInput)
        );
    }

    #[test]
    fn method_calls_retain_the_exact_receiver_parameter_and_conversion() {
        let fixture = method_call_fixture(BoundUnitId::new(79));
        let receiver_parameter = ReceiverParameterSymbolId::from_symbol_id(SymbolId::new(9));
        let member = bray_symbols::FunctionSymbolId::from_symbol_id(SymbolId::new(4));
        let candidate = method_candidate(1, fixture.value_type, receiver_parameter);

        let input = CallableSelectionRequest::new(
            fixture.call,
            Some(MemberTarget::new(member.into(), fixture.value_type, [])),
            Some(ReceiverSelection::new(
                fixture.receiver,
                ReceiverCapability::Shared,
            )),
            [],
            [candidate],
        );
        let result = select(&fixture.unit, &fixture.types, input);

        let CandidateSelection::Selected(call) = result.value() else {
            panic!("one applicable method must be selected");
        };
        let Some(receiver) = call.receiver() else {
            panic!("selected methods must retain their receiver mapping");
        };

        assert_eq!(receiver.expression(), fixture.receiver);
        assert_eq!(receiver.parameter(), receiver_parameter);
        assert_eq!(receiver.conversion().source_type(), fixture.value_type);
        assert_eq!(receiver.conversion().target_type(), fixture.value_type);
        assert_eq!(receiver.conversion().target(), &ConversionTarget::Identity);
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
        receiver: BoundExpressionId,
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
                BoundExpression::Call(BoundCallExpression::pending(origin, callee, [])),
            );

            vec![receiver, unrelated_receiver, callee, call]
        });

        let result = ExpressionTypeResult::new(value_type, ExpressionTypeStatus::Valid);
        let types = checked_expression_types(&unit, expressions.iter().copied(), result);

        MethodCallFixture {
            call: expressions[3],
            receiver: expressions[0],
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

        let callable_type = callable_type(parameters, result_type);

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

    fn method_candidate(
        declaration: u32,
        value_type: TypeId,
        receiver: ReceiverParameterSymbolId,
    ) -> CallableCandidate {
        let callable_type = callable_type([], value_type);
        let signature = CallableSignature::new(
            callable_type,
            Some(ReceiverParameterSignature::new(
                receiver,
                value_type,
                ReceiverMode::Shared,
            )),
            [],
            value_type,
        );
        let resolution = BoundResolvedCall::new(
            BoundCallableTarget::Indirect(callable_type),
            [],
            BoundCallResult::Immediate(value_type),
        );

        CallableCandidate::new(
            declaration_key(SymbolKind::Function, declaration),
            resolution,
            signature,
            [],
            CallableCandidateState::Available,
        )
    }

    fn callable_type(
        parameters: impl IntoIterator<Item = CallableParameterData>,
        result: TypeId,
    ) -> TypeId {
        let dependency = match semantic_values()
            .intern_dependency_contract_template(DependencyContractTemplateData::new([]))
        {
            Ok(dependency) => dependency,
            Err(error) => panic!("empty dependency contract must be valid: {error:?}"),
        };

        let callable = CallableTypeData::new(
            parameters,
            result,
            CallableConstness::Runtime,
            CallableTrust::Safe,
            CallableAbi::C,
            CallableDependencyContracts::synchronous(dependency),
        );

        match semantic_values().intern_type(TypeData::Callable(callable)) {
            Ok(callable) => callable,
            Err(error) => panic!("callable test type must be valid: {error:?}"),
        }
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
