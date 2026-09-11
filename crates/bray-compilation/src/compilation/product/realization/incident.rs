use bray_codegen::{CodegenCleanupIncident, CodegenTarget};
use bray_ir::{
    MirAbandonmentAction, MirHelperReference, MirSourceAnchor, MirStandardLibraryHelper,
};
use bray_symbols::TypeId;

use super::super::super::{CodegenPreparationError, Compilation};
use super::super::specialization::ConcreteCodegenInstance;
use crate::fact::CancellationToken;

pub(super) struct ConcreteCleanupIncident {
    ty: TypeId,
    type_identity: [u8; 32],
    source: Option<MirSourceAnchor>,
    pub(super) cleanup: ConcreteCodegenInstance,
    pub(super) allocation: ConcreteCodegenInstance,
    pub(super) deallocation: ConcreteCodegenInstance,
}

impl ConcreteCleanupIncident {
    pub(super) fn into_dependencies(self) -> [ConcreteCodegenInstance; 3] {
        [self.cleanup, self.allocation, self.deallocation]
    }

    pub(super) fn into_mapping(self) -> CodegenCleanupIncident {
        // Mappings retain the Arc-backed instance keys independently of query-owned instances.
        CodegenCleanupIncident::new(
            self.ty,
            self.type_identity,
            self.source,
            self.cleanup.key().clone(),
            self.allocation.key().clone(),
            self.deallocation.key().clone(),
        )
    }
}

impl Compilation {
    pub(super) fn concrete_operation_error_identity(
        &self,
        owner: &ConcreteCodegenInstance,
        operation: &bray_ir::MirOperation,
        cancellation: &CancellationToken,
    ) -> Result<Option<[u8; 32]>, CodegenPreparationError> {
        let bray_ir::MirOperationKind::Host(bray_ir::MirHostOperation::ResolveRootTerminal {
            error: Some(ty),
            ..
        }) = operation.kind()
        else {
            return Ok(None);
        };

        let ty =
            self.concrete_codegen_type(*ty, owner.substitution(), Some(owner), cancellation)?;

        let values = self.semantic_value_store()?;
        let context = self.binding_context(cancellation)?;

        Ok(Some(super::super::structural_type_identity(
            values, &context, ty,
        )?))
    }

    pub(super) fn concrete_operation_incident(
        &self,
        owner: &ConcreteCodegenInstance,
        mir: &bray_ir::MirUnit,
        operation: &bray_ir::MirOperation,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Option<ConcreteCleanupIncident>, CodegenPreparationError> {
        let bray_ir::MirOperationKind::Async(bray_ir::MirAsyncOperation::TransferCleanupIncident {
            invocation,
            incident,
            ..
        }) = operation.kind()
        else {
            return Ok(None);
        };

        let ty = mir.operand_type(incident).map_err(|cause| {
            CodegenPreparationError::InvalidSpecializedMir {
                source: operation.source().clone(),
                cause,
            }
        })?;

        let ty = self.concrete_codegen_type(ty, owner.substitution(), Some(owner), cancellation)?;

        let source = self.cleanup_incident_source(mir, *invocation, operation.source())?;

        self.concrete_cleanup_incident(ty, source, target, cancellation)
            .map(Some)
    }

    fn cleanup_incident_source(
        &self,
        mir: &bray_ir::MirUnit,
        invocation: bray_ir::MirOperationId,
        source: &MirSourceAnchor,
    ) -> Result<Option<MirSourceAnchor>, CodegenPreparationError> {
        let invalid = || CodegenPreparationError::InvalidSpecializedMir {
            source: source.clone(),
            cause: bray_ir::MirUnitBuildError::InvalidCall(invocation),
        };

        let producer = mir.operation(invocation).ok_or_else(invalid)?;

        let call = match producer.kind() {
            bray_ir::MirOperationKind::Call(call)
            | bray_ir::MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame {
                initializer: bray_ir::MirFrameInitializer::Callable(call),
                ..
            }) => call,
            _ => return Err(invalid()),
        };

        let bray_ir::MirCallTarget::Direct(callable) = call.target() else {
            return Err(invalid());
        };

        Ok(Some(
            self.callable_body_key(callable.instance().definition())?
                .map_or_else(
                    || source.clone(),
                    |key| {
                        MirSourceAnchor::source(bray_bound_tree::BoundNodeOrigin::source(
                            key.source(),
                        ))
                    },
                ),
        ))
    }

    pub(super) fn concrete_cleanup_incident(
        &self,
        ty: TypeId,
        source: Option<MirSourceAnchor>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCleanupIncident, CodegenPreparationError> {
        let values = self.semantic_value_store()?;
        let context = self.binding_context(cancellation)?;
        let type_identity = super::super::structural_type_identity(values, &context, ty)?;

        Ok(ConcreteCleanupIncident {
            ty,
            type_identity,
            source,
            cleanup: self.concrete_codegen_lifecycle(
                MirHelperReference::Abandon {
                    action: MirAbandonmentAction::Destroy,
                    ty,
                },
                target,
            )?,
            allocation: self.concrete_standard_library_helper(
                MirStandardLibraryHelper::MemoryAllocate,
                target,
                cancellation,
            )?,
            deallocation: self.concrete_standard_library_helper(
                MirStandardLibraryHelper::MemoryDeallocate,
                target,
                cancellation,
            )?,
        })
    }
}
