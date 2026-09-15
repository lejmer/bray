use bray_binder::SymbolQueryProvider;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    CallableInstanceData, CallableSignature, CallableSignatureQuery, SymbolQueryRequest,
    TypeAssociatedLifecycleSlot, TypeData, TypeId,
};

use super::{
    Compilation, SemanticDataKind, SemanticQueryContext, SemanticQueryFailure,
    SemanticQueryViolation,
};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation) fn selected_lifecycle_signature(
        &self,
        ty: TypeId,
        slot: TypeAssociatedLifecycleSlot,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<(CallableInstanceData, CallableSignature)>>, FactQueryError>
    {
        let values = self.semantic_value_store()?;

        let data = values.type_data(ty);

        let TypeData::Named {
            definition,
            substitution,
        } = data.as_ref()
        else {
            return Ok(DiagnosticResult::without_diagnostics(None));
        };

        let surface =
            self.type_associated_surface_result_with_cancellation(*definition, cancellation)?;

        let members = surface
            .value()
            .lifecycle_members()
            .iter()
            .filter(|member| member.slot() == slot)
            .map(|member| member.id())
            .collect::<Vec<_>>();

        let member = match members.as_slice() {
            [] => {
                // The result retains surface diagnostics after releasing the query publication.
                return Ok(DiagnosticResult::new(None, surface.diagnostics().clone()));
            }
            [member] => *member,
            _ => {
                return Err(SemanticQueryFailure::contract(
                    SemanticQueryContext::Type(ty),
                    SemanticQueryViolation::CountMismatch {
                        data: SemanticDataKind::LifecycleMember,
                        expected: 1,
                        actual: members.len(),
                    },
                )
                .into());
            }
        };

        let callable = super::implementation::callable_instance(values, member, [*substitution])?;
        let binding = self.binding_context(cancellation)?;

        let signature = binding
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                callable.definition().callable_symbol(),
            ))
            .map_err(super::binder::binding_query_error)?;

        let constants = self.checked_constant_terms_for_templates_with_cancellation(
            [
                signature.value().callable_type(),
                signature.value().result(),
            ],
            cancellation,
        )?;

        let resolved = bray_checker::resolve_callable_signature_template(
            values,
            signature.value(),
            callable.substitution(),
            constants.value(),
        )
        .map_err(FactQueryError::from)?;

        let diagnostics = DiagnosticBag::merged_all([
            surface.diagnostics(),
            signature.diagnostics(),
            constants.diagnostics(),
        ]);

        if resolved.is_none() && diagnostics.has_errors() {
            return Ok(DiagnosticResult::new(None, diagnostics));
        }

        let signature = resolved.ok_or_else(|| {
            SemanticQueryFailure::contract(
                SemanticQueryContext::Type(ty),
                SemanticQueryViolation::Missing(SemanticDataKind::CallableSignature),
            )
        })?;

        Ok(DiagnosticResult::new(
            Some((callable, signature)),
            diagnostics,
        ))
    }
}
