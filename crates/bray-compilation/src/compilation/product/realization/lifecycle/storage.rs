use bray_binder::SymbolQueryProvider;
use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
use bray_ir::{
    MirAsyncOperation, MirCall, MirCallTarget, MirCallableReference, MirFrameInitializer,
    MirFrameReference, MirHelperReference, MirMemoryOperation, MirOperand, MirOperationKind,
    MirPlace, MirProjectionKind, MirRuntimeReference, MirSourceAnchor, MirStorageKind,
    MirStoreKind, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::{
    BorrowKind, CallableExecution, CallableSignature, CallableSignatureQuery, NamedTypeSymbolId,
    SymbolQueryRequest, TypeAssociatedLifecycleSlot, TypeData, TypeId,
};

use super::super::super::super::CodegenPreparationError;
use super::super::super::super::Compilation;
use super::super::support::{
    closed_array_length, projected_lifecycle_place, receiver_codegen_type,
};
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
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

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
        let member = CompilerKnownDeclarationKey::try_new(member)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let (callable, signature) =
            self.storage_lifecycle_callable(storage, target, &member, cancellation)?;

        let [parameter] = signature.parameters() else {
            return Err(FactQueryError::InfrastructureFailure.into());
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
                .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?
                .result()
                .ok_or(FactQueryError::InfrastructureFailure)?;

            MirOperand::Value(value)
        } else {
            MirOperand::Move(storage_place)
        };

        builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Call(MirCall::protocol(
                    MirCallTarget::Direct(callable),
                    bray_bound_tree::BoundCallResult::Immediate(signature.result()),
                    [argument],
                    [],
                )),
                Some(signature.result()),
            )
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?
            .result()
            .ok_or(FactQueryError::InfrastructureFailure.into())
    }

    pub(in crate::compilation::product::realization) fn storage_lifecycle_callable(
        &self,
        storage: TypeId,
        target: TypeId,
        member: &CompilerKnownDeclarationKey,
        cancellation: &CancellationToken,
    ) -> Result<(MirCallableReference, CallableSignature), CodegenPreparationError> {
        let binding_context = self.binding_context(cancellation)?;

        let selected = super::super::super::super::operation::selected_storage_callable(
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
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::Callable(callable_type) = callable_type.as_ref() else {
            return Err(FactQueryError::InfrastructureFailure.into());
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
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

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
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

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
                .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?
                .result()
                .ok_or(FactQueryError::InfrastructureFailure)?;

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

        let Some(role) =
            super::super::super::super::foreign::compiler_known_representation(self, *definition)
        else {
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

        if role != RepresentationRole::Task {
            return Ok(false);
        }

        match reference {
            MirHelperReference::Finalize(_) | MirHelperReference::StaticFinalize(_) => {
                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Async(MirAsyncOperation::ResolveTask {
                        task: MirOperand::Move(place.clone()),
                        runtime: MirRuntimeReference::new(
                            RuntimeAbiRole::JoinRegistration,
                            runtime_abi,
                        ),
                    }),
                )?;
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
                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Async(MirAsyncOperation::ResolveTask {
                        task: MirOperand::Move(place.clone()),
                        runtime: MirRuntimeReference::new(
                            RuntimeAbiRole::JoinRegistration,
                            runtime_abi,
                        ),
                    }),
                )?;

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
                    .ok_or(FactQueryError::InfrastructureFailure)?;

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

        builder
            .push_operation(block, source.clone(), operation, Some(operation_result))
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?
            .result()
            .ok_or(FactQueryError::InfrastructureFailure.into())
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
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        match receiver_data.as_ref() {
            TypeData::Borrow { kind, .. } => {
                let value = builder
                    .push_operation(
                        block,
                        source.clone(),
                        MirOperationKind::Borrow { kind: *kind, place },
                        Some(receiver),
                    )
                    .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?
                    .result()
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                Ok(MirOperand::Value(value))
            }
            TypeData::Error
            | TypeData::Named { .. }
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. }
            | TypeData::Tuple(_)
            | TypeData::Array { .. }
            | TypeData::Slice(_)
            | TypeData::Generator(_)
            | TypeData::Nullable(_)
            | TypeData::TraitView(_)
            | TypeData::OwnedIndirection { .. }
            | TypeData::Callable(_) => Ok(MirOperand::Move(place)),
        }
    }

    pub(in crate::compilation::product::realization) fn lifecycle_callable(
        &self,
        ty: TypeId,
        slot: TypeAssociatedLifecycleSlot,
        cancellation: &CancellationToken,
    ) -> Result<
        Option<(MirCallableReference, TypeId, TypeId, CallableExecution)>,
        CodegenPreparationError,
    > {
        let values = self.semantic_value_store()?;

        let data = values
            .type_data(ty)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::Named {
            definition,
            substitution,
        } = data.as_ref()
        else {
            return Ok(None);
        };

        let surface =
            self.type_associated_surface_result_with_cancellation(*definition, cancellation)?;

        let mut members = surface
            .value()
            .lifecycle_members()
            .iter()
            .filter(|member| member.slot() == slot)
            .map(|member| member.id());

        let Some(member) = members.next() else {
            return Ok(None);
        };

        if members.next().is_some() {
            return Err(FactQueryError::InfrastructureFailure.into());
        }

        let callable = super::super::super::super::implementation::callable_instance(
            values,
            member,
            [*substitution],
        )?;

        let binding_context = self.binding_context(cancellation)?;

        let signature = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                callable.definition().callable_symbol(),
            ))
            .map_err(super::super::super::super::binder::binding_query_error)?;

        let constants = self.checked_constant_terms_for_templates_with_cancellation(
            [
                signature.value().callable_type(),
                signature.value().result(),
            ],
            cancellation,
        )?;

        let signature = bray_checker::resolve_callable_signature_template(
            values,
            signature.value(),
            callable.substitution(),
            constants.value(),
        )
        .map_err(FactQueryError::CheckerInfrastructure)?
        .ok_or(FactQueryError::InfrastructureFailure)?;

        let receiver = signature
            .receiver()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let receiver_ty =
            self.concrete_codegen_type(receiver.ty(), Some(*substitution), None, cancellation)?;

        let receiver = receiver_codegen_type(values, receiver_ty, receiver.mode())?;

        let result = self.concrete_codegen_type(
            signature.result(),
            Some(*substitution),
            None,
            cancellation,
        )?;

        let callable_type = values
            .type_data(signature.callable_type())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::Callable(callable_type) = callable_type.as_ref() else {
            return Err(FactQueryError::InfrastructureFailure.into());
        };

        Ok(Some((
            MirCallableReference::new(callable, callable_type.abi()),
            receiver,
            result,
            callable_type.execution(),
        )))
    }

    pub(in crate::compilation::product::realization) fn lifecycle_children(
        &self,
        place: MirPlace,
        cancellation: &CancellationToken,
    ) -> Result<Vec<MirPlace>, CodegenPreparationError> {
        let values = self.semantic_value_store()?;

        let data = values
            .type_data(place.ty())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let children = match data.as_ref() {
            TypeData::Named {
                definition: NamedTypeSymbolId::Struct(structure),
                substitution,
            } => {
                if super::super::super::super::foreign::compiler_known_representation(
                    self,
                    NamedTypeSymbolId::Struct(*structure),
                )
                .is_some()
                {
                    return Err(CodegenPreparationError::UnsupportedType(place.ty()));
                }

                let binding_context = self.binding_context(cancellation)?;

                let structure = binding_context
                    .structure(*structure)
                    .map_err(super::super::super::super::binder::binding_query_error)?
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                structure
                    .fields()
                    .iter()
                    .map(|field| {
                        let template = binding_context
                            .resolve_symbol_query(SymbolQueryRequest::<
                                bray_symbols::StructFieldTypeQuery,
                            >::new(*field))
                            .map_err(super::super::super::super::binder::binding_query_error)?;

                        let ty = self.resolve_codegen_type(
                            template.value(),
                            *substitution,
                            cancellation,
                        )?;

                        Ok((
                            MirProjectionKind::Field(bray_ir::MirFieldReference::Struct(*field)),
                            ty,
                        ))
                    })
                    .collect::<Result<Vec<_>, FactQueryError>>()?
            }
            TypeData::Tuple(elements) => elements
                .iter()
                .copied()
                .enumerate()
                .map(|(index, ty)| {
                    let index = u32::try_from(index)
                        .map_err(|_| CodegenPreparationError::LayoutOverflow(place.ty()))?;

                    Ok((MirProjectionKind::TupleField(index), ty))
                })
                .collect::<Result<Vec<_>, CodegenPreparationError>>()?,
            TypeData::Array { element, length } => {
                let length = closed_array_length(values, *length)?;

                let length = u32::try_from(length)
                    .map_err(|_| CodegenPreparationError::LayoutOverflow(place.ty()))?;

                (0..length)
                    .map(|index| (MirProjectionKind::ElementFromStart(index), *element))
                    .collect()
            }
            TypeData::Error
            | TypeData::Named {
                definition: NamedTypeSymbolId::Union(_),
                ..
            }
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. }
            | TypeData::Slice(_)
            | TypeData::Generator(_)
            | TypeData::Nullable(_)
            | TypeData::Borrow { .. }
            | TypeData::TraitView(_)
            | TypeData::OwnedIndirection { .. }
            | TypeData::Callable(_) => {
                return Err(CodegenPreparationError::UnsupportedType(place.ty()));
            }
        };

        Ok(children
            .into_iter()
            .map(|(kind, ty)| projected_lifecycle_place(&place, kind, ty))
            .collect())
    }
}
