use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_checker::resolve_callable_signature_template;
use bray_compiler_known::{CompilerKnownDeclarationKey, CompilerKnownOperationRole};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    CallableInstanceData, CallableSignature, CallableSignatureQuery, GenericArgument,
    GenericParameterSymbolId, ImplementationInstanceId, ImplementationRequirementKey,
    ImplementationSelection, SymbolQueryRequest, TraitCallableMemberSymbolId, TypeId,
};

use super::super::Compilation;
use super::super::binder::{CompilationBindingContext, binding_query_error};
use super::super::implementation::{
    implementation_callable_instance, implementation_fulfillments, implementation_requirement,
};
use crate::fact::{CancellationToken, FactQueryError};

type SelectedStorageCallable = (
    ImplementationRequirementKey,
    ImplementationInstanceId,
    CallableInstanceData,
    CallableSignature,
);

pub(in crate::compilation) fn selected_storage_callable(
    compilation: &Compilation,
    binding_context: &CompilationBindingContext<'_>,
    storage: TypeId,
    target: TypeId,
    member_key: &CompilerKnownDeclarationKey,
    cancellation: &CancellationToken,
) -> Result<DiagnosticResult<Option<SelectedStorageCallable>>, FactQueryError> {
    let contract = compilation
        .available_compiler_known_symbols()
        .operation_contract(CompilerKnownOperationRole::BoxConstruction)
        .ok_or(FactQueryError::InfrastructureFailure)?;

    let trait_symbol = binding_context
        .symbols()
        .trait_symbol(contract.trait_definition())
        .ok_or(FactQueryError::InfrastructureFailure)?;

    let parameters = trait_symbol
        .generic_type_parameters()
        .iter()
        .copied()
        .map(GenericParameterSymbolId::Type);

    let requirement = implementation_requirement(
        binding_context.semantic_values(),
        storage,
        contract.trait_definition(),
        parameters,
        [GenericArgument::Type(target)],
    )?;

    let selected =
        compilation.implementation_selection_result_with_cancellation(requirement, cancellation)?;

    let mut diagnostics = DiagnosticBag::new().merged(selected.diagnostics());

    let ImplementationSelection::Selected(witness) = selected.value() else {
        return Ok(DiagnosticResult::new(None, diagnostics));
    };

    let implementation = binding_context
        .semantic_values()
        .implementation_instance_data(*witness)
        .map_err(FactQueryError::SemanticValueStore)?;

    let fulfillments = implementation_fulfillments(binding_context, implementation.definition())?;

    let member = compilation
        .available_compiler_known_symbols()
        .declaration_symbol::<TraitCallableMemberSymbolId>(member_key)
        .ok_or(FactQueryError::InfrastructureFailure)?;

    let application = binding_context
        .semantic_values()
        .trait_application_data(requirement.trait_application())
        .map_err(FactQueryError::SemanticValueStore)?;

    let Some(callable) = implementation_callable_instance(
        binding_context,
        fulfillments.callables,
        member,
        application.substitution(),
        implementation.substitution(),
    )?
    else {
        return Ok(DiagnosticResult::new(None, diagnostics));
    };

    let callable = callable.instance();

    let signature = binding_context
        .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
            callable.definition().callable_symbol(),
        ))
        .map_err(binding_query_error)?;

    diagnostics = diagnostics.merged(signature.diagnostics());

    let checked = compilation.checked_constant_terms_for_templates_with_cancellation(
        [
            signature.value().callable_type(),
            signature.value().result(),
        ],
        cancellation,
    )?;

    diagnostics = diagnostics.merged(checked.diagnostics());

    let signature = resolve_callable_signature_template(
        binding_context.semantic_values(),
        signature.value(),
        callable.substitution(),
        checked.value(),
    )
    .map_err(FactQueryError::from)?;

    Ok(DiagnosticResult::new(
        signature.map(|signature| (requirement, *witness, callable, signature)),
        diagnostics,
    ))
}
