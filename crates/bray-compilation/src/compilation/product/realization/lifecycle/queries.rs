use bray_ir::MirCallableReference;
use bray_symbols::{
    CallableExecution, CallableSignature, TypeAssociatedLifecycleSlot, TypeData, TypeId,
};

use super::super::support::receiver_codegen_type;
use crate::compilation::CodegenPreparationError;
use crate::compilation::Compilation;
use crate::compilation::{
    ProductDataKind, ProductQueryContext, ProductQueryFailure, ProductValueKind,
};
use crate::fact::{CancellationToken, FactQueryError};
use bray_compiler_known::CompilerKnownDeclarationKey;

impl Compilation {
    pub(in crate::compilation::product) fn lifecycle_callable(
        &self,
        ty: TypeId,
        slot: TypeAssociatedLifecycleSlot,
        cancellation: &CancellationToken,
    ) -> Result<
        Option<(MirCallableReference, TypeId, TypeId, CallableExecution)>,
        CodegenPreparationError,
    > {
        let values = self.semantic_value_store()?;
        let binding_context = self.binding_context(cancellation)?;

        let selected =
            crate::compilation::operation::selected_lifecycle_callable(&binding_context, ty, slot)?;

        if selected.diagnostics().has_errors() {
            // The codegen failure retains the semantic selection's owned diagnostics.
            return Err(CodegenPreparationError::Diagnostics(
                selected.diagnostics().clone(),
            ));
        }

        let (selected, _) = selected.into_parts();

        let Some((callable, signature)) = selected else {
            return Ok(None);
        };

        let receiver = signature.receiver().ok_or_else(|| {
            ProductQueryFailure::missing(
                ProductQueryContext::CallableData(callable),
                ProductDataKind::CallableReceiver,
            )
        })?;

        let receiver_ty = self.concrete_codegen_type(
            receiver.ty(),
            Some(callable.substitution()),
            None,
            cancellation,
        )?;

        let receiver = receiver_codegen_type(values, receiver_ty, receiver.mode())?;

        let result = self.concrete_codegen_type(
            signature.result(),
            Some(callable.substitution()),
            None,
            cancellation,
        )?;

        let callable_type = values
            .type_data(signature.callable_type())
            .map_err(FactQueryError::SemanticValueStore)?;

        let TypeData::Callable(callable_type) = callable_type.as_ref() else {
            return Err(ProductQueryFailure::UnexpectedSemanticType {
                ty: signature.callable_type(),
                expected: ProductValueKind::CallableType,
                actual: callable_type.as_ref().clone(),
            }
            .into());
        };

        Ok(Some((
            MirCallableReference::new(callable, callable_type.abi()),
            receiver,
            result,
            callable_type.execution(),
        )))
    }

    pub(in crate::compilation::product::realization) fn storage_lifecycle_callable(
        &self,
        storage: TypeId,
        target: TypeId,
        member: &CompilerKnownDeclarationKey,
        cancellation: &CancellationToken,
    ) -> Result<(MirCallableReference, CallableSignature), CodegenPreparationError> {
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

        let callable_type = values
            .type_data(signature.callable_type())
            .map_err(FactQueryError::SemanticValueStore)?;

        let TypeData::Callable(callable_type) = callable_type.as_ref() else {
            return Err(ProductQueryFailure::UnexpectedSemanticType {
                ty: signature.callable_type(),
                expected: ProductValueKind::CallableType,
                actual: callable_type.as_ref().clone(),
            }
            .into());
        };

        // The generated MIR owns this Arc-backed signature after releasing the query result.
        Ok((
            MirCallableReference::new(*callable, callable_type.abi()),
            signature.clone(),
        ))
    }
}
