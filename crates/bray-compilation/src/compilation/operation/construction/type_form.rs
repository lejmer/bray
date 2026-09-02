use bray_binder::BindingQueryContext;
use bray_bound_tree::{ConstructionDefaultProvider, ConstructionInputId, ConstructionTarget};
use bray_checker::{
    ConstructionInputSurface, ImplementationSelectionEvidence, OperationCandidate,
    OperationCandidateState,
};
use bray_compiler_known::CompilerKnownDeclarationKey;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{ImplementationSelection, RuntimeDefaultPresence, SymbolName, TypeData, TypeId};

use super::super::super::Compilation;
use super::super::super::binder::{CompilationBindingContext, binding_query_error};
use super::super::selected_storage_callable;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn type_form_construction_candidate(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        result_type: TypeId,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationCandidate>, FactQueryError> {
        let data = binding_context
            .semantic_values()
            .type_data(result_type)
            .map_err(FactQueryError::SemanticValueStore)?;

        let TypeData::OwnedIndirection { storage, target } = data.as_ref() else {
            return Ok(None);
        };

        let member_key = CompilerKnownDeclarationKey::try_new("StorageCreate")
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let selected = selected_storage_callable(
            self,
            binding_context,
            *storage,
            *target,
            &member_key,
            cancellation,
        )?;

        *diagnostics = diagnostics.merged(selected.diagnostics());

        let Some((requirement, witness, callable, resolved_signature)) = selected.value() else {
            return Ok(None);
        };

        let callable_type = binding_context
            .semantic_values()
            .type_data(resolved_signature.callable_type())
            .map_err(FactQueryError::SemanticValueStore)?;

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

            let record = binding_context
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
            callable: *callable,
            requirement: *requirement,
            witness: *witness,
        };

        let implementation = binding_context
            .semantic_values()
            .implementation_instance_data(*witness)
            .map_err(FactQueryError::SemanticValueStore)?;

        let key = binding_context
            .symbol_key(implementation.definition().into_any())
            .map_err(binding_query_error)?
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
            .with_implementation_selections([ImplementationSelectionEvidence::new(
                *requirement,
                ImplementationSelection::Selected(*witness),
            )]),
        ))
    }
}
