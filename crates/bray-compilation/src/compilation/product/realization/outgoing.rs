use bray_bound_tree::{LifecycleAction, LifecycleCallable, LifecyclePhase};
use bray_codegen::CodegenTarget;
use bray_compiler_known::RepresentationRole;
use bray_ir::{MirCallTarget, MirCleanupPhase, MirHelperReference, MirOperationKind};
use bray_lowering::SyntheticLoweringContext;
use bray_symbols::{CallableExecution, CallableInstanceData, TypeId};

use super::super::specialization::ConcreteCodegenInstance;
use super::synthetic::CompilationSyntheticLoweringContext;
use crate::compilation::{
    CodegenPreparationError, Compilation, ProductDataKind, ProductQueryContext, ProductQueryFailure,
};
use crate::fact::CancellationToken;

impl Compilation {
    pub(super) fn operation_outgoing_capacity(
        &self,
        owner: &ConcreteCodegenInstance,
        operation: &MirOperationKind,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Option<u32>, CodegenPreparationError> {
        match operation {
            MirOperationKind::AdmitOutgoing { ty, .. }
            | MirOperationKind::DischargeOutgoing { ty, .. } => {
                let ty = self.concrete_codegen_type(
                    *ty,
                    owner.substitution(),
                    Some(owner),
                    cancellation,
                )?;

                self.owner_outgoing_capacity(ty, cancellation).map(Some)
            }
            MirOperationKind::Call(call) if call.is_cleanup() => {
                let MirCallTarget::Direct(reference) = call.target() else {
                    return Err(ProductQueryFailure::InvalidCleanupCallTarget {
                        context: ProductQueryContext::Instance(owner.key().clone()),
                        // Retain the rejected target only while constructing its diagnostic.
                        target: call.target().clone(),
                    }
                    .into());
                };

                let callee = self.concrete_codegen_callable_data(
                    owner,
                    &reference.instance(),
                    target,
                    cancellation,
                )?;

                let callable = callee.callable_instance().ok_or_else(|| {
                    ProductQueryFailure::missing(
                        ProductQueryContext::Instance(callee.key().clone()),
                        ProductDataKind::ConcreteInstance,
                    )
                })?;

                let result = self.concrete_codegen_type(
                    call.result().ty(),
                    owner.substitution(),
                    Some(owner),
                    cancellation,
                )?;

                self.call_outgoing_capacity(
                    callable,
                    CallableExecution::Synchronous,
                    result,
                    cancellation,
                )
                .map(Some)
            }
            _ => Ok(None),
        }
    }

    pub(super) fn owner_outgoing_capacity(
        &self,
        ty: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<u32, CodegenPreparationError> {
        let context = CompilationSyntheticLoweringContext::new(self, cancellation)?;
        let mut capacity = 0;

        for (phase, helper) in [
            (LifecyclePhase::Finalize, MirHelperReference::Finalize(ty)),
            (LifecyclePhase::Destroy, MirHelperReference::Destroy(ty)),
            (
                LifecyclePhase::Cancel,
                MirHelperReference::Cleanup {
                    phase: MirCleanupPhase::TaskCancellation,
                    ty,
                },
            ),
        ] {
            if self.codegen_lifecycle_is_trivial(&helper, cancellation)? {
                continue;
            }

            let mut add = |call: LifecycleCallable| -> Result<(), CodegenPreparationError> {
                capacity += self.call_outgoing_capacity(
                    call.callable,
                    call.execution,
                    call.result,
                    cancellation,
                )?;

                Ok(())
            };

            match context.lifecycle_action(ty, phase)? {
                LifecycleAction::Call(call) => add(call)?,
                LifecycleAction::Owned {
                    borrow, teardown, ..
                } => {
                    add(borrow)?;

                    for call in teardown.into_iter().flatten() {
                        add(call)?;
                    }
                }
                // Structural forwarding uses the child owner's existing allowance.
                _ => {}
            }
        }

        Ok(capacity)
    }

    fn call_outgoing_capacity(
        &self,
        callable: CallableInstanceData,
        execution: CallableExecution,
        result: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<u32, CodegenPreparationError> {
        let unit = self.codegen_representation_type(RepresentationRole::Unit)?;

        let empty = execution == CallableExecution::Synchronous
            && result == unit
            && self
                .verified_execution_obligation(
                    callable,
                    bray_checker::ExecutionObligation::Property(
                        bray_checker::ExecutionProperty::Total,
                        None,
                    ),
                    None,
                    cancellation,
                )?
                .is_some();

        Ok(u32::from(!empty))
    }
}
