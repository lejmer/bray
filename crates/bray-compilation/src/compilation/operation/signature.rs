use bray_symbols::CallableConditions;

use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, CallableParameterData, CallableSignature, CallableTypeData,
    ImplementationInstanceId, ReceiverParameterSignature, SelfTypeContext, TypeData,
    TypeExpressionTemplate, TypeId,
};

use super::super::binder::CompilationBindingContext;
use super::super::checker::CompilationCheckerContext;
use super::query::symbol_contract_failure;
use crate::compilation::{SemanticDataKind, SemanticQueryViolation};
use crate::fact::FactQueryError;

pub(in crate::compilation) fn selected_callable_signature(
    binding: &CompilationBindingContext<'_>,
    instance: bray_symbols::CallableInstanceData,
) -> Result<DiagnosticResult<Option<CallableSignature>>, FactQueryError> {
    let signature = binding
        .resolve_symbol_query(bray_symbols::SymbolQueryRequest::<
            bray_symbols::CallableSignatureQuery,
        >::new(instance.definition().callable_symbol()))
        .map_err(super::super::binder::binding_query_error)?;

    let constants = binding
        .compilation()
        .checked_constant_terms_for_templates_with_cancellation(
            [
                signature.value().callable_type(),
                signature.value().result(),
            ],
            binding.cancellation(),
        )?;

    let diagnostics = DiagnosticBag::merged_all([signature.diagnostics(), constants.diagnostics()]);

    let signature = bray_checker::resolve_callable_signature_template(
        binding.semantic_values(),
        signature.value(),
        instance.substitution(),
        constants.value(),
    )
    .map_err(FactQueryError::from)?;

    Ok(DiagnosticResult::new(signature, diagnostics))
}

