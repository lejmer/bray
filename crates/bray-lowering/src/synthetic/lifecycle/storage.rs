use bray_bound_tree::BoundCallResult;
use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
use bray_ir::{
    MirAsyncOperation, MirCall, MirCallTarget, MirHelperReference, MirMemoryOperation, MirOperand,
    MirOperationKind, MirPlace, MirProjectionKind, MirRuntimeReference, MirSourceAnchor,
    MirStorageKind, MirStoreKind, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::{BorrowKind, TypeData, TypeId};

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    #[expect(
        clippy::too_many_arguments,
        reason = "storage projection retains the selected policy and target type"
    )]
    pub(super) fn storage_target_place(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        storage_place: MirPlace,
        storage: TypeId,
        target: TypeId,
    ) -> Result<MirPlace, C::Error> {
        let borrowed = self.push_storage_lifecycle_call(
            builder,
            block,
            source,
            storage_place,
            storage,
            target,
            "StorageBorrowMut",
            Some(BorrowKind::Mutable),
        )?;

        let values = self.context.semantic_values();

        let pointer = values
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target,
            })
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let temporary = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, pointer)
            .map_err(|cause| self.mir_error(source, cause))?;

        let temporary_place = MirPlace::new(temporary, [], pointer);

        self.push_lifecycle_operation(
            builder,
            block,
            source,
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: temporary_place.clone(),
                value: MirOperand::Value(borrowed),
            },
        )?;

        Ok(temporary_place.project(MirProjectionKind::Dereference, target))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "storage protocol calls retain exact policy, target, and MIR placement"
    )]
    pub(super) fn push_storage_lifecycle_call(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        storage_place: MirPlace,
        storage: TypeId,
        target: TypeId,
        member: &str,
        borrow: Option<BorrowKind>,
    ) -> Result<bray_ir::MirValueId, C::Error> {
        let member = CompilerKnownDeclarationKey::try_new(member)
            .ok_or_else(|| SyntheticLoweringError::InvalidStorageMemberKey(member.to_owned()))?;

        let (callable, signature) = self.context.storage_callable(storage, target, &member)?;

        let [parameter] = signature.parameters() else {
            return Err(SyntheticLoweringError::StorageParameterCount {
                member: member.clone(),
                actual: signature.parameters().len(),
            }
            .into());
        };

        let argument = if let Some(kind) = borrow {
            let value = builder
                .push_operation(
                    block,
                    source.clone(),
                    MirOperationKind::Borrow {
                        kind,
                        place: storage_place,
                    },
                    Some(parameter.ty()),
                )
                .map_err(|cause| self.mir_error(source, cause))?;

            let operation = value.operation();

            let value =
                value
                    .result()
                    .ok_or_else(|| SyntheticLoweringError::MissingOperationResult {
                        source: source.clone(),
                        operation,
                    })?;

            MirOperand::Value(value)
        } else {
            MirOperand::Move(storage_place)
        };

        let result = builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Call(MirCall::protocol(
                    MirCallTarget::Direct(callable),
                    BoundCallResult::Immediate(signature.result()),
                    [argument],
                    [],
                )),
                Some(signature.result()),
            )
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

    pub(super) fn push_compiler_known_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        reference: &MirHelperReference,
        place: &MirPlace,
        runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
    ) -> Result<bool, C::Error> {
        let Some(ty) = reference.lifecycle_type() else {
            return Ok(false);
        };

        let values = self.context.semantic_values();

        let data = values
            .type_data(ty)
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let TypeData::Named {
            definition,
            substitution,
        } = data.as_ref()
        else {
            return Ok(false);
        };

        if let MirHelperReference::Destroy(_) = reference
            && let Some(element) = self
                .context
                .imported_raw_buffer_element(*definition, *substitution)?
        {
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
                .map_err(|cause| self.mir_error(source, cause))?;

            let operation = buffer.operation();

            let buffer =
                buffer
                    .result()
                    .ok_or_else(|| SyntheticLoweringError::MissingOperationResult {
                        source: source.clone(),
                        operation,
                    })?;

            self.push_lifecycle_operation(
                builder,
                block,
                source,
                MirOperationKind::Memory(MirMemoryOperation::new(
                    bray_bound_tree::CheckedMemoryOperationKind::RawBufferRelease { element },
                    [MirOperand::Value(buffer)],
                    [borrowed],
                    None,
                )),
            )?;

            return Ok(true);
        }

        let Some(role) = self.context.representation_role(*definition) else {
            return Ok(false);
        };

        if role == RepresentationRole::String {
            if matches!(
                reference,
                MirHelperReference::Destroy(_)
                    | MirHelperReference::Cleanup {
                        phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                        ..
                    }
            ) {
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

            return Ok(true);
        }

        if role == RepresentationRole::PanicReport {
            if matches!(
                reference,
                MirHelperReference::Destroy(_)
                    | MirHelperReference::Cleanup {
                        phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                        ..
                    }
            ) {
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
                    .map_err(|cause| self.mir_error(source, cause))?;
            }

            return Ok(true);
        }

        if role != RepresentationRole::Task {
            return Ok(false);
        }

        match reference {
            MirHelperReference::Finalize(_) | MirHelperReference::StaticFinalize(_) => {
                self.push_task_resolution(builder, block, source, place.clone(), runtime_abi)?;
            }
            MirHelperReference::Destroy(_) => {
                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Async(MirAsyncOperation::DestroyTerminalTask {
                        task: MirOperand::Move(place.clone()),
                    }),
                )?;
            }
            MirHelperReference::Cleanup {
                phase: bray_ir::MirCleanupPhase::TaskCancellation,
                ..
            } => {
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
            MirHelperReference::Cleanup {
                phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                ..
            } => {
                self.push_task_resolution(builder, block, source, place.clone(), runtime_abi)?;

                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Async(MirAsyncOperation::DestroyTerminalTask {
                        task: MirOperand::Move(place.clone()),
                    }),
                )?;
            }
            MirHelperReference::AnonymousCallable(_)
            | MirHelperReference::DeclaredCallable(_)
            | MirHelperReference::CallableDefault(_)
            | MirHelperReference::ConstructionDefault(_)
            | MirHelperReference::TypeForm(_)
            | MirHelperReference::Conversion(_)
            | MirHelperReference::BeginGenerator
            | MirHelperReference::PushGenerator
            | MirHelperReference::FinishGenerator
            | MirHelperReference::PanicReport
            | MirHelperReference::StandardLibrary(_)
            | MirHelperReference::CreateFrame(_)
            | MirHelperReference::MoveInactiveFrame(_)
            | MirHelperReference::ComposeAwaitedFrame(_)
            | MirHelperReference::CommitAwaitedCompletion(_)
            | MirHelperReference::DestroyTerminalTask => {
                return Err(SyntheticLoweringError::MissingHelper(reference.clone()).into());
            }
        }

        Ok(true)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "owned lifecycle realization keeps the storage policy and target explicit"
    )]
    pub(super) fn push_owned_indirection_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        storage: TypeId,
        target: TypeId,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let storage_place = place.project(MirProjectionKind::OwnedStorage, storage);

        let target_place = self.storage_target_place(
            builder,
            block,
            source,
            storage_place.clone(),
            storage,
            target,
        )?;

        match role {
            bray_ir::MirGeneratedLifecycleRole::Destroy => {
                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Finalize(target_place),
                )?;

                self.push_storage_lifecycle_call(
                    builder,
                    block,
                    source,
                    storage_place.clone(),
                    storage,
                    target,
                    "StorageDestroy",
                    Some(BorrowKind::Mutable),
                )?;

                self.push_storage_lifecycle_call(
                    builder,
                    block,
                    source,
                    storage_place,
                    storage,
                    target,
                    "StorageRelease",
                    None,
                )?;
            }
            bray_ir::MirGeneratedLifecycleRole::Cleanup(phase) => {
                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Cleanup {
                        phase,
                        place: target_place,
                    },
                )?;
            }
            bray_ir::MirGeneratedLifecycleRole::Finalize
            | bray_ir::MirGeneratedLifecycleRole::StaticFinalize => {
                return Err(SyntheticLoweringError::UnsupportedLifecycleRole(role).into());
            }
        }

        Ok(block)
    }
}
