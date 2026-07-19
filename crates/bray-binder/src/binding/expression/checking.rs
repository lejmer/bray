use bray_bound_tree::{
    AnyBoundNodeId, BoundCallResult, BoundCallableTarget, BoundExpression, BoundResolvedCall,
    BoundUnit, BoundUnitRoot, BoundWalkControl, BoundWalkEvent, BoundWalkOutcome,
    walk_bound_unit_view,
};
use bray_checker::{
    CallableCandidate, CallableCandidateState, CallableSelectionRequest, ExpressionTypeEvidence,
    ExpressionTypeInput, SemanticSelectionInput,
};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    CallableDefinitionId, CallableExecution, CallableInstanceData, CallableSignatureFact,
    CallableSymbolId, GenericArgument, GenericOwnerId, GenericParameterSymbolId,
    GenericSubstitutionData, NamedTypeSymbolId, StructSymbolId, SymbolFactRequest, TypeData,
};

use crate::{BinderFactContext, BinderFactError, BinderFactResult, SymbolFactProvider};

/// Binder-owned inputs for expression typing and semantic selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundExpressionCheckInput {
    types: ExpressionTypeInput,
    selections: SemanticSelectionInput,
}

impl BoundExpressionCheckInput {
    /// Returns expression evidence and contextual expectations established by binding.
    pub const fn types(&self) -> &ExpressionTypeInput {
        &self.types
    }

    /// Returns binder-enumerated semantic candidate requests for one checker evaluation.
    pub fn selections(&self) -> SemanticSelectionInput {
        // Selection consumes and sorts candidate collections while this cached input stays shared.
        self.selections.clone()
    }
}

/// Binds checker inputs that depend on exact resolved names and declaration surfaces.
pub fn bind_expression_check_input<C>(
    facts: &C,
    unit: &BoundUnit,
    available: &bray_symbols::AvailableCompilerKnownSymbols,
) -> BinderFactResult<BoundExpressionCheckInput>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>,
{
    let mut evidence = Vec::new();
    let mut calls = Vec::new();
    let mut failure = None;

    let outcome = walk_bound_unit_view(unit.view(), root_node(unit.root()), |event| {
        if facts.is_cancelled() {
            failure = Some(BinderFactError::Cancelled);

            return BoundWalkControl::Stop;
        }

        let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression_id)) = event else {
            return BoundWalkControl::Continue;
        };

        let Some(expression) = unit.view().expression(expression_id) else {
            failure = Some(BinderFactError::DependencyUnavailable);

            return BoundWalkControl::Stop;
        };

        match expression {
            BoundExpression::Name(name) => {
                let Some(callable) = surface_callable(name.target()) else {
                    return BoundWalkControl::Continue;
                };

                match callable_signature(facts, callable) {
                    Ok(signature) => evidence.push(ExpressionTypeEvidence::new(
                        expression_id,
                        signature.callable_type(),
                    )),
                    Err(error) => {
                        failure = Some(error);

                        return BoundWalkControl::Stop;
                    }
                }
            }
            BoundExpression::Call(call) => match bind_direct_call(facts, unit, available, call) {
                Ok(Some(candidate)) => {
                    evidence.push(ExpressionTypeEvidence::new(
                        expression_id,
                        candidate.resolution().result().ty(),
                    ));
                    calls.push(CallableSelectionRequest::new(
                        expression_id,
                        None,
                        None,
                        // Selection input must own arguments after this borrowed unit walk ends.
                        call.arguments().iter().cloned(),
                        [candidate],
                    ));
                }
                Ok(None) => {}
                Err(error) => {
                    failure = Some(error);

                    return BoundWalkControl::Stop;
                }
            },
            _ => {}
        }

        BoundWalkControl::Continue
    });

    if let Some(error) = failure {
        return Err(error);
    }

    if outcome != BoundWalkOutcome::Completed {
        return Err(BinderFactError::DependencyUnavailable);
    }

    Ok(BoundExpressionCheckInput {
        types: ExpressionTypeInput::new().with_evidence(evidence),
        selections: SemanticSelectionInput::new().with_calls(calls),
    })
}

fn bind_direct_call<C>(
    facts: &C,
    unit: &BoundUnit,
    available: &bray_symbols::AvailableCompilerKnownSymbols,
    call: &bray_bound_tree::BoundCallExpression,
) -> BinderFactResult<Option<CallableCandidate>>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>,
{
    let Some(BoundExpression::Name(callee)) = unit.view().expression(call.callee()) else {
        return Ok(None);
    };

    let Some(callable) = surface_callable(callee.target()) else {
        return Ok(None);
    };

    let symbol = callable.into_any();
    let signature = callable_signature(facts, callable)?;
    let Some(instance) = callable_instance(facts, symbol)? else {
        return Ok(None);
    };

    let result = call_result(facts, available, &signature)?;
    let resolution = BoundResolvedCall::new(BoundCallableTarget::Declaration(instance), [], result);
    // Symbol keys are Arc-backed identities retained independently by candidate input.
    let key = facts
        .symbols()
        .symbol_key(symbol)
        .cloned()
        .ok_or(BinderFactError::DependencyUnavailable)?;
    let state = if call.is_recovered()
        || callee.is_recovered()
        || facts.symbols().symbol_is_recovered(symbol).unwrap_or(true)
    {
        CallableCandidateState::Recovered
    } else {
        CallableCandidateState::Available
    };
    let defaults = signature
        .parameters()
        .iter()
        .filter_map(|parameter| {
            facts
                .symbols()
                .callable_parameter(parameter.parameter())
                .and_then(|record| record.default_provider())
                .map(|provider| (parameter.parameter(), provider))
        })
        .collect::<Vec<_>>();

    Ok(Some(CallableCandidate::new(
        key, resolution, signature, defaults, state,
    )))
}