pub(super) fn associated_member_substitution(
    binding: &CompilationBindingContext<'_>,
    surface: &bray_symbols::TypeAssociatedSurface,
    origin: bray_symbols::TypeAssociatedMemberOrigin,
    subject: TypeId,
    direct: bray_symbols::GenericSubstitutionId,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<bray_symbols::GenericSubstitutionId>, FactQueryError> {
    let bray_symbols::TypeAssociatedMemberOrigin::InherentImplementation(implementation) = origin
    else {
        return Ok(Some(direct));
    };

    let contribution = surface
        .implementations()
        .iter()
        .find(|candidate| candidate.implementation() == implementation)
        .ok_or_else(|| {
            symbol_contract_failure(
                implementation.into(),
                SemanticQueryViolation::Missing(SemanticDataKind::ImplementationUsing),
            )
        })?;

    let pattern = binding.compilation().resolve_implementation_self_type(
        binding,
        implementation.into(),
        diagnostics,
    )?;

    super::super::implementation::match_implementation_subject(
        implementation.into(),
        contribution.generic().parameters(),
        pattern,
        subject,
        binding.semantic_values(),
    )
    .map_err(|error| {
        super::super::implementation::implementation_match_query_error(implementation.into(), error)
    })
}

pub(super) fn operation_callable_type(
    values: &bray_symbols::SemanticValueStore,
    template: &TypeExpressionTemplate,
    parameters: &[TypeId],
    result: TypeId,
    member: AnySymbolId,
) -> Result<TypeId, FactQueryError> {
    let (
        parameter_surface,
        constness,
        trust,
        abi,
        dependencies,
        phase_behaviors,
        conditions,
        variadic,
    ) = match template {
        TypeExpressionTemplate::Callable(callable) => {
            let parameters = callable
                .parameters()
                .iter()
                .map(|parameter| {
                    (
                        parameter.name().clone(),
                        parameter.position(),
                        parameter.mode(),
                    )
                })
                .collect::<Vec<_>>();

            (
                parameters,
                callable.constness(),
                callable.trust(),
                callable.abi(),
                callable.dependencies(),
                callable.phase_behaviors().clone(),
                // Rebuilt types retain the immutable declaration conditions.
                callable.conditions().clone(),
                callable.is_variadic(),
            )
        }
        TypeExpressionTemplate::Resolved(ty) => {
            let data = values
                .type_data(*ty)
                .map_err(FactQueryError::SemanticValueStore)?;

            let TypeData::Callable(callable) = data.as_ref() else {
                return Err(symbol_contract_failure(
                    member,
                    SemanticQueryViolation::Unsupported(SemanticDataKind::CallableSignature),
                ));
            };

            let parameters = callable
                .parameters()
                .iter()
                .map(|parameter| {
                    (
                        parameter.name().clone(),
                        parameter.position(),
                        parameter.mode(),
                    )
                })
                .collect::<Vec<_>>();

            (
                parameters,
                callable.constness(),
                callable.trust(),
                callable.abi(),
                callable.dependency_contracts(),
                callable.phase_behaviors().clone(),
                // Rebuilt types retain the immutable declaration conditions.
                callable.conditions().clone(),
                callable.is_variadic(),
            )
        }
        _ => {
            return Err(symbol_contract_failure(
                member,
                SemanticQueryViolation::Unsupported(SemanticDataKind::CallableSignature),
            ));
        }
    };

    if parameter_surface.len() != parameters.len() {
        return Err(symbol_contract_failure(
            member,
            SemanticQueryViolation::CountMismatch {
                data: SemanticDataKind::CallableSignature,
                expected: parameter_surface.len(),
                actual: parameters.len(),
            },
        ));
    }

    let parameters = parameter_surface
        .into_iter()
        .zip(parameters.iter().copied())
        .map(|((name, position, mode), ty)| CallableParameterData::new(name, position, mode, ty));

    let callable = CallableTypeData::new(parameters, result, constness, trust, abi, dependencies)
        .with_phase_behaviors(phase_behaviors)
        .with_conditions(conditions)
        .with_variadic(variadic);

    values
        .intern_type(TypeData::Callable(callable))
        .map_err(FactQueryError::SemanticValueStore)
}

pub(super) fn member_callable_signature(
    signature: CallableSignature,
    receiver_type: TypeId,
) -> CallableSignature {
    let receiver = signature.receiver().map(|receiver| {
        ReceiverParameterSignature::new(receiver.parameter(), receiver_type, receiver.mode())
    });

    CallableSignature::new(
        signature.callable_type(),
        receiver,
        signature.parameters().iter().copied(),
        signature.result(),
    )
}

pub(super) fn normalize_callable_type_equalities(
    binding_context: &CompilationBindingContext<'_>,
    signature: CallableSignature,
    constraints: &[(
        bray_symbols::GenericOwnerId,
        bray_symbols::CheckedConstraint,
    )],
) -> Result<CallableSignature, FactQueryError> {
    let values = binding_context.semantic_values();

    signature
        .try_map_types(|ty| super::constraint::normalize_type_equalities(values, ty, constraints))
}

pub(super) fn normalize_callable_type_valued_members(
    binding_context: &CompilationBindingContext<'_>,
    signature: CallableSignature,
    witness: ImplementationInstanceId,
    diagnostics: &mut DiagnosticBag,
) -> Result<CallableSignature, FactQueryError> {
    let checker =
        CompilationCheckerContext::new(*binding_context).with_implementation_witnesses([witness]);

    bray_checker::normalize_callable_signature_type_valued_members(&checker, signature, diagnostics)
        .map_err(FactQueryError::from)
}

pub(super) fn substitute_callable_self(
    binding_context: &CompilationBindingContext<'_>,
    signature: CallableSignature,
    context: SelfTypeContext,
    replacement: TypeId,
) -> Result<CallableSignature, FactQueryError> {
    let values = binding_context.semantic_values();

    signature.try_map_types(|ty| {
        values
            .substitute_contextual_self(ty, context, replacement)
            .map_err(FactQueryError::SemanticValueStore)
    })
}
