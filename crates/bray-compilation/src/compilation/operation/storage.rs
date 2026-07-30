use bray_binder::{BinderFactContext, SymbolFactProvider};
use bray_checker::resolve_callable_signature_template;
use bray_compiler_known::{CompilerKnownDeclarationKey, CompilerKnownOperationRole};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    CallableInstanceData, CallableSignature, CallableSignatureFact, GenericArgument,
    GenericParameterSymbolId, ImplementationInstanceId, ImplementationRequirementKey,
    ImplementationSelection, SymbolFactRequest, TraitCallableMemberSymbolId, TypeId,
};

use super::super::Compilation;
use super::super::binder::{CompilationBinderFacts, binder_fact_error};
use super::super::implementation::{
    callable_instance, implementation_fulfillments, implementation_requirement,
    selected_callable,
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
    facts: &CompilationBinderFacts<'_>,
    storage: TypeId,
    target: TypeId,
    member_key: &CompilerKnownDeclarationKey,
    cancellation: &CancellationToken,
) -> Result<DiagnosticResult<Option<SelectedStorageCallable>>, FactQueryError> {
    let contract = compilation
        .available_compiler_known_symbols()
        .operation_contract(CompilerKnownOperationRole::BoxConstruction)
        .ok_or(FactQueryError::InfrastructureFailure)?;

    let trait_symbol = facts
        .symbols()
        .trait_symbol(contract.trait_definition())
        .ok_or(FactQueryError::InfrastructureFailure)?;

    let parameters = trait_symbol
        .generic_type_parameters()
        .iter()
        .copied()
        .map(GenericParameterSymbolId::Type);

    let requirement = implementation_requirement(
        facts.semantic_values(),
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

    let implementation = facts
        .semantic_values()
        .implementation_instance_data(*witness)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let fulfillments = implementation_fulfillments(facts, implementation.definition())?;

    let member = compilation
        .available_compiler_known_symbols()
        .declaration_symbol::<TraitCallableMemberSymbolId>(member_key)
        .ok_or(FactQueryError::InfrastructureFailure)?;

    let Some(fulfillment) = selected_callable(facts, fulfillments.callables, member) else {
        return Ok(DiagnosticResult::new(None, diagnostics));
    };

    let signature = facts
        .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(
            fulfillment.into(),
        ))
        .map_err(binder_fact_error)?;

    diagnostics = diagnostics.merged(signature.diagnostics());

    let checked = compilation.checked_constant_terms_for_templates_with_cancellation(
        [signature.value().callable_type(), signature.value().result()],
        cancellation,
    )?;

    diagnostics = diagnostics.merged(checked.diagnostics());

    let application = facts
        .semantic_values()
        .trait_application_data(requirement.trait_application())
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let callable = callable_instance(
        facts.semantic_values(),
        fulfillment.into(),
        [application.substitution(), implementation.substitution()],
    )?;

    let signature = resolve_callable_signature_template(
        facts.semantic_values(),
        signature.value(),
        callable.substitution(),
        checked.value(),
    )
    .map_err(FactQueryError::CheckerInfrastructure)?;

    Ok(DiagnosticResult::new(
        signature.map(|signature| (requirement, *witness, callable, signature)),
        diagnostics,
    ))
}
