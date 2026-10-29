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

        let (block, borrowed) = self.check_lifecycle_value(
            builder,
            block,
            source,
            borrowed,
            borrow.result.ty(),
            self.context.callable_completion_reports(borrow.callable)?,
        )?;

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
        &mut self,
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

        let buffer_storage = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, buffer_type)
            .map_err(|cause| self.capacity_error(cause))?;

        let buffer_place = MirPlace::new(buffer_storage, [], buffer_type);

        self.push_lifecycle_operation(
            builder,
            block,
            source,
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: buffer_place.clone(),
                value: MirOperand::Value(buffer),
            },
        )?;

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

        let prefix_value = emit(
            MirOperationKind::Memory(Box::new(MirMemoryOperation::new(
                CheckedMemoryOperationKind::RawBufferInitializedSliceMut,
                [MirOperand::Copy(buffer_place.clone())],
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
            matches!(
                role,
                bray_ir::MirGeneratedLifecycleRole::Destroy(_)
                    | bray_ir::MirGeneratedLifecycleRole::ReleaseBuffer
            )
            .then(|| MirOperand::Copy(buffer_place)),
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "runtime lifecycle emission joins the selected action and retirement with the ABI and MIR context"
    )]
    pub(super) fn push_selected_runtime_lifecycle(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: &MirPlace,
        runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
        action: bray_bound_tree::LifecycleAction,
        retirement: Option<MirOperationKind>,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let ty = place.ty();

        match action {
            bray_bound_tree::LifecycleAction::RawBuffer(element)
            | bray_bound_tree::LifecycleAction::Generator(element)
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
            bray_bound_tree::LifecycleAction::RawBuffer(element)
            | bray_bound_tree::LifecycleAction::Generator(element) => {
                let block = self
                    .push_buffer_prefix_lifecycle(builder, block, source, role, place, element)?;

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
                );

                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Memory(Box::new(release)),
                )?;

                let block = outcome
                    .check(builder, block, source, false)
                    .map_err(|cause| self.capacity_error(cause))?;

                return self.finish_cleanup_outcome(builder, block, source, &outcome, &mut None);
            }
            bray_bound_tree::LifecycleAction::ReleaseString => {
                let outcome = self.cleanup_outcome(builder, block, source)?;

                if let Some(retirement) = retirement {
                    self.push_lifecycle_operation(builder, block, source, retirement)?;
                }

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

                let block = outcome
                    .check(builder, block, source, false)
                    .map_err(|cause| self.capacity_error(cause))?;

                return self.finish_cleanup_outcome(builder, block, source, &outcome, &mut None);
            }
            bray_bound_tree::LifecycleAction::DestroyReport => {
                if let Some(retirement) = retirement {
                    self.push_lifecycle_operation(builder, block, source, retirement)?;
                }

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
                        if let Some(retirement) = retirement {
                            self.push_lifecycle_operation(builder, block, source, retirement)?;
                        }

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
                bray_ir::MirGeneratedLifecycleRole::ReleaseBuffer => {
                    unreachable!("buffer release selects its raw-buffer representation")
                }
                bray_ir::MirGeneratedLifecycleRole::Destroy(_) => {
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
                            task: place.clone(),
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

    #[expect(
        clippy::too_many_arguments,
        reason = "owned indirection emission retains storage and target types and selected borrow and release protocols"
    )]
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

        let outcome = self.cleanup_outcome(builder, block, source)?;
        let retirement = self.owner_retirement.take();

        let block = self.push_child_lifecycle_operation(
            builder,
            block,
            source,
            role,
            target_place,
            &outcome,
            retirement,
        )?;

        if let Some(release) = release {
            self.push_selected_call(
                builder,
                block,
                source,
                storage_place,
                release,
                release.result,
            )?;
        }

        let block = outcome
            .check(builder, block, source, false)
            .map_err(|cause| self.capacity_error(cause))?;

        self.finish_cleanup_outcome(builder, block, source, &outcome, &mut None)
    }
}

pub(super) fn buffer_cleanup_progress(
    builder: &MirUnitBuilder,
    buffer: MirOperand,
    count: MirOperand,
) -> MirOperationKind {
    let types = [builder.operand_type(&buffer), builder.operand_type(&count)];

    MirOperationKind::Memory(Box::new(MirMemoryOperation::new(
        bray_bound_tree::CheckedMemoryOperationKind::RawBufferSetInitializedCount,
        [buffer, count],
        types,
        None,
    )))
}
