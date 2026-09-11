use bray_bound_tree::BoundCallResult;
use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
use bray_ir::{
    MirAsyncOperation, MirCall, MirCallTarget, MirEdge, MirHelperReference, MirOperand,
    MirOperationKind, MirPlace, MirProjectionKind, MirRuntimeReference, MirSourceAnchor,
    MirTerminatorKind, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::{BorrowKind, TypeData, TypeId};

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};
use crate::cleanup_outcome::CleanupOutcome;

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    #[expect(
        clippy::too_many_arguments,
        reason = "storage projection retains the selected policy and target type"
    )]
    fn checked_storage_target_place(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        storage_place: MirPlace,
        storage: TypeId,
        target: TypeId,
        outcome: &CleanupOutcome,
        failed: bray_ir::MirBlockId,
    ) -> Result<(bray_ir::MirBlockId, MirPlace), C::Error> {
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

        let (completed, temporary_place) = outcome
            .check_value(builder, block, source, borrowed, failed)
            .map_err(|cause| self.mir_error(source, cause))?;

        Ok((
            completed,
            temporary_place.project(MirProjectionKind::Dereference, target),
        ))
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
    ) -> Result<Option<bray_ir::MirBlockId>, C::Error> {
        let Some(ty) = reference.lifecycle_type() else {
            return Ok(None);
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
            return Ok(None);
        };

        if self
            .context
            .raw_buffer_element(*definition, *substitution)?
            .is_some()
        {
            let role = bray_ir::MirGeneratedLifecycleRole::from_reference(reference)
                .ok_or_else(|| SyntheticLoweringError::MissingHelper(reference.clone()))?;

            return self
                .push_represented_lifecycle_operations(builder, block, source, role, place.clone())
                .map(Some);
        }

        let Some(role) = self.context.representation_role(*definition) else {
            return Ok(None);
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

            return Ok(Some(block));
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

            return Ok(Some(block));
        }

        if role == RepresentationRole::Future {
            return match reference {
                MirHelperReference::Destroy(_)
                | MirHelperReference::Cleanup {
                    phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                    ..
                } => self
                    .resolve_inactive_future(builder, block, source, place.clone())
                    .map(Some),
                MirHelperReference::Finalize(_)
                | MirHelperReference::StaticFinalize(_)
                | MirHelperReference::Cleanup {
                    phase: bray_ir::MirCleanupPhase::TaskCancellation,
                    ..
                } => Ok(Some(block)),
                _ => Err(SyntheticLoweringError::MissingHelper(reference.clone()).into()),
            };
        }

        if role != RepresentationRole::Task {
            return Ok(None);
        }

        match reference {
            MirHelperReference::Finalize(_) | MirHelperReference::StaticFinalize(_) => {
                let outcome = self.cleanup_outcome(builder, block, source)?;

                let completion = self.task_completion_type(place.ty())?;

                let block = self.push_task_resolution(
                    builder,
                    block,
                    source,
                    place.clone(),
                    completion,
                    &outcome,
                )?;

                return self
                    .finish_cleanup_outcome(builder, block, source, &outcome)
                    .map(Some);
            }
            MirHelperReference::Destroy(_) => {
                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Async(MirAsyncOperation::DestroyTerminalTask {
                        task: MirOperand::Move(place.clone()),
                        completion: None,
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
                        task: MirOperand::Copy(place.clone()),
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
                let outcome = self.cleanup_outcome(builder, block, source)?;

                let completion = self.task_completion_type(place.ty())?;

                let block = self.push_task_resolution(
                    builder,
                    block,
                    source,
                    place.clone(),
                    completion,
                    &outcome,
                )?;

                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Async(MirAsyncOperation::DestroyTerminalTask {
                        task: MirOperand::Move(place.clone()),
                        completion: None,
                    }),
                )?;

                return self
                    .finish_cleanup_outcome(builder, block, source, &outcome)
                    .map(Some);
            }
            MirHelperReference::AnonymousCallable(_)
            | MirHelperReference::Abandon { .. }
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
            | MirHelperReference::ComposeAwaitedFrame(_)
            | MirHelperReference::DestroyTerminalTask => {
                return Err(SyntheticLoweringError::MissingHelper(reference.clone()).into());
            }
        }

        Ok(Some(block))
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
        outcome: &crate::cleanup_outcome::CleanupOutcome,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let storage_place = place.project(MirProjectionKind::OwnedStorage, storage);

        let kind = builder
            .block_kind(block)
            .map_err(|cause| self.mir_error(source, cause))?;

        let after_target = builder
            .push_block(source.clone(), kind)
            .map_err(|cause| self.mir_error(source, cause))?;

        let (block, target_place) = self.checked_storage_target_place(
            builder,
            block,
            source,
            storage_place.clone(),
            storage,
            target,
            outcome,
            after_target,
        )?;

        let operation = match role {
            bray_ir::MirGeneratedLifecycleRole::Abandon(bray_ir::MirAbandonmentAction::Quiesce) => {
                MirOperationKind::Abandon {
                    action: bray_ir::MirAbandonmentAction::Quiesce,
                    place: target_place,
                }
            }
            bray_ir::MirGeneratedLifecycleRole::Abandon(_) => {
                return Err(SyntheticLoweringError::UnsupportedLifecycleRole(role).into());
            }
            bray_ir::MirGeneratedLifecycleRole::Destroy => MirOperationKind::Finalize(target_place),
            bray_ir::MirGeneratedLifecycleRole::Cleanup(phase) => MirOperationKind::Cleanup {
                phase,
                place: target_place,
            },
            bray_ir::MirGeneratedLifecycleRole::Finalize
            | bray_ir::MirGeneratedLifecycleRole::StaticFinalize => {
                return Err(SyntheticLoweringError::UnsupportedLifecycleRole(role).into());
            }
        };

        let block = self.resolve_lifecycle_action(builder, block, source, operation, outcome)?;

        builder
            .set_terminator(
                block,
                source.clone(),
                MirTerminatorKind::Goto(MirEdge::new(after_target, [])),
            )
            .map_err(|cause| self.mir_error(source, cause))?;

        let mut block = after_target;

        if role == bray_ir::MirGeneratedLifecycleRole::Destroy {
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

            block = outcome
                .check(builder, block, source)
                .map_err(|cause| self.mir_error(source, cause))?;

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

            block = outcome
                .check(builder, block, source)
                .map_err(|cause| self.mir_error(source, cause))?;
        }

        Ok(block)
    }
}
