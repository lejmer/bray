use bray_binder::{BinderFactContext, SymbolFactProvider};
use bray_bound_tree::{ConstructionDefaultProvider, ConstructionInputId, ConstructionTarget};
use bray_checker::{
    ConstructionInputSurface, ImplementationSelectionEvidence, OperationCandidate,
    OperationCandidateState, resolve_callable_signature_template,
};
use bray_compiler_known::CompilerKnownDeclarationKey;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CallableSignatureFact, GenericArgument, GenericParameterSymbolId, ImplementationSelection,
    RuntimeDefaultPresence, SymbolFactRequest, SymbolName, TraitCallableMemberSymbolId, TypeData,
    TypeId,
};

use super::super::super::Compilation;
use super::super::super::binder::{CompilationBinderFacts, binder_fact_error};
use super::super::super::implementation::{
    callable_instance, implementation_fulfillments, implementation_requirement, selected_callable,
};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn type_form_construction_candidate(
        &self,
        facts: &CompilationBinderFacts<'_>,
        result_type: TypeId,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationCandidate>, FactQueryError> {
        let data = facts
            .semantic_values()
            .type_data(result_type)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::OwnedIndirection { storage, target } = data.as_ref() else {
            return Ok(None);
        };

        let contract = self
            .available_compiler_known_symbols()
            .operation_contract(bray_compiler_known::CompilerKnownOperationRole::BoxConstruction)
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
            *storage,
            contract.trait_definition(),
            parameters,
            [GenericArgument::Type(*target)],
        )?;

        let selected =
            self.implementation_selection_result_with_cancellation(requirement, cancellation)?;

        *diagnostics = diagnostics.merged(selected.diagnostics());

        let ImplementationSelection::Selected(witness) = selected.value() else {
            return Ok(None);
        };

        let implementation = facts
            .semantic_values()
            .implementation_instance_data(*witness)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let fulfillments = implementation_fulfillments(facts, implementation.definition())?;

        let member_key = CompilerKnownDeclarationKey::try_new("StorageCreate")
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let member = self
            .available_compiler_known_symbols()
            .declaration_symbol::<TraitCallableMemberSymbolId>(&member_key)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let Some(fulfillment) = selected_callable(facts, fulfillments.callables, member) else {
            return Ok(None);
        };

        let signature = facts
            .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(
                fulfillment.into(),
            ))
            .map_err(binder_fact_error)?;

        *diagnostics = diagnostics.merged(signature.diagnostics());

        let checked = self.checked_constant_terms_for_templates_with_cancellation(
            [
                signature.value().callable_type(),
                signature.value().result(),
            ],
            cancellation,
        )?;

        *diagnostics = diagnostics.merged(checked.diagnostics());

        let trait_application = facts
            .semantic_values()
            .trait_application_data(requirement.trait_application())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let callable_substitutions = [
            trait_application.substitution(),
            implementation.substitution(),
        ];

        let callable = callable_instance(
            facts.semantic_values(),
            fulfillment.into(),
            callable_substitutions,
        )?;

        let Some(resolved_signature) = resolve_callable_signature_template(
            facts.semantic_values(),
            signature.value(),
            callable.substitution(),
            checked.value(),
        )
        .map_err(FactQueryError::CheckerInfrastructure)?
        else {
            return Ok(None);
        };

        let callable_type = facts
            .semantic_values()
            .type_data(resolved_signature.callable_type())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::Callable(callable_type) = callable_type.as_ref() else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        if callable_type.parameters().len() != resolved_signature.parameters().len() {
            return Err(FactQueryError::InfrastructureFailure);
        }

        let mut state = OperationCandidateState::Available;
        let mut surfaces = Vec::with_capacity(resolved_signature.parameters().len());

        for (parameter, parameter_type) in resolved_signature
            .parameters()
            .iter()
            .zip(callable_type.parameters())
        {
            let parameter_type_id = parameter.ty();
            let parameter = parameter.parameter();

            let record = facts
                .symbols()
                .callable_parameter(parameter)
                .ok_or(FactQueryError::InfrastructureFailure)?;

            if record.default_presence() == RuntimeDefaultPresence::Recovered {
                state = OperationCandidateState::Recovered;
            }

            let name = SymbolName::try_new(parameter_type.name().as_str())
                .ok_or(FactQueryError::InfrastructureFailure)?;

            surfaces.push(ConstructionInputSurface::new(
                ConstructionInputId::CallableParameter(parameter),
                name,
                parameter_type.position(),
                parameter_type_id,
                record
                    .default_provider()
                    .map(ConstructionDefaultProvider::CallableParameter),
                record.ordinal(),
            ));
        }

        let target = ConstructionTarget::TypeForm {
            callable,
            requirement,
            witness: *witness,
        };

        let key = facts
            .symbol_key(implementation.definition().into_any())
            .map_err(binder_fact_error)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        // The candidate owns the shared key returned by the immutable symbol table.
        Ok(Some(
            OperationCandidate::symbol_construction(
                key.clone(),
                target,
                result_type,
                surfaces,
                state,
            )
            // The selected fact and candidate evidence own the same immutable selection.
            .with_implementation_selections([ImplementationSelectionEvidence::new(
                requirement,
                selected.value().clone(),
            )]),
        ))
    }
}
