use bray_bound_tree::LifecycleCallable;
use bray_symbols::{TypeAssociatedLifecycleSlot, TypeData, TypeId};

use super::super::support::receiver_codegen_type;
use crate::compilation::CodegenPreparationError;
use crate::compilation::Compilation;
use crate::compilation::{
    ProductDataKind, ProductQueryContext, ProductQueryFailure, ProductValueKind,
};
use crate::fact::CancellationToken;
use bray_compiler_known::CompilerKnownDeclarationKey;

impl Compilation {
    pub(in crate::compilation::product::realization) fn lifecycle_callable(
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

        let receiver = signature.receiver().ok_or_else(|| {
            ProductQueryFailure::missing(
                ProductQueryContext::CallableData(*callable),
                ProductDataKind::CallableReceiver,
            )
        })?;

        let receiver_ty =
            self.concrete_codegen_type(receiver.ty(), Some(*substitution), None, cancellation)?;

        let receiver = receiver_codegen_type(values, receiver_ty, receiver.mode())?;

        let result = self.concrete_codegen_type(
            signature.result(),
            Some(*substitution),
            None,
            cancellation,
        )?;

        let callable_type = values.type_data(signature.callable_type());

        let TypeData::Callable(callable_type) = callable_type.as_ref() else {
            return Err(ProductQueryFailure::UnexpectedSemanticType {
                ty: signature.callable_type(),
                expected: ProductValueKind::CallableType,
                actual: callable_type.as_ref().clone(),
            }
            .into());
        };

        Ok(Some(LifecycleCallable {
            callable: *callable,
            abi: callable_type.abi(),
            receiver,
            result,
            execution: callable_type.execution(),
        }))
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

        let Some((_, _, callable, signature)) = selected.value() else {
            return Err(CodegenPreparationError::UnsupportedType(storage));
        };

        let values = self.semantic_value_store()?;

        let callable_type = values.type_data(signature.callable_type());

        let TypeData::Callable(callable_type) = callable_type.as_ref() else {
            return Err(ProductQueryFailure::UnexpectedSemanticType {
                ty: signature.callable_type(),
                expected: ProductValueKind::CallableType,
                actual: callable_type.as_ref().clone(),
            }
            .into());
        };

        let [parameter] = signature.parameters() else {
            return Err(ProductQueryFailure::count_mismatch(
                // The failure owns the protocol identity after this query returns.
                ProductQueryContext::CompilerKnownDeclaration(member.clone()),
                ProductDataKind::CallableParameters,
                1,
                signature.parameters().len(),
            )
            .into());
        };

        Ok(LifecycleCallable {
            callable: *callable,
            abi: callable_type.abi(),
            receiver: parameter.ty(),
            result: signature.result(),
            execution: bray_symbols::CallableExecution::Synchronous,
        })
    }
}
