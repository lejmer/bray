use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirAsyncOperation, MirCall, MirCallTarget, MirCallableReference, MirFrameInitializer,
    MirFrameReference, MirOperand, MirOperationKind, MirPlace, MirSourceAnchor, MirUnitBuilder,
};
use bray_symbols::{CallableExecution, TypeData, TypeId};

use super::super::super::super::super::CodegenPreparationError;
use super::super::super::super::super::Compilation;
use super::super::super::super::super::{
    ProductDataKind, ProductQueryContext, ProductQueryFailure,
};
use crate::fact::FactQueryError;

impl Compilation {
    pub(in crate::compilation::product::realization) fn push_lifecycle_operation(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        operation: MirOperationKind,
    ) -> Result<(), CodegenPreparationError> {
        builder
            .push_operation(block, source.clone(), operation, None)
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        Ok(())
    }

    pub(in crate::compilation::product::realization) fn push_lifecycle_call(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: (MirCallableReference, TypeId, TypeId, CallableExecution),
    ) -> Result<(), CodegenPreparationError> {
        let (callable, receiver, result, _) = callable;

        let receiver = self.lifecycle_receiver_operand(builder, block, source, place, receiver)?;

        builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Call(MirCall::protocol(
                    MirCallTarget::Direct(callable),
                    bray_bound_tree::BoundCallResult::Immediate(result),
                    [receiver],
                    [],
                )),
                Some(result),
            )
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        Ok(())
    }

    pub(in crate::compilation::product::realization) fn push_static_finalizer_call(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: (MirCallableReference, TypeId, TypeId, CallableExecution),
    ) -> Result<bray_ir::MirValueId, CodegenPreparationError> {
        let (callable, receiver, result, execution) = callable;

        let receiver = self.lifecycle_receiver_operand(builder, block, source, place, receiver)?;

        let (operation, operation_result) = match execution {
            CallableExecution::Synchronous => (
                MirOperationKind::Call(MirCall::protocol(
                    MirCallTarget::Direct(callable),
                    bray_bound_tree::BoundCallResult::Immediate(result),
                    [receiver],
                    [],
                )),
                result,
            ),
            CallableExecution::Asynchronous => {
                let future = self
                    .available_compiler_known_symbols()
                    .unary_representation_type(
                        self.semantic_value_store()?,
                        RepresentationRole::Future,
                        result,
                    )
                    .map_err(FactQueryError::SemanticValueStore)?
                    .ok_or_else(|| {
                        ProductQueryFailure::missing(
                            ProductQueryContext::UnaryRepresentation {
                                role: RepresentationRole::Future,
                                argument: result,
                            },
                            ProductDataKind::ResolvedType,
                        )
                    })?;

                let call = MirCall::protocol(
                    MirCallTarget::Direct(callable),
                    bray_bound_tree::BoundCallResult::LazyFuture(
                        bray_bound_tree::BoundFutureConstruction::new(result, future),
                    ),
                    [receiver],
                    [],
                );

                (
                    MirOperationKind::Async(MirAsyncOperation::CreateFrame {
                        frame: MirFrameReference::Erased,
                        initializer: MirFrameInitializer::Callable(call),
                    }),
                    future,
                )
            }
        };

        let result = builder
            .push_operation(block, source.clone(), operation, Some(operation_result))
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        let operation = result.operation();

        result.result().ok_or_else(|| {
            ProductQueryFailure::missing(
                ProductQueryContext::MirOperation {
                    source: source.clone(),
                    operation,
                },
                ProductDataKind::OperationResultType,
            )
            .into()
        })
    }

    fn lifecycle_receiver_operand(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        receiver: TypeId,
    ) -> Result<MirOperand, CodegenPreparationError> {
        let values = self.semantic_value_store()?;

        let receiver_data = values
            .type_data(receiver)
            .map_err(FactQueryError::SemanticValueStore)?;

        match receiver_data.as_ref() {
            TypeData::Borrow { kind, .. } => {
                let value = builder
                    .push_operation(
                        block,
                        source.clone(),
                        MirOperationKind::Borrow { kind: *kind, place },
                        Some(receiver),
                    )
                    .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

                let operation = value.operation();

                let value = value.result().ok_or_else(|| {
                    ProductQueryFailure::missing(
                        ProductQueryContext::MirOperation {
                            source: source.clone(),
                            operation,
                        },
                        ProductDataKind::OperationResultType,
                    )
                })?;

                Ok(MirOperand::Value(value))
            }
            TypeData::Error
            | TypeData::Named { .. }
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. }
            | TypeData::Tuple(_)
            | TypeData::Array { .. }
            | TypeData::FlexibleArray(_)
            | TypeData::Slice(_)
            | TypeData::Generator(_)
            | TypeData::Nullable(_)
            | TypeData::TraitView(_)
            | TypeData::OwnedIndirection { .. }
            | TypeData::Callable(_) => Ok(MirOperand::Move(place)),
        }
    }
}
