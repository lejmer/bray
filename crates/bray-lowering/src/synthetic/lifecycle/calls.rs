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
        let (_, _, result, execution) = callable;

        let outcome = self.cleanup_outcome(builder, block, source)?;
        let value = self.push_static_finalizer_call(builder, block, source, place, callable)?;

        let completed = match execution {
            CallableExecution::Synchronous => {
                let invalid = |cause| self.mir_error(source, cause);
                let kind = builder.block_kind(block).map_err(invalid)?;
                let finished = builder.push_block(source.clone(), kind).map_err(invalid)?;

                let (completed, result) = outcome
                    .check_value(builder, block, source, value, finished)
                    .map_err(invalid)?;

                let completed = self.retain_finalizer_error(
                    builder,
                    completed,
                    source,
                    MirOperand::Move(result),
                    &outcome,
                )?;

                builder
                    .set_terminator(
                        completed,
                        source.clone(),
                        bray_ir::MirTerminatorKind::Goto(bray_ir::MirEdge::new(finished, [])),
                    )
                    .map_err(invalid)?;

                finished
            }
            CallableExecution::Asynchronous => {
                let (resumed, run_result, variants) = self.await_lifecycle_result(
                    builder,
                    block,
                    source,
                    crate::cleanup_await::CleanupAwait::Frame(
                        MirOperand::Value(value),
                        bray_ir::MirFrameEntry::Body,
                    ),
                    result,
                )?;

                // Preserve the completed payload path independently of the outcome tag branch.
                let (completed, finished) = outcome
                    .resolve_run_result(
                        builder,
                        resumed,
                        source,
                        run_result.clone(),
                        (
                            variants,
                            crate::cleanup_outcome::CleanupCancellation::Propagate,
                        ),
                    )
                    .map_err(|cause| self.mir_error(source, cause))?;

                let result = run_result.project(
                    bray_ir::MirProjectionKind::ActiveUnionPayloadElement {
                        variant: variants.completed(),
                        ordinal: bray_symbols::SymbolOrdinal::new(0),
                    },
                    result,
                );

                let completed = self.retain_finalizer_error(
                    builder,
                    completed,
                    source,
                    MirOperand::Move(result),
                    &outcome,
                )?;

                builder
                    .set_terminator(
                        completed,
                        source.clone(),
                        bray_ir::MirTerminatorKind::Goto(bray_ir::MirEdge::new(finished, [])),
                    )
                    .map_err(|cause| self.mir_error(source, cause))?;

                finished
            }
        };

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

    pub(super) fn lifecycle_receiver_operand(
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
