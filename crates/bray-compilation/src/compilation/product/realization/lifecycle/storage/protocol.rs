use bray_bound_tree::BoundCallResult;
use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
use bray_ir::{
    MirAsyncOperation, MirCall, MirCallTarget, MirCallableReference, MirHelperReference,
    MirMemoryOperation, MirOperand, MirOperationKind, MirPlace, MirProjectionKind,
    MirRuntimeReference, MirSourceAnchor, MirStorageKind, MirStoreKind, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::{BorrowKind, CallableSignature, TypeData, TypeId};

use super::super::super::super::super::CodegenPreparationError;
use super::super::super::super::super::Compilation;
use super::super::super::super::super::{
    ProductDataKind, ProductQueryContext, ProductQueryFailure, ProductValueKind,
};
use super::super::super::support::projected_lifecycle_place;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    #[expect(
        clippy::too_many_arguments,
        reason = "storage projection retains the selected policy and target type"
    )]
    pub(in crate::compilation::product::realization) fn storage_target_place(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        storage_place: MirPlace,
        storage: TypeId,
        target: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<MirPlace, CodegenPreparationError> {
        let borrowed = self.push_storage_lifecycle_call(
            builder,
            block,
            source,
            storage_place,
            storage,
            target,
            "StorageBorrowMut",
            Some(BorrowKind::Mutable),
            cancellation,
        )?;

        let values = self.semantic_value_store()?;

        let pointer = values
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target,
            })
            .map_err(FactQueryError::SemanticValueStore)?;

        let temporary = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, pointer)
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

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

        Ok(projected_lifecycle_place(
            &temporary_place,
            MirProjectionKind::Dereference,
            target,
        ))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "storage protocol calls retain exact policy, target, and MIR placement"
    )]
    pub(in crate::compilation::product::realization) fn push_storage_lifecycle_call(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        storage_place: MirPlace,
        storage: TypeId,
        target: TypeId,
        member: &str,
        borrow: Option<BorrowKind>,
        cancellation: &CancellationToken,
    ) -> Result<bray_ir::MirValueId, CodegenPreparationError> {
        let member = CompilerKnownDeclarationKey::try_new(member).ok_or_else(|| {
            ProductQueryFailure::InvalidCompilerKnownDeclarationKey {
                key: member.to_owned(),
            }
        })?;

        let (callable, signature) =
            self.storage_lifecycle_callable(storage, target, &member, cancellation)?;

        let [parameter] = signature.parameters() else {
            return Err(ProductQueryFailure::count_mismatch(
                ProductQueryContext::CompilerKnownDeclaration(member.clone()),
                ProductDataKind::CallableParameters,
                1,
                signature.parameters().len(),
            )
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

    pub(in crate::compilation::product::realization) fn storage_lifecycle_callable(
        &self,
        storage: TypeId,
        target: TypeId,
        member: &CompilerKnownDeclarationKey,
        cancellation: &CancellationToken,
    ) -> Result<(MirCallableReference, CallableSignature), CodegenPreparationError> {
        let binding_context = self.binding_context(cancellation)?;

        let selected = super::super::super::super::super::operation::selected_storage_callable(
            self,
            &binding_context,
            storage,
            target,
            member,
            cancellation,
        )?;

        if selected.diagnostics().has_errors() {
            return Err(CodegenPreparationError::UnsupportedType(storage));
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

    pub(in crate::compilation::product::realization) fn push_compiler_known_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        reference: &MirHelperReference,
        place: &MirPlace,
        runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
        cancellation: &CancellationToken,
    ) -> Result<bool, CodegenPreparationError> {
        let Some(ty) = reference.lifecycle_type() else {
            return Ok(false);
        };

        let values = self.semantic_value_store()?;

        let data = values
            .type_data(ty)
            .map_err(FactQueryError::SemanticValueStore)?;

        let TypeData::Named {
            definition,
            substitution,
        } = data.as_ref()
        else {
            return Ok(false);
        };

        if let MirHelperReference::Destroy(_) = reference
            && let Some(element) =
                self.imported_raw_buffer_element(*definition, *substitution, cancellation)?
        {
            let borrowed = values
                .intern_type(TypeData::Borrow {
                    kind: BorrowKind::Mutable,
                    target: ty,
                })
                .map_err(FactQueryError::SemanticValueStore)?;

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
                .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

            let operation = buffer.operation();

            let buffer = buffer.result().ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::MirOperation {
                        source: source.clone(),
                        operation,
                    },
                    ProductDataKind::OperationResultType,
                )
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

        let Some(role) = super::super::super::super::super::foreign::compiler_known_representation(
            self,
            *definition,
        ) else {
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
                let status = self.codegen_representation_type(RepresentationRole::ScalarU32)?;

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
                    .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;
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
                return Err(CodegenPreparationError::MissingHelperInstance(
                    reference.clone(),
                ));
            }
        }

        Ok(true)
    }
}
