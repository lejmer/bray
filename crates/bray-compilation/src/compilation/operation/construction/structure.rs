use bray_binder::BindingQueryContext;
use bray_bound_tree::{ConstructionDefaultProvider, ConstructionInputId, ConstructionTarget};
use bray_checker::{ConstructionInputSurface, OperationCandidate, OperationCandidateState};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    NamedTypeSymbolId, RuntimeDefaultPresence, StructFieldTypeQuery, SymbolQueryRequest, TypeData,
    TypeId,
};

use super::super::super::Compilation;
use super::super::super::binder::{CompilationBindingContext, binding_query_error};
use super::super::query::symbol_contract_failure;
use crate::compilation::{SemanticDataKind, SemanticQueryViolation};
use crate::fact::FactQueryError;

impl Compilation {
    pub(super) fn struct_construction_candidate(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        result_type: TypeId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationCandidate>, FactQueryError> {
        let data = binding_context
            .semantic_values()
            .type_data(result_type)
            .map_err(FactQueryError::SemanticValueStore)?;

        let TypeData::Named {
            definition: NamedTypeSymbolId::Struct(structure),
            substitution,
        } = data.as_ref()
        else {
            return Ok(None);
        };

        if binding_context
            .symbols()
            .compiler_known_provider()
            .heap_storage_policy()
            == Some(*structure)
        {
            // Only the selected Storage<T> implementation can establish a heap handle's allocation.
            return Ok(None);
        }

        let record = binding_context
            .structure(*structure)
            .map_err(binding_query_error)?
            .ok_or_else(|| {
                symbol_contract_failure(
                    (*structure).into(),
                    SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
                )
            })?;

        let inputs = record
            .fields()
            .iter()
            .copied()
            .map(|field| {
                self.struct_field_input(binding_context, field, *substitution, diagnostics)
            })
            .collect::<Result<Option<Vec<_>>, _>>()?;

        let Some(inputs) = inputs else {
            return Ok(None);
        };

        let is_recovered = inputs.iter().any(|(_, is_recovered)| *is_recovered);
        let inputs = inputs.into_iter().map(|(input, _)| input);

        let key = binding_context
            .symbol_key((*structure).into())
            .map_err(binding_query_error)?
            .ok_or_else(|| {
                symbol_contract_failure(
                    (*structure).into(),
                    SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
                )
            })?;

        // The candidate owns the shared key returned by the immutable symbol table.
        Ok(Some(OperationCandidate::symbol_construction(
            key.clone(),
            ConstructionTarget::Struct(*structure),
            result_type,
            inputs,
            if is_recovered {
                OperationCandidateState::Recovered
            } else {
                OperationCandidateState::Available
            },
        )))
    }

    fn struct_field_input(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        field: bray_symbols::StructFieldSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<(ConstructionInputSurface, bool)>, FactQueryError> {
        let record = binding_context
            .struct_field(field)
            .map_err(binding_query_error)?
            .ok_or_else(|| {
                symbol_contract_failure(
                    field.into(),
                    SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
                )
            })?;

        let Some(name) = binding_context
            .member_name(field.into())
            .map_err(binding_query_error)?
            .cloned()
        else {
            return Ok(None);
        };

        let Some(ty) = self.resolve_member_type(
            binding_context,
            SymbolQueryRequest::<StructFieldTypeQuery>::new(field),
            substitution,
            diagnostics,
        )?
        else {
            return Ok(None);
        };

        let presence = record.default_presence();

        let default = record
            .default_provider()
            .map(ConstructionDefaultProvider::StructField);

        Ok(Some((
            ConstructionInputSurface::new(
                ConstructionInputId::StructField(field),
                name,
                bray_symbols::CallablePosition::NamedOnly,
                ty,
                default,
                record.ordinal(),
            ),
            presence == RuntimeDefaultPresence::Recovered,
        )))
    }
}
