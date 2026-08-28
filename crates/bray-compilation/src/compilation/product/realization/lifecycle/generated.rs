use bray_binder::SymbolQueryProvider;
use bray_ir::{
    MirBlockKind, MirEdge, MirGeneratorOperation, MirHelperReference, MirOperand, MirOperationKind,
    MirPlace, MirProjectionKind, MirRuntimeReference, MirSourceAnchor, MirTerminatorKind,
    MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::{
    BorrowKind, GenericSubstitutionId, NamedTypeSymbolId, SymbolQueryRequest,
    TypeAssociatedLifecycleSlot, TypeData, TypeId, UnionPayloadFieldTypeQuery,
};

use super::super::super::super::CodegenPreparationError;
use super::super::super::super::Compilation;
use super::super::support::{lifecycle_operation_block_kind, projected_lifecycle_place};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation::product::realization) fn push_generated_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        reference: &MirHelperReference,
        place: MirPlace,
        runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
        cancellation: &CancellationToken,
    ) -> Result<bray_ir::MirBlockId, CodegenPreparationError> {
        if self.push_compiler_known_lifecycle_operations(
            builder,
            block,
            source,
            reference,
            &place,
            runtime_abi,
            cancellation,
        )? {
            return Ok(block);
        }

        match reference {
            MirHelperReference::Finalize(ty) | MirHelperReference::StaticFinalize(ty) => {
                if let Some(callable) = self.lifecycle_callable(
                    *ty,
                    TypeAssociatedLifecycleSlot::Finalizer,
                    cancellation,
                )? {
                    self.push_lifecycle_call(builder, block, source, place, callable)?;
                }
            }
            MirHelperReference::Destroy(ty) => {
                if let Some(callable) = self.lifecycle_callable(
                    *ty,
                    TypeAssociatedLifecycleSlot::Destructor,
                    cancellation,
                )? {
                    self.push_lifecycle_call(builder, block, source, place.clone(), callable)?;
                }

                return self.push_represented_lifecycle_operations(
                    builder,
                    block,
                    source,
                    bray_ir::MirGeneratedLifecycleRole::Destroy,
                    place,
                    runtime_abi,
                    cancellation,
                );
            }
            MirHelperReference::Cleanup {
                phase: bray_ir::MirCleanupPhase::TaskCancellation,
                ..
            } => {
                return self.push_represented_lifecycle_operations(
                    builder,
                    block,
                    source,
                    bray_ir::MirGeneratedLifecycleRole::Cleanup(
                        bray_ir::MirCleanupPhase::TaskCancellation,
                    ),
                    place,
                    runtime_abi,
                    cancellation,
                );
            }
            MirHelperReference::Cleanup {
                phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                ..
            } => {
                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Finalize(place.clone()),
                )?;

                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Destroy(place),
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

        Ok(block)
    }

    pub(in crate::compilation::product::realization) fn push_represented_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
        cancellation: &CancellationToken,
    ) -> Result<bray_ir::MirBlockId, CodegenPreparationError> {
        let values = self.semantic_value_store()?;

        let data = values
            .type_data(place.ty())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        match data.as_ref() {
            TypeData::Named {
                definition: NamedTypeSymbolId::Union(union),
                substitution,
            } => self.push_union_lifecycle_operations(
                builder,
                block,
                source,
                role,
                place,
                *union,
                *substitution,
                cancellation,
            ),
            TypeData::Nullable(target) => self
                .push_nullable_lifecycle_operations(builder, block, source, role, place, *target),
            TypeData::Generator(element) => {
                let operation = match role {
                    bray_ir::MirGeneratedLifecycleRole::Destroy => MirGeneratorOperation::Destroy {
                        destination: place,
                        element: *element,
                        runtime: MirRuntimeReference::new(
                            RuntimeAbiRole::GeneratorDestruction,
                            runtime_abi,
                        ),
                    },
                    bray_ir::MirGeneratedLifecycleRole::Cleanup(
                        bray_ir::MirCleanupPhase::TaskCancellation,
                    ) => MirGeneratorOperation::CleanupBroadcast {
                        destination: place,
                        element: *element,
                        runtime: MirRuntimeReference::new(
                            RuntimeAbiRole::GeneratorCleanupBroadcast,
                            runtime_abi,
                        ),
                    },
                    bray_ir::MirGeneratedLifecycleRole::Finalize
                    | bray_ir::MirGeneratedLifecycleRole::StaticFinalize
                    | bray_ir::MirGeneratedLifecycleRole::Cleanup(
                        bray_ir::MirCleanupPhase::LifecycleResolution,
                    ) => return Err(FactQueryError::InfrastructureFailure.into()),
                };

                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Generator(operation),
                )?;

                Ok(block)
            }
            TypeData::OwnedIndirection { storage, target } => self
                .push_owned_indirection_lifecycle_operations(
                    builder,
                    block,
                    source,
                    role,
                    place,
                    *storage,
                    *target,
                    cancellation,
                ),
            TypeData::Error
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. }
            | TypeData::FlexibleArray(_)
            | TypeData::Slice(_)
            | TypeData::TraitView(_) => Err(CodegenPreparationError::UnsupportedType(place.ty())),
            TypeData::Named { .. } | TypeData::Tuple(_) | TypeData::Array { .. } => {
                let children = self.lifecycle_children(place, cancellation)?;

                self.push_child_lifecycle_operations(builder, block, source, role, children)?;

                Ok(block)
            }
            TypeData::Borrow { .. } | TypeData::Callable(_) => {
                Err(FactQueryError::InfrastructureFailure.into())
            }
        }
    }

    pub(in crate::compilation::product::realization) fn push_nullable_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        target: TypeId,
    ) -> Result<bray_ir::MirBlockId, CodegenPreparationError> {
        let kind = lifecycle_operation_block_kind(role)?;

        let present = builder
            .push_block(source.clone(), kind)
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        let absent = builder
            .push_block(source.clone(), kind)
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        let merge = builder
            .push_block(source.clone(), kind)
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        builder
            .set_terminator(
                block,
                source.clone(),
                MirTerminatorKind::PatternBranch {
                    subject: MirOperand::Copy(place.clone()),
                    predicate: bray_ir::MirPatternPredicate::NullablePresent,
                    matched: MirEdge::new(present, []),
                    unmatched: MirEdge::new(absent, []),
                },
            )
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        let child = projected_lifecycle_place(&place, MirProjectionKind::NullableValue, target);

        self.push_child_lifecycle_operations(builder, present, source, role, [child])?;

        for branch in [present, absent] {
            builder
                .set_terminator(
                    branch,
                    source.clone(),
                    MirTerminatorKind::Goto(MirEdge::new(merge, [])),
                )
                .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;
        }

        Ok(merge)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "union lifecycle dispatch keeps its checked type and MIR context explicit"
    )]
    pub(in crate::compilation::product::realization) fn push_union_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        union: bray_symbols::UnionSymbolId,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
    ) -> Result<bray_ir::MirBlockId, CodegenPreparationError> {
        let kind = lifecycle_operation_block_kind(role)?;

        let merge = builder
            .push_block(source.clone(), kind)
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        let binding_context = self.binding_context(cancellation)?;

        let union = binding_context
            .union(union)
            .map_err(super::super::super::super::binder::binding_query_error)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let mut current = block;

        for variant in union.variants() {
            let variant_record = binding_context
                .union_variant(*variant)
                .map_err(super::super::super::super::binder::binding_query_error)?
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let matched = builder
                .push_block(source.clone(), kind)
                .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

            let unmatched = builder
                .push_block(source.clone(), kind)
                .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

            builder
                .set_terminator(
                    current,
                    source.clone(),
                    MirTerminatorKind::PatternBranch {
                        subject: MirOperand::Copy(place.clone()),
                        predicate: bray_ir::MirPatternPredicate::ActiveUnionVariant(*variant),
                        matched: MirEdge::new(matched, []),
                        unmatched: MirEdge::new(unmatched, []),
                    },
                )
                .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

            let children = variant_record
                .payload_fields()
                .iter()
                .map(|field| {
                    let template = binding_context
                        .resolve_symbol_query(
                            SymbolQueryRequest::<UnionPayloadFieldTypeQuery>::new(*field),
                        )
                        .map_err(super::super::super::super::binder::binding_query_error)?;

                    let ty =
                        self.resolve_codegen_type(template.value(), substitution, cancellation)?;

                    Ok(projected_lifecycle_place(
                        &place,
                        MirProjectionKind::ActiveUnionPayloadField {
                            variant: *variant,
                            field: *field,
                        },
                        ty,
                    ))
                })
                .collect::<Result<Vec<_>, FactQueryError>>()?;

            self.push_child_lifecycle_operations(builder, matched, source, role, children)?;

            builder
                .set_terminator(
                    matched,
                    source.clone(),
                    MirTerminatorKind::Goto(MirEdge::new(merge, [])),
                )
                .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

            current = unmatched;
        }

        let unmatched = if kind == MirBlockKind::CleanupBroadcast {
            MirTerminatorKind::Goto(MirEdge::new(merge, []))
        } else {
            MirTerminatorKind::Unreachable
        };

        builder
            .set_terminator(current, source.clone(), unmatched)
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        Ok(merge)
    }

    pub(in crate::compilation::product::realization) fn push_child_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        children: impl IntoIterator<Item = MirPlace>,
    ) -> Result<(), CodegenPreparationError> {
        for child in children.into_iter().collect::<Vec<_>>().into_iter().rev() {
            match role {
                bray_ir::MirGeneratedLifecycleRole::Destroy => {
                    self.push_lifecycle_operation(
                        builder,
                        block,
                        source,
                        MirOperationKind::Finalize(child.clone()),
                    )?;

                    self.push_lifecycle_operation(
                        builder,
                        block,
                        source,
                        MirOperationKind::Destroy(child),
                    )?;
                }
                bray_ir::MirGeneratedLifecycleRole::Cleanup(phase) => {
                    self.push_lifecycle_operation(
                        builder,
                        block,
                        source,
                        MirOperationKind::Cleanup {
                            phase,
                            place: child,
                        },
                    )?;
                }
                bray_ir::MirGeneratedLifecycleRole::Finalize
                | bray_ir::MirGeneratedLifecycleRole::StaticFinalize => {
                    return Err(FactQueryError::InfrastructureFailure.into());
                }
            }
        }

        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "owned lifecycle realization keeps the storage policy and target explicit"
    )]
    pub(in crate::compilation::product::realization) fn push_owned_indirection_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        storage: TypeId,
        target: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<bray_ir::MirBlockId, CodegenPreparationError> {
        let storage_place =
            projected_lifecycle_place(&place, MirProjectionKind::OwnedStorage, storage);

        let target_place = self.storage_target_place(
            builder,
            block,
            source,
            storage_place.clone(),
            storage,
            target,
            cancellation,
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
                    cancellation,
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
                    cancellation,
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
                return Err(FactQueryError::InfrastructureFailure.into());
            }
        }

        Ok(block)
    }
}
