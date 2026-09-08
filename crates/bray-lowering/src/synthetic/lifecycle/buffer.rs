use bray_bound_tree::CheckedMemoryOperationKind;
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirAbandonmentAction, MirBinaryOperator, MirBlockId, MirCleanupPhase, MirEdge,
    MirGeneratedLifecycleRole, MirOperand, MirOperationKind, MirPlace, MirProjectionKind,
    MirSourceAnchor, MirStorageKind, MirStoreKind, MirTerminatorKind, MirUnitBuilder,
};
use bray_symbols::{BorrowKind, TypeData, TypeId};

use super::super::memory::push_memory;
use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};
use crate::cleanup_loop::ReverseCleanupLoop;
use crate::operand::integer_constant;

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    /// Resolves initialized elements of the shared pointer/capacity/initialized-count buffer layout.
    pub(super) fn push_buffer_lifecycle(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        role: MirGeneratedLifecycleRole,
        place: MirPlace,
        element: TypeId,
    ) -> Result<MirBlockId, C::Error> {
        let destroys = match role {
            MirGeneratedLifecycleRole::Destroy
            | MirGeneratedLifecycleRole::Cleanup(MirCleanupPhase::LifecycleResolution)
            | MirGeneratedLifecycleRole::Abandon(MirAbandonmentAction::Destroy) => true,
            MirGeneratedLifecycleRole::Cleanup(MirCleanupPhase::TaskCancellation)
            | MirGeneratedLifecycleRole::Abandon(MirAbandonmentAction::Quiesce) => false,
            MirGeneratedLifecycleRole::Finalize | MirGeneratedLifecycleRole::StaticFinalize => {
                return Ok(block);
            }
            MirGeneratedLifecycleRole::Abandon(MirAbandonmentAction::Destructor) => {
                return Err(SyntheticLoweringError::UnsupportedLifecycleRole(role).into());
            }
        };

        let values = self.context.semantic_values();
        let invalid = |cause| self.mir_error(source, cause);

        let usize_type = self
            .context
            .representation_type(RepresentationRole::ScalarUsize)?;

        let boolean = self
            .context
            .representation_type(RepresentationRole::ScalarBool)?;

        let borrowed = values
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target: place.ty(),
            })
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let buffer =
            self.lifecycle_receiver_operand(builder, block, source, place.clone(), borrowed)?;

        let slice = values
            .intern_type(TypeData::Slice(element))
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let slice_borrow = values
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target: slice,
            })
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let elements = push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::RawBufferInitializedSliceMut,
            [buffer.clone()],
            Some(slice_borrow),
        )
        .map_err(invalid)?
        .ok_or(SyntheticLoweringError::MissingTypeResult(slice_borrow))?;

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, slice_borrow)
            .map_err(invalid)?;

        let elements_place = MirPlace::new(storage, [], slice_borrow);

        builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Store {
                    kind: MirStoreKind::Initialize,
                    destination: elements_place.clone(),
                    value: elements,
                },
                None,
            )
            .map_err(invalid)?;

        let length = push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::RawBufferInitializedCount,
            [buffer.clone()],
            Some(usize_type),
        )
        .map_err(invalid)?
        .ok_or(SyntheticLoweringError::MissingTypeResult(usize_type))?;

        let zero = integer_constant(values, usize_type, 0)
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let one = integer_constant(values, usize_type, 1)
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let outcome = self.cleanup_outcome(builder, block, source)?;

        let cleanup =
            ReverseCleanupLoop::new(builder, block, source, length, boolean, [zero, one], None)
                .map_err(invalid)?;

        let element_place = elements_place
            .project(MirProjectionKind::Dereference, slice)
            .project(
                MirProjectionKind::Index(MirOperand::Copy(cleanup.counter.clone())),
                element,
            );

        if destroys {
            let buffer = self.lifecycle_receiver_operand(
                builder,
                cleanup.body,
                source,
                place.clone(),
                borrowed,
            )?;

            push_memory(
                builder,
                cleanup.body,
                source,
                CheckedMemoryOperationKind::RawBufferSetInitializedCount,
                [buffer, MirOperand::Copy(cleanup.counter.clone())],
                None,
            )
            .map_err(invalid)?;
        }

        let body = if role == MirGeneratedLifecycleRole::Destroy {
            self.resolve_lifecycle_action(
                builder,
                cleanup.body,
                source,
                MirOperationKind::Finalize(element_place.clone()),
                &outcome,
            )?
        } else {
            cleanup.body
        };

        let operation = match role {
            MirGeneratedLifecycleRole::Abandon(action) => MirOperationKind::Abandon {
                action,
                place: element_place,
            },
            MirGeneratedLifecycleRole::Cleanup(phase) => MirOperationKind::Cleanup {
                phase,
                place: element_place,
            },
            MirGeneratedLifecycleRole::Destroy => MirOperationKind::Destroy(element_place),
            _ => return Err(SyntheticLoweringError::UnsupportedLifecycleRole(role).into()),
        };

        let completed =
            self.resolve_lifecycle_action(builder, body, source, operation, &outcome)?;

        cleanup
            .close(builder, completed, source, None)
            .map_err(invalid)?;

        let completed = if destroys {
            self.release_buffer_storage(
                builder,
                cleanup.continuation,
                source,
                place,
                element,
                &outcome,
            )?
        } else {
            cleanup.continuation
        };

        self.finish_cleanup_outcome(builder, completed, source, &outcome)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "buffer release retains the owning cleanup outcome until allocation release completes"
    )]
    fn release_buffer_storage(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        element: TypeId,
        outcome: &crate::cleanup_outcome::CleanupOutcome,
    ) -> Result<MirBlockId, C::Error> {
        let values = self.context.semantic_values();
        let invalid = |cause| self.mir_error(source, cause);

        let usize_type = self
            .context
            .representation_type(RepresentationRole::ScalarUsize)?;

        let boolean = self
            .context
            .representation_type(RepresentationRole::ScalarBool)?;

        let pointer_type = self
            .context
            .compiler_known_symbols()
            .unary_representation_type(values, RepresentationRole::RawPointer, element)
            .map_err(SyntheticLoweringError::SemanticValue)?
            .ok_or(SyntheticLoweringError::MissingRepresentation {
                role: RepresentationRole::RawPointer,
                argument: Some(element),
            })?;

        let pointer = place.project(MirProjectionKind::TupleField(0), pointer_type);
        let capacity = place.project(MirProjectionKind::TupleField(1), usize_type);

        let zero = integer_constant(values, usize_type, 0)
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let present = builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Binary {
                    operator: MirBinaryOperator::NotEqual,
                    left: MirOperand::Copy(capacity.clone()),
                    right: zero.clone(),
                },
                Some(boolean),
            )
            .map_err(invalid)?;

        let present = present.result().ok_or_else(|| {
            invalid(bray_ir::MirUnitBuildError::MissingOperationResult(
                present.operation(),
            ))
        })?;

        let kind = builder.block_kind(block).map_err(invalid)?;
        let release = builder.push_block(source.clone(), kind).map_err(invalid)?;
        let done = builder.push_block(source.clone(), kind).map_err(invalid)?;

        builder
            .set_terminator(
                block,
                source.clone(),
                MirTerminatorKind::Branch {
                    condition: MirOperand::Value(present),
                    then_edge: MirEdge::new(release, []),
                    else_edge: MirEdge::new(done, []),
                },
            )
            .map_err(invalid)?;

        let [size, alignment] = self.push_memory_layout(builder, release, source, element)?;

        let bytes = builder
            .push_operation(
                release,
                source.clone(),
                MirOperationKind::Binary {
                    operator: MirBinaryOperator::Multiply,
                    left: MirOperand::Copy(capacity.clone()),
                    right: size,
                },
                Some(usize_type),
            )
            .map_err(invalid)?;

        let bytes = bytes.result().ok_or_else(|| {
            invalid(bray_ir::MirUnitBuildError::MissingOperationResult(
                bytes.operation(),
            ))
        })?;

        push_memory(
            builder,
            release,
            source,
            CheckedMemoryOperationKind::RawDeallocate,
            [
                MirOperand::Copy(pointer.clone()),
                MirOperand::Value(bytes),
                alignment,
            ],
            None,
        )
        .map_err(invalid)?;

        let release = outcome.check(builder, release, source).map_err(invalid)?;

        builder
            .set_terminator(
                release,
                source.clone(),
                MirTerminatorKind::Goto(MirEdge::new(done, [])),
            )
            .map_err(invalid)?;

        let null = push_memory(
            builder,
            done,
            source,
            CheckedMemoryOperationKind::Null { pointee: element },
            [],
            Some(pointer_type),
        )
        .map_err(invalid)?
        .ok_or(SyntheticLoweringError::MissingTypeResult(pointer_type))?;

        crate::raw_buffer::reset_raw_buffer(builder, done, source, place, null, zero)
            .map_err(invalid)?;

        Ok(done)
    }
}
