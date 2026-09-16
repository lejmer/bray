use bray_ir::{
    MirBlockKind, MirEdge, MirGeneratorOperation, MirHelperReference, MirOperand, MirOperationKind,
    MirPlace, MirProjectionKind, MirRuntimeReference, MirSourceAnchor, MirTerminatorKind,
    MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::TypeId;

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
        let role = bray_ir::MirGeneratedLifecycleRole::from_reference(reference)
            .ok_or_else(|| SyntheticLoweringError::MissingHelper(reference.clone()))?;

        let action = self
            .context
            .lifecycle_action(place.ty(), super::support::lifecycle_phase(role))?;

        self.expand_lifecycle_action(builder, block, source, role, place, runtime_abi, action)
    }

    pub(super) fn expand_lifecycle_action(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
        action: bray_bound_tree::LifecycleAction,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        use bray_bound_tree::LifecycleAction;

        match action {
            LifecycleAction::None => Ok(block),
            LifecycleAction::Call(callable) => {
                self.push_lifecycle_call(builder, block, source, place, callable)
            }
            LifecycleAction::Resolve => self.resolve_lifecycle_sequence(
                builder,
                block,
                source,
                [
                    MirOperationKind::Finalize(place.clone()),
                    MirOperationKind::Destroy(place),
                ],
            ),
            LifecycleAction::Members(members) => {
                let children = members.iter().copied().enumerate().map(|(index, ty)| {
                    let index = u32::try_from(index)
                        .map_err(|_| SyntheticLoweringError::LayoutOverflow(place.ty()))?;

                    Ok(place.project(MirProjectionKind::TupleField(index), ty))
                });

                self.push_child_lifecycle_operations(builder, block, source, role, children)
            }
            LifecycleAction::Alternatives(variants) => {
                self.push_union_lifecycle_operations(builder, block, source, role, place, &variants)
            }
            LifecycleAction::Nullable(target) => {
                self.push_nullable_lifecycle_operations(builder, block, source, role, place, target)
            }
            LifecycleAction::Array { element, length } => {
                self.push_array_lifecycle(builder, block, source, role, place, element, length)
            }
            LifecycleAction::Owned {
                storage,
                target,
                borrow,
                teardown,
            } => self.push_owned_indirection_lifecycle_operations(
                builder, block, source, place, storage, target, borrow, teardown,
            ),
            LifecycleAction::Generator(element) => {
                let operation = match role {
                    bray_ir::MirGeneratedLifecycleRole::Destroy => MirGeneratorOperation::Destroy {
                        destination: place,
                        element,
                        runtime: MirRuntimeReference::new(
                            RuntimeAbiRole::GeneratorDestruction,
                            runtime_abi,
                        ),
                    },
                    bray_ir::MirGeneratedLifecycleRole::Cleanup(
                        bray_ir::MirCleanupPhase::TaskCancellation,
                    ) => MirGeneratorOperation::CleanupBroadcast {
                        destination: place,
                        element,
                        runtime: MirRuntimeReference::new(
                            RuntimeAbiRole::GeneratorCleanupBroadcast,
                            runtime_abi,
                        ),
                    },
                    _ => return Err(SyntheticLoweringError::UnsupportedLifecycleRole(role).into()),
                };

                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Generator(operation),
                )?;

                Ok(block)
            }
            action @ (LifecycleAction::RawBuffer(_)
            | LifecycleAction::ReleaseString
            | LifecycleAction::DestroyReport
            | LifecycleAction::Task) => self.push_selected_runtime_lifecycle(
                builder,
                block,
                source,
                role,
                &place,
                runtime_abi,
                action,
            ),
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
        let kind = lifecycle_operation_block_kind(role)?;

        let present = builder
            .push_block(source.clone(), kind)
            .map_err(|cause| self.capacity_error(cause))?;

        let absent = builder
            .push_block(source.clone(), kind)
            .map_err(|cause| self.capacity_error(cause))?;

        let merge = builder
            .push_block(source.clone(), kind)
            .map_err(|cause| self.capacity_error(cause))?;

        builder.set_terminator(
            block,
            source.clone(),
            MirTerminatorKind::PatternBranch {
                subject: MirOperand::Copy(place.clone()),
                predicate: bray_ir::MirPatternPredicate::NullablePresent,
                matched: MirEdge::new(present, []),
                unmatched: MirEdge::new(absent, []),
            },
        );

        let child = place.project(MirProjectionKind::NullableValue, target);

        let present =
            self.push_child_lifecycle_operations(builder, present, source, role, [Ok(child)])?;

        for branch in [present, absent] {
            builder.set_terminator(
                branch,
                source.clone(),
                MirTerminatorKind::Goto(MirEdge::new(merge, [])),
            );
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
        variants: &[(bray_symbols::UnionVariantSymbolId, std::sync::Arc<[TypeId]>)],
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let kind = lifecycle_operation_block_kind(role)?;

        let merge = builder
            .push_block(source.clone(), kind)
            .map_err(|cause| self.capacity_error(cause))?;

        let mut current = block;

        for (variant, members) in variants {
            let matched = builder
                .push_block(source.clone(), kind)
            .map_err(|cause| self.capacity_error(cause))?;

            let unmatched = builder
                .push_block(source.clone(), kind)
            .map_err(|cause| self.capacity_error(cause))?;

            builder.set_terminator(
                current,
                source.clone(),
                MirTerminatorKind::PatternBranch {
                    subject: MirOperand::Copy(place.clone()),
                    predicate: bray_ir::MirPatternPredicate::ActiveUnionVariant(*variant),
                    matched: MirEdge::new(matched, []),
                    unmatched: MirEdge::new(unmatched, []),
                },
            );

            let children = members.iter().enumerate().map(|(index, member)| {
                let ty = *member;

                let projection = MirProjectionKind::ActiveUnionPayloadElement {
                    variant: *variant,
                    ordinal: bray_symbols::SymbolOrdinal::new(
                        u32::try_from(index)
                            .map_err(|_| SyntheticLoweringError::LayoutOverflow(place.ty()))?,
                    ),
                };

                Ok(place.project(projection, ty))
            });

            let matched =
                self.push_child_lifecycle_operations(builder, matched, source, role, children)?;

            builder.set_terminator(
                matched,
                source.clone(),
                MirTerminatorKind::Goto(MirEdge::new(merge, [])),
            );

            current = unmatched;
        }

        let unmatched = if kind == MirBlockKind::CleanupBroadcast {
            MirTerminatorKind::Goto(MirEdge::new(merge, []))
        } else {
            MirTerminatorKind::Unreachable
        };

        builder.set_terminator(current, source.clone(), unmatched);

        Ok(merge)
    }

    pub(super) fn push_child_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        children: impl IntoIterator<Item = Result<MirPlace, C::Error>, IntoIter: DoubleEndedIterator>,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let mut operations = Vec::new();

        for child in children.into_iter().rev() {
            operations.extend(
                child_lifecycle_operations(role, child?)?
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
    match role {
        bray_ir::MirGeneratedLifecycleRole::Destroy => {
            // Both lifecycle stages operate on the same represented child.
            Ok([
                Some(MirOperationKind::Finalize(child.clone())),
                Some(MirOperationKind::Destroy(child)),
            ])
        }
        bray_ir::MirGeneratedLifecycleRole::Cleanup(phase) => Ok([
            Some(MirOperationKind::Cleanup {
                phase,
                place: child,
            }),
            None,
        ]),
        bray_ir::MirGeneratedLifecycleRole::Finalize
        | bray_ir::MirGeneratedLifecycleRole::StaticFinalize => {
            Err(SyntheticLoweringError::UnsupportedLifecycleRole(role))
        }
    }
}
