use bray_ir::{
    MirBlockKind, MirEdge, MirHelperReference, MirOperand, MirOperationKind, MirPlace,
    MirProjectionKind, MirSourceAnchor, MirTerminatorKind, MirUnitBuilder,
};
use bray_symbols::{
    GenericSubstitutionId, NamedTypeSymbolId, TypeAssociatedLifecycleSlot, TypeData, TypeId,
};

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};
use super::support::lifecycle_operation_block_kind;

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(super) fn push_generated_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        reference: &MirHelperReference,
        place: MirPlace,
        runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        if let MirHelperReference::Abandon { action, .. } = reference {
            return self.push_abandonment_operations(
                builder,
                block,
                source,
                *action,
                place,
                runtime_abi,
            );
        }

        if let Some(completed) = self.push_compiler_known_lifecycle_operations(
            builder,
            block,
            source,
            reference,
            &place,
            runtime_abi,
        )? {
            return Ok(completed);
        }

        match reference {
            MirHelperReference::Finalize(ty) | MirHelperReference::StaticFinalize(ty) => {
                if self.context.finalization_complete(*ty)? {
                    return Ok(block);
                }

                if let Some(callable) = self
                    .context
                    .lifecycle_callable(*ty, TypeAssociatedLifecycleSlot::Finalizer)?
                {
                    return self.push_lifecycle_call(builder, block, source, place, callable);
                }
            }
            MirHelperReference::Destroy(ty) => {
                if let Some(callable) = self
                    .context
                    .lifecycle_callable(*ty, TypeAssociatedLifecycleSlot::Destructor)?
                {
                    // The consuming destructor body resolves its checked initialized remainder.
                    return self.push_lifecycle_call(builder, block, source, place, callable);
                }

                return self.push_represented_lifecycle_operations(
                    builder,
                    block,
                    source,
                    bray_ir::MirGeneratedLifecycleRole::Destroy,
                    place,
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
                );
            }
            MirHelperReference::Cleanup {
                phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                ..
            } => {
                // Finalization and destruction independently retain the same destination path.
                return self.resolve_lifecycle_sequence(
                    builder,
                    block,
                    source,
                    [
                        MirOperationKind::Finalize(place.clone()),
                        MirOperationKind::Destroy(place),
                    ],
                );
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

        Ok(block)
    }

    pub(super) fn push_represented_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let values = self.context.semantic_values();

        let data = values
            .type_data(place.ty())
            .map_err(SyntheticLoweringError::SemanticValue)?;

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
            ),
            TypeData::Nullable(target) => self
                .push_nullable_lifecycle_operations(builder, block, source, role, place, *target),
            TypeData::Array { element, length } => {
                self.push_array_lifecycle(builder, block, source, role, place, *element, *length)
            }
            TypeData::Generator(element) => {
                self.push_buffer_lifecycle(builder, block, source, role, place, *element)
            }
            TypeData::OwnedIndirection { storage, target } => self
                .push_owned_indirection_lifecycle_operations(
                    builder, block, source, role, place, *storage, *target,
                ),
            TypeData::Error
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. }
            | TypeData::FlexibleArray(_)
            | TypeData::Slice(_)
            | TypeData::TraitView(_) => {
                Err(SyntheticLoweringError::UnsupportedType(place.ty()).into())
            }
            TypeData::Named { .. } | TypeData::Tuple(_) => {
                let children = self.lifecycle_children(place)?;

                self.push_child_lifecycle_operations(builder, block, source, role, children)
            }
            TypeData::Borrow { .. } | TypeData::Callable(_) => {
                Err(SyntheticLoweringError::UnexpectedLifecycleType {
                    ty: place.ty(),
                    actual: data.as_ref().clone(),
                }
                .into())
            }
        }
    }

    pub(super) fn push_nullable_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        target: TypeId,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let kind = lifecycle_operation_block_kind(role, builder)?;

        let present = builder
            .push_block(source.clone(), kind)
            .map_err(|cause| self.mir_error(source, cause))?;

        let absent = builder
            .push_block(source.clone(), kind)
            .map_err(|cause| self.mir_error(source, cause))?;

        let merge = builder
            .push_block(source.clone(), kind)
            .map_err(|cause| self.mir_error(source, cause))?;

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
            .map_err(|cause| self.mir_error(source, cause))?;

        let child = place.project(MirProjectionKind::NullableValue, target);

        let present =
            self.push_child_lifecycle_operations(builder, present, source, role, [child])?;

        for branch in [present, absent] {
            builder
                .set_terminator(
                    branch,
                    source.clone(),
                    MirTerminatorKind::Goto(MirEdge::new(merge, [])),
                )
                .map_err(|cause| self.mir_error(source, cause))?;
        }

        Ok(merge)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "union lifecycle dispatch keeps its checked type and MIR context explicit"
    )]
    pub(super) fn push_union_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        union: bray_symbols::UnionSymbolId,
        substitution: GenericSubstitutionId,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let kind = lifecycle_operation_block_kind(role, builder)?;

        let merge = builder
            .push_block(source.clone(), kind)
            .map_err(|cause| self.mir_error(source, cause))?;

        let representation = self
            .context
            .declared_representation(NamedTypeSymbolId::Union(union))?;

        let bray_symbols::DeclaredStorageShape::Union(variants) = representation.storage() else {
            return Err(SyntheticLoweringError::UnresolvedType(place.ty()).into());
        };

        let mut current = block;

        for variant in variants.iter() {
            let matched = builder
                .push_block(source.clone(), kind)
                .map_err(|cause| self.mir_error(source, cause))?;

            let unmatched = builder
                .push_block(source.clone(), kind)
                .map_err(|cause| self.mir_error(source, cause))?;

            builder
                .set_terminator(
                    current,
                    source.clone(),
                    MirTerminatorKind::PatternBranch {
                        subject: MirOperand::Copy(place.clone()),
                        predicate: bray_ir::MirPatternPredicate::ActiveUnionVariant(
                            variant.variant(),
                        ),
                        matched: MirEdge::new(matched, []),
                        unmatched: MirEdge::new(unmatched, []),
                    },
                )
                .map_err(|cause| self.mir_error(source, cause))?;

            let children = variant
                .members()
                .iter()
                .enumerate()
                .map(|(index, member)| {
                    let ty = self.context.resolve_type(member.ty(), substitution)?;

                    let projection = MirProjectionKind::ActiveUnionPayloadElement {
                        variant: variant.variant(),
                        ordinal: bray_symbols::SymbolOrdinal::new(
                            u32::try_from(index)
                                .map_err(|_| SyntheticLoweringError::LayoutOverflow(place.ty()))?,
                        ),
                    };

                    Ok(place.project(projection, ty))
                })
                .collect::<Result<Vec<_>, C::Error>>()?;

            let matched =
                self.push_child_lifecycle_operations(builder, matched, source, role, children)?;

            builder
                .set_terminator(
                    matched,
                    source.clone(),
                    MirTerminatorKind::Goto(MirEdge::new(merge, [])),
                )
                .map_err(|cause| self.mir_error(source, cause))?;

            current = unmatched;
        }

        let unmatched = if kind == MirBlockKind::CleanupBroadcast {
            MirTerminatorKind::Goto(MirEdge::new(merge, []))
        } else {
            MirTerminatorKind::Unreachable
        };

        builder
            .set_terminator(current, source.clone(), unmatched)
            .map_err(|cause| self.mir_error(source, cause))?;

        Ok(merge)
    }

    pub(super) fn push_child_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        children: impl IntoIterator<Item = MirPlace, IntoIter: DoubleEndedIterator>,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let mut operations = Vec::new();

        for child in children.into_iter().rev() {
            operations.extend(
                child_lifecycle_operations(role, child)?
                    .into_iter()
                    .flatten(),
            );
        }

        self.resolve_lifecycle_sequence(builder, block, source, operations)
    }
}

pub(super) fn child_lifecycle_operations(
    role: bray_ir::MirGeneratedLifecycleRole,
    child: MirPlace,
) -> Result<[Option<MirOperationKind>; 2], SyntheticLoweringError> {
    let operations = match role {
        // Finalization and destruction retain the same represented child path.
        bray_ir::MirGeneratedLifecycleRole::Destroy => [
            Some(MirOperationKind::Finalize(child.clone())),
            Some(MirOperationKind::Destroy(child)),
        ],
        bray_ir::MirGeneratedLifecycleRole::Cleanup(phase) => [
            Some(MirOperationKind::Cleanup {
                phase,
                place: child,
            }),
            None,
        ],
        bray_ir::MirGeneratedLifecycleRole::Abandon(action) => [
            Some(MirOperationKind::Abandon {
                action,
                place: child,
            }),
            None,
        ],
        bray_ir::MirGeneratedLifecycleRole::Finalize
        | bray_ir::MirGeneratedLifecycleRole::StaticFinalize => {
            return Err(SyntheticLoweringError::UnsupportedLifecycleRole(role));
        }
    };

    Ok(operations)
}
