use bray_codegen::CodegenTarget;
use bray_ir::{MirCallableReference, MirHelperReference, MirOperationKind};
use bray_symbols::{TypeAssociatedLifecycleSlot, TypeId};

use crate::compilation::{CodegenPreparationError, Compilation};
use crate::fact::CancellationToken;

use super::super::super::specialization::ConcreteCodegenInstance;

impl Compilation {
    pub(in crate::compilation::product) fn concrete_operation_cleanup_allowance(
        &self,
        owner: &ConcreteCodegenInstance,
        operation: &MirOperationKind,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Option<Vec<ConcreteCodegenInstance>>, CodegenPreparationError> {
        let (MirOperationKind::AdmitCleanup(ty) | MirOperationKind::DischargeCleanup(ty)) =
            operation
        else {
            return Ok(None);
        };

        let ty =
            self.concrete_codegen_type(*ty, owner.substitution(), Some(owner), cancellation)?;

        let mut requirements = Vec::new();

        for (slot, callable) in self.cleanup_allowance_callables(ty, cancellation)? {
            let invocation = if slot == TypeAssociatedLifecycleSlot::Destructor {
                // The consuming body's realized remainder can require a protected frame even
                // when its declared callable is synchronous. Use that exact implementation.
                self.concrete_codegen_lifecycle(MirHelperReference::Destroy(ty), target)?
            } else {
                self.concrete_codegen_callable_data(
                    owner,
                    &callable.instance(),
                    target,
                    cancellation,
                )?
            };

            requirements.push(invocation);
        }

        Ok(Some(requirements))
    }

    pub(in crate::compilation::product) fn cleanup_allowance_callables(
        &self,
        ty: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<Vec<(TypeAssociatedLifecycleSlot, MirCallableReference)>, CodegenPreparationError>
    {
        let mut requirements = Vec::new();

        let context = crate::compilation::checker::CompilationCheckerContext::new(
            self.binding_context(cancellation)?,
        );

        for slot in [
            TypeAssociatedLifecycleSlot::Finalizer,
            TypeAssociatedLifecycleSlot::Destructor,
        ] {
            let Some((callable, _, result, execution)) =
                self.lifecycle_callable(ty, slot, cancellation)?
            else {
                continue;
            };

            let conditions =
                self.execution_callable_conditions(callable.instance(), cancellation)?;

            let certified = self.instance_callable_proofs(callable.instance(), cancellation)?;
            let diagnostics = conditions.diagnostics().merged(certified.diagnostics());

            if diagnostics.has_errors() {
                return Err(CodegenPreparationError::Diagnostics(diagnostics));
            }

            if bray_checker::prove_cleanup_admission_free(
                &context,
                conditions.value(),
                certified.value(),
                execution,
                result,
            )
            .map_err(crate::fact::FactQueryError::from)?
            .is_some()
            {
                continue;
            }

            requirements.push((slot, callable));
        }

        Ok(requirements)
    }
}