fn callable_signature<C>(
    facts: &C,
    callable: CallableSymbolId,
) -> BinderFactResult<bray_symbols::CallableSignature>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>,
{
    facts
        .symbol_facts()
        .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(callable))
        // The fact result is cache-owned while candidate input retains its Arc-backed signature.
        .map(|result| result.value().clone())
}

fn callable_instance<C>(
    facts: &C,
    symbol: bray_symbols::AnySymbolId,
) -> BinderFactResult<Option<CallableInstanceData>>
where
    C: BinderFactContext + ?Sized,
{
    let Some(definition) = CallableDefinitionId::try_new(symbol) else {
        return Err(BinderFactError::DependencyUnavailable);
    };
    let Some(owner) = GenericOwnerId::try_new(symbol) else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    let has_parameters = facts
        .symbols()
        .generic_type_parameters()
        .iter()
        .any(|parameter| parameter.owner() == owner)
        || facts
            .symbols()
            .generic_const_parameters()
            .iter()
            .any(|parameter| parameter.owner() == owner);

    if has_parameters {
        return Ok(None);
    }

    let substitution = GenericSubstitutionData::try_new(owner, [], [])
        .map_err(|_| BinderFactError::DependencyUnavailable)?;
    let substitution = facts
        .semantic_values()
        .intern_generic_substitution(substitution)
        .map_err(|_| BinderFactError::DependencyUnavailable)?;

    Ok(Some(CallableInstanceData::new(definition, substitution)))
}

fn call_result<C>(
    facts: &C,
    available: &bray_symbols::AvailableCompilerKnownSymbols,
    signature: &bray_symbols::CallableSignature,
) -> BinderFactResult<BoundCallResult>
where
    C: BinderFactContext + ?Sized,
{
    let callable = facts
        .semantic_values()
        .type_data(signature.callable_type())
        .map_err(|_| BinderFactError::DependencyUnavailable)?;
    let TypeData::Callable(callable) = callable.as_ref() else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    match callable.execution() {
        CallableExecution::Synchronous => Ok(BoundCallResult::Immediate(signature.result())),
        CallableExecution::Asynchronous => {
            let future = future_type(facts, available, signature.result())?;

            Ok(BoundCallResult::LazyFuture(
                bray_bound_tree::BoundFutureConstruction::new(signature.result(), future),
            ))
        }
    }
}

fn future_type<C>(
    facts: &C,
    available: &bray_symbols::AvailableCompilerKnownSymbols,
    completion: bray_symbols::TypeId,
) -> BinderFactResult<bray_symbols::TypeId>
where
    C: BinderFactContext + ?Sized,
{
    let definition = available
        .representation_symbol::<StructSymbolId>(RepresentationRole::Future)
        .ok_or(BinderFactError::DependencyUnavailable)?;
    let named = NamedTypeSymbolId::Struct(definition);
    let owner =
        GenericOwnerId::try_new(named.into_any()).ok_or(BinderFactError::DependencyUnavailable)?;
    let parameters = facts
        .symbols()
        .structure(definition)
        .map(|record| record.generic_type_parameters())
        .ok_or(BinderFactError::DependencyUnavailable)?;
    let [parameter] = parameters else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    let substitution = GenericSubstitutionData::try_new(
        owner,
        [GenericParameterSymbolId::from(*parameter)],
        [GenericArgument::Type(completion)],
    )
    .map_err(|_| BinderFactError::DependencyUnavailable)?;
    let substitution = facts
        .semantic_values()
        .intern_generic_substitution(substitution)
        .map_err(|_| BinderFactError::DependencyUnavailable)?;

    facts
        .semantic_values()
        .intern_type(TypeData::Named {
            definition: named,
            substitution,
        })
        .map_err(|_| BinderFactError::DependencyUnavailable)
}

fn surface_callable(target: bray_bound_tree::BoundReferenceTarget) -> Option<CallableSymbolId> {
    let bray_bound_tree::BoundReferenceTarget::Surface(symbol) = target else {
        return None;
    };

    CallableSymbolId::try_from_any(symbol)
}

const fn root_node(root: BoundUnitRoot) -> AnyBoundNodeId {
    match root {
        BoundUnitRoot::CallableBody(body) | BoundUnitRoot::AnonymousCallable { body, .. } => {
            AnyBoundNodeId::CallableBody(body)
        }
        BoundUnitRoot::Expression(expression) => AnyBoundNodeId::Expression(expression),
        BoundUnitRoot::ExpressionSequence(block) => AnyBoundNodeId::Block(block),
    }
}
