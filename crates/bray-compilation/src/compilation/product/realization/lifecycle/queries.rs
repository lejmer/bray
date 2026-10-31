use bray_bound_tree::LifecycleCallable;
use bray_symbols::{TypeAssociatedLifecycleSlot, TypeData, TypeId};

use crate::compilation::CodegenPreparationError;
use crate::compilation::Compilation;
use crate::fact::CancellationToken;
use bray_compiler_known::CompilerKnownDeclarationKey;

impl Compilation {
    pub(in crate::compilation::product) fn lifecycle_callable(
        &self,
        ty: TypeId,
        slot: TypeAssociatedLifecycleSlot,
        cancellation: &CancellationToken,
    ) -> Result<Option<LifecycleCallable>, CodegenPreparationError> {
        let values = self.semantic_value_store()?;

        let data = values.type_data(ty);

        let TypeData::Named { substitution, .. } = data.as_ref() else {
            return Ok(None);
        };

        let selected = self.selected_lifecycle_signature(ty, slot, cancellation)?;

        if selected.diagnostics().has_errors() {
            // The failure retains selected lifecycle diagnostics after the query result drops.
            return Err(CodegenPreparationError::Diagnostics(
                selected.diagnostics().clone(),
            ));
        }

        let Some((callable, signature)) = selected.value() else {
            return Ok(None);
        };

        let mut selected = LifecycleCallable::from_signature(
            values,
            self.available_compiler_known_symbols(),
            *callable,
            signature,
        )
        .map_err(crate::fact::FactQueryError::SemanticValueStore)?;

        selected.receiver =
            self.concrete_codegen_type(selected.receiver, Some(*substitution), None, cancellation)?;

        let concrete = |ty| self.concrete_codegen_type(ty, Some(*substitution), None, cancellation);

        selected.result = match selected.result {
            bray_bound_tree::BoundCallResult::Immediate(ty) => {
                bray_bound_tree::BoundCallResult::Immediate(concrete(ty)?)
            }
            bray_bound_tree::BoundCallResult::LazyFuture(future) => {
                bray_bound_tree::BoundCallResult::LazyFuture(
                    bray_bound_tree::BoundFutureConstruction::new(
                        concrete(future.completion_type())?,
                        concrete(future.future_type())?,
                    ),
                )
            }
        };

        Ok(Some(selected))
    }

    pub(in crate::compilation::product::realization) fn storage_lifecycle_callable(
        &self,
        storage: TypeId,
        target: TypeId,
        member: &CompilerKnownDeclarationKey,
        cancellation: &CancellationToken,
    ) -> Result<LifecycleCallable, CodegenPreparationError> {
        let binding_context = self.binding_context(cancellation)?;

        let selected = crate::compilation::operation::selected_storage_callable(
            self,
            &binding_context,
            storage,
            target,
            member,
            cancellation,
        )?;

        if selected.diagnostics().has_errors() {
            // Preserve the selected implementation's diagnostics beyond this query result.
            return Err(CodegenPreparationError::Diagnostics(
                selected.diagnostics().clone(),
            ));
        }

        let (_, _, callable, signature) = selected.value().as_ref().unwrap_or_else(|| {
            panic!(
                "checked storage {storage:?} for {target:?} retains selected operation {member:?}"
            )
        });

        let values = self.semantic_value_store()?;

        let callable_type = values.type_data(signature.callable_type());

        let TypeData::Callable(callable_type) = callable_type.as_ref() else {
            panic!(
                "checked storage operation {member:?} retains callable type {:?}, got {callable_type:?}",
                signature.callable_type()
            );
        };

        let [parameter] = signature.parameters() else {
            panic!(
                "checked storage operation {member:?} retains one parameter, got {:?}",
                signature.parameters()
            );
        };

        Ok(LifecycleCallable {
            callable: *callable,
            abi: callable_type.abi(),
            phase_behaviors: std::sync::Arc::new(callable_type.phase_behaviors().clone()),
            receiver: parameter.ty(),
            result: bray_bound_tree::BoundCallResult::Immediate(signature.result()),
        })
    }
}
