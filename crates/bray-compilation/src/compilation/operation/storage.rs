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
use super::query::symbol_contract_failure;
use crate::compilation::{
    SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
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
    let role = CompilerKnownOperationRole::BoxConstruction;

    let contract = compilation
        .available_compiler_known_symbols()
        .operation_contract(role)
        .ok_or_else(|| compiler_known_operation_unavailable(storage, target, role))?;

    let trait_symbol = binding_context
        .symbols()
        .trait_symbol(contract.trait_definition())
        .ok_or_else(|| {
            symbol_contract_failure(
                contract.trait_definition().into(),
                SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
            )
        })?;

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
        .implementation_instance_data(*witness);

    let fulfillments = implementation_fulfillments(binding_context, implementation.definition())?;

    let member = compilation
        .available_compiler_known_symbols()
        .declaration_symbol::<TraitCallableMemberSymbolId>(member_key)
        .ok_or_else(|| compiler_known_declaration_unavailable(member_key))?;

    let application = binding_context
        .semantic_values()
        .trait_application_data(requirement.trait_application());

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

    let self_context = if callable.uses_trait_default() {
        bray_symbols::SelfTypeContext::Trait(application.definition())
    } else {
        bray_symbols::SelfTypeContext::Implementation(implementation.definition())
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

    let signature = signature
        .map(|signature| {
            super::signature::substitute_callable_self(
                binding_context,
                signature,
                self_context,
                storage,
            )
        })
        .transpose()?;

    Ok(DiagnosticResult::new(
        signature.map(|signature| (requirement, *witness, callable, signature)),
        diagnostics,
    ))
}

fn compiler_known_operation_unavailable(
    storage: TypeId,
    target: TypeId,
    role: CompilerKnownOperationRole,
) -> FactQueryError {
    SemanticQueryFailure::contract(
        SemanticQueryContext::Type(storage),
        SemanticQueryViolation::CompilerKnownOperationUnavailable {
            storage,
            target,
            role,
        },
    )
    .into()
}

fn compiler_known_declaration_unavailable(key: &CompilerKnownDeclarationKey) -> FactQueryError {
    SemanticQueryFailure::contract(
        SemanticQueryContext::CompilerKnownDeclaration(key.clone()),
        SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
    )
    .into()
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::CompilerKnownOperationRole;
    use bray_symbols::{SemanticValueStore, TypeData};

    use super::{compiler_known_declaration_unavailable, compiler_known_operation_unavailable};
    use crate::compilation::{
        SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
    };
    use crate::fact::FactQueryError;

    #[test]
    fn missing_storage_contract_retains_types_and_operation_role() {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store must build: {error:?}"));

        let storage = values
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("storage type must intern: {error:?}"));

        let target = values
            .intern_type(TypeData::tuple([storage]))
            .unwrap_or_else(|error| panic!("target type must intern: {error:?}"));

        let role = CompilerKnownOperationRole::BoxConstruction;

        assert_eq!(
            compiler_known_operation_unavailable(storage, target, role),
            FactQueryError::from(SemanticQueryFailure::contract(
                SemanticQueryContext::Type(storage),
                SemanticQueryViolation::CompilerKnownOperationUnavailable {
                    storage,
                    target,
                    role,
                },
            ))
        );
    }

    #[test]
    fn missing_storage_member_retains_compiler_known_declaration_key() {
        let Some(key) = bray_compiler_known::CompilerKnownDeclarationKey::try_new("StorageCreate")
        else {
            panic!("test compiler-known declaration key must be valid");
        };

        assert_eq!(
            compiler_known_declaration_unavailable(&key),
            FactQueryError::from(SemanticQueryFailure::contract(
                SemanticQueryContext::CompilerKnownDeclaration(key),
                SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
            ))
        );
    }
}
