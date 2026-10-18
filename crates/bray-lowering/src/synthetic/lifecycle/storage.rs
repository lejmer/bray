use bray_bound_tree::BoundCallResult;
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirAsyncOperation, MirCall, MirCallTarget, MirMemoryOperation, MirOperand, MirOperationKind,
    MirPlace, MirProjectionKind, MirRuntimeReference, MirSourceAnchor, MirStorageKind,
    MirStoreKind, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::{BorrowKind, TypeData, TypeId};

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(super) fn storage_target_place(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        storage_place: MirPlace,
        target: TypeId,
        borrow: bray_bound_tree::LifecycleCallable,
    ) -> Result<(bray_ir::MirBlockId, MirPlace), C::Error> {
        let borrowed =
            self.push_selected_call(builder, block, source, storage_place, borrow, borrow.result)?;

        let (block, borrowed) =
            self.check_lifecycle_value(builder, block, source, borrowed, borrow.result.ty())?;

        let temporary = builder
            .push_storage(
                source.clone(),
                MirStorageKind::Temporary,
                borrow.result.ty(),
            )
            .map_err(|cause| self.capacity_error(cause))?;

        let pointer = MirPlace::new(temporary, [], borrow.result.ty());

        self.push_lifecycle_operation(
            builder,
            block,
            source,
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: pointer.clone(),
                value: MirOperand::Value(borrowed),
            },
        )?;

        Ok((
            block,
            pointer.project(MirProjectionKind::Dereference, target),
        ))
    }

    fn push_buffer_prefix_lifecycle(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: &MirPlace,
        element: TypeId,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        use bray_bound_tree::CheckedMemoryOperationKind;

        let values = self.context.semantic_values();

        let slice = values
            .intern_type(TypeData::Slice(element))
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let borrowed = |target| {
            values
                .intern_type(TypeData::Borrow {
                    kind: BorrowKind::Mutable,
                    target,
                })
                .map_err(SyntheticLoweringError::SemanticValue)
        };

        let buffer_type = borrowed(place.ty())?;
        let prefix = borrowed(slice)?;

        let integer = self
            .context
            .representation_type(RepresentationRole::ScalarUsize)?;

        let mut emit = |operation, ty| {
            builder
                .push_operation(block, source.clone(), operation, Some(ty))
                .map(|commit| {
                    commit
                        .result()
                        .expect("prefix observation publishes its result")
                })
                .map_err(|cause| self.capacity_error(cause))
        };

        // The borrow observes the initialized range without transferring any element or backing.
        let buffer = emit(
            MirOperationKind::Borrow {
                kind: BorrowKind::Mutable,
                place: place.clone(),
            },
            buffer_type,
        )?;

        let prefix_value = emit(
            MirOperationKind::Memory(Box::new(MirMemoryOperation::new(
                CheckedMemoryOperationKind::RawBufferInitializedSliceMut,
                [MirOperand::Value(buffer)],
                [buffer_type],
                Some(prefix),
            ))),
            prefix,
        )?;

        let length = emit(
            MirOperationKind::Memory(Box::new(MirMemoryOperation::new(
                CheckedMemoryOperationKind::SequenceLength,
                [MirOperand::Value(prefix_value)],
                [prefix],
                Some(integer),
            ))),
            integer,
        )?;

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, prefix)
            .map_err(|cause| self.capacity_error(cause))?;

        let prefix_place = MirPlace::new(storage, [], prefix);

        self.push_lifecycle_operation(
            builder,
            block,
            source,
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: prefix_place.clone(),
                value: MirOperand::Value(prefix_value),
            },
        )?;

        self.push_sequence_lifecycle(
            builder,
            block,
            source,
            role,
            prefix_place.project(MirProjectionKind::Dereference, slice),
            element,
            MirOperand::Value(length),
        )
    }

    pub(super) fn push_selected_runtime_lifecycle(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: &MirPlace,
        runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
        action: bray_bound_tree::LifecycleAction,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let ty = place.ty();

        match action {
            bray_bound_tree::LifecycleAction::RawBuffer(element)
                if matches!(
                    role,
                    bray_ir::MirGeneratedLifecycleRole::Quiesce
                        | bray_ir::MirGeneratedLifecycleRole::Cleanup(
                            bray_ir::MirCleanupPhase::TaskCancellation
                        )
                ) =>
            {
                return self
                    .push_buffer_prefix_lifecycle(builder, block, source, role, place, element);
            }
            bray_bound_tree::LifecycleAction::RawBuffer(element) => {
                let outcome = self.cleanup_outcome(builder, block, source)?;
                let values = self.context.semantic_values();

                let borrowed = values
                    .intern_type(TypeData::Borrow {
                        kind: BorrowKind::Mutable,
                        target: ty,
                    })
                    .map_err(SyntheticLoweringError::SemanticValue)?;

                let buffer = builder
                    .push_operation(
                        block,
                        source.clone(),
                        MirOperationKind::Borrow {
                            kind: BorrowKind::Mutable,
                            place: place.clone(),
                        },
                        Some(borrowed),
                    )
                    .map_err(|cause| self.capacity_error(cause))?;

                let buffer = buffer
                    .result()
                    .expect("value-producing MIR operation must publish a result");

                let release = MirMemoryOperation::new(
                    bray_bound_tree::CheckedMemoryOperationKind::RawBufferRelease { element },
                    [MirOperand::Value(buffer)],
                    [borrowed],
                    None,
                )
                .with_abandoned_cleanup(
                    self.abandoned
                        .clone()
                        .expect("buffer destruction retains its invocation argument"),
                );

                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Memory(Box::new(release)),
                )?;

                let block = outcome
                    .check(builder, block, source)
                    .map_err(|cause| self.capacity_error(cause))?;

                return self.finish_cleanup_outcome(builder, block, source, &outcome, &mut None);
            }
            bray_bound_tree::LifecycleAction::ReleaseString => {
                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Text(bray_ir::MirTextOperation::new(
                        bray_ir::MirTextOperationKind::Release,
                        [MirOperand::Move(place.clone())],
                        [ty],
                        None,
                    )),
                )?;
            }
            bray_bound_tree::LifecycleAction::DestroyReport => {
                let status = self
                    .context
                    .representation_type(RepresentationRole::ScalarU32)?;

                let call = MirCall::protocol(
                    MirCallTarget::Runtime(MirRuntimeReference::new(
                        RuntimeAbiRole::PanicReportDestruction,
                        runtime_abi,
                    )),
                    BoundCallResult::Immediate(status),
                    [MirOperand::Move(place.clone())],
                    [],
                );

                builder
                    .push_operation(
                        block,
                        source.clone(),
                        MirOperationKind::Call(call),
                        Some(status),
                    )
                    .map_err(|cause| self.capacity_error(cause))?;
            }
            bray_bound_tree::LifecycleAction::Task => match role {
                bray_ir::MirGeneratedLifecycleRole::Quiesce
                | bray_ir::MirGeneratedLifecycleRole::StaticFinalize
                | bray_ir::MirGeneratedLifecycleRole::Cleanup(
                    bray_ir::MirCleanupPhase::LifecycleResolution,
                ) => {
                    let outcome = self.cleanup_outcome(builder, block, source)?;

                    let block = self.push_task_resolution(
                        builder,
                        block,
                        source,
                        place.clone(),
                        runtime_abi,
                        &outcome,
                    )?;

                    if matches!(role, bray_ir::MirGeneratedLifecycleRole::Cleanup(_)) {
                        self.push_lifecycle_operation(
                            builder,
                            block,
                            source,
                            MirOperationKind::Async(MirAsyncOperation::DestroyTerminalTask {
                                task: MirOperand::Move(place.clone()),
                            }),
                        )?;
                    }

                    return self
                        .finish_cleanup_outcome(builder, block, source, &outcome, &mut None);
                }
                bray_ir::MirGeneratedLifecycleRole::Destroy => {
                    self.push_lifecycle_operation(
                        builder,
                        block,
                        source,
                        MirOperationKind::Async(MirAsyncOperation::DestroyTerminalTask {
                            task: MirOperand::Move(place.clone()),
                        }),
                    )?;
                }
                bray_ir::MirGeneratedLifecycleRole::Cleanup(
                    bray_ir::MirCleanupPhase::TaskCancellation,
                ) => {
                    self.push_lifecycle_operation(
                        builder,
                        block,
                        source,
                        MirOperationKind::Async(MirAsyncOperation::RequestTaskCancellation {
                            task: MirOperand::Move(place.clone()),
                            runtime: MirRuntimeReference::new(
                                RuntimeAbiRole::TaskCancellationRequest,
                                runtime_abi,
                            ),
                        }),
                    )?;
                }
            },
            _ => panic!("lifecycle storage does not support role {role:?}"),
        }

        Ok(block)
    }

    pub(super) fn push_owned_indirection_lifecycle_operations(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        storage: TypeId,
        target: TypeId,
        borrow: bray_bound_tree::LifecycleCallable,
        release: Option<bray_bound_tree::LifecycleCallable>,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let storage_place = place.project(MirProjectionKind::OwnedStorage, storage);

        let (block, target_place) = self.storage_target_place(
            builder,
            block,
            source,
            storage_place.clone(),
            target,
            borrow,
        )?;

        let Some(release) = release else {
            return self.push_child_lifecycle_operations(
                builder,
                block,
                source,
                role,
                [Ok(target_place)],
            );
        };

        let outcome = self.cleanup_outcome(builder, block, source)?;

        let block = self.push_child_lifecycle_operation(
            builder,
            block,
            source,
            role,
            target_place,
            &outcome,
        )?;

        self.push_selected_call(
            builder,
            block,
            source,
            storage_place,
            release,
            release.result,
        )?;

        let block = outcome
            .check(builder, block, source)
            .map_err(|cause| self.capacity_error(cause))?;

        self.finish_cleanup_outcome(builder, block, source, &outcome, &mut None)
    }
}
