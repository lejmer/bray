use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirAsyncOperation, MirCall, MirCallTarget, MirCallableReference, MirFrameInitializer,
    MirFrameReference, MirOperand, MirOperationKind, MirPlace, MirSourceAnchor, MirUnitBuilder,
};
use bray_symbols::{CallableExecution, TypeData, TypeId};

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(super) fn push_lifecycle_operation(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        operation: MirOperationKind,
    ) -> Result<(), C::Error> {
        builder
            .push_operation(block, source.clone(), operation, None)
            .map_err(|cause| self.mir_error(source, cause))?;

        Ok(())
    }

    pub(super) fn push_lifecycle_call(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: (MirCallableReference, TypeId, TypeId, CallableExecution),
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let (callable, receiver, result, _) = callable;

        let receiver = self.lifecycle_receiver_operand(builder, block, source, place, receiver)?;
        let outcome = self.cleanup_outcome(builder, block, source)?;

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
            .map_err(|cause| self.mir_error(source, cause))?;

        let completed = outcome
            .check(builder, block, source)
            .map_err(|cause| self.mir_error(source, cause))?;

        self.finish_cleanup_outcome(builder, completed, source, &outcome)
    }

    pub(super) fn push_static_finalizer_call(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: (MirCallableReference, TypeId, TypeId, CallableExecution),
    ) -> Result<bray_ir::MirValueId, C::Error> {
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
                    .context
                    .compiler_known_symbols()
                    .unary_representation_type(
                        self.context.semantic_values(),
                        RepresentationRole::Future,
                        result,
                    )
                    .map_err(SyntheticLoweringError::SemanticValue)?
                    .ok_or_else(|| SyntheticLoweringError::MissingRepresentation {
                        role: RepresentationRole::Future,
                        argument: Some(result),
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
            .map_err(|cause| self.mir_error(source, cause))?;

        let operation = result.operation();

        result.result().ok_or_else(|| {
            SyntheticLoweringError::MissingOperationResult {
                source: source.clone(),
                operation,
            }
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
    ) -> Result<MirOperand, C::Error> {
        let values = self.context.semantic_values();

        let receiver_data = values
            .type_data(receiver)
            .map_err(SyntheticLoweringError::SemanticValue)?;

        match receiver_data.as_ref() {
            TypeData::Borrow { kind, .. } => {
                let value = builder
                    .push_operation(
                        block,
                        source.clone(),
                        MirOperationKind::Borrow { kind: *kind, place },
                        Some(receiver),
                    )
                    .map_err(|cause| self.mir_error(source, cause))?;

                let operation = value.operation();

                let value = value.result().ok_or_else(|| {
                    SyntheticLoweringError::MissingOperationResult {
                        source: source.clone(),
                        operation,
                    }
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
