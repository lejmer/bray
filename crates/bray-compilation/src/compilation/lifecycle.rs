use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    CallableInstanceData, CallableSignature,
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

        let mut diagnostics = surface.diagnostics().clone();
        let resolved = self.resolve_callable_instance_signature(&binding, callable, &mut diagnostics)?;

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
