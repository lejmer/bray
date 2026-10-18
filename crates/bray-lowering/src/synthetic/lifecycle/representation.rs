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
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        reference: &MirHelperReference,
        place: MirPlace,
        runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let role =
            bray_ir::MirGeneratedLifecycleRole::from_reference(reference).unwrap_or_else(|| {
                panic!(
                    "synthetic lowering contract violation: MissingHelper {value:?}",
                    value = reference.clone()
                )
            });

        let action = self.context.lifecycle_action(place.ty(), role.phase())?;

        self.expand_lifecycle_action(builder, block, source, role, place, runtime_abi, action)
    }

    pub(super) fn expand_lifecycle_action(
        &mut self,
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
            LifecycleAction::Call(callable) => self
                .push_checked_lifecycle_call(builder, block, source, place, callable)
                .map(|(block, _)| block),
            LifecycleAction::Resolve => self.push_owner_resolution(builder, block, source, place),
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
                release,
            } => self.push_owned_indirection_lifecycle_operations(
                builder, block, source, role, place, storage, target, borrow, release,
            ),
            LifecycleAction::Generator(element) => {
                let collection = Box::new(
                    self.collection
                        .as_ref()
                        .expect("generator traversal borrows its enclosing cleanup collection")
                        .collection(),
                );

                let operation = match role {
                    bray_ir::MirGeneratedLifecycleRole::Destroy => MirGeneratorOperation::Destroy {
                        destination: place,
                        element,
                        collection,
                        abandoned: self
                            .abandoned
                            .clone()
                            .expect("generator destruction retains its invocation argument"),
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
                        collection,
                        runtime: MirRuntimeReference::new(
                            RuntimeAbiRole::GeneratorCleanupBroadcast,
                            runtime_abi,
                        ),
                    },
                    _ => panic!("lifecycle representation does not support role {role:?}"),
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
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        target: TypeId,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let kind = lifecycle_operation_block_kind(role);

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
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        variants: &[(bray_symbols::UnionVariantSymbolId, std::sync::Arc<[TypeId]>)],
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let kind = lifecycle_operation_block_kind(role);

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
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        children: impl IntoIterator<Item = Result<MirPlace, C::Error>, IntoIter: DoubleEndedIterator>,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let mut children = children.into_iter().rev().peekable();

        if children.peek().is_none() {
            return Ok(block);
        }

        let outcome = self.cleanup_outcome(builder, block, source)?;
        let mut block = block;

        for child in children {
            block = self
                .push_child_lifecycle_operation(builder, block, source, role, child?, &outcome)?;
        }

        self.finish_cleanup_outcome(builder, block, source, &outcome, &mut None)
    }

    pub(super) fn push_child_lifecycle_operation(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        child: MirPlace,
        outcome: &crate::cleanup_outcome::CleanupOutcome,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let operation = match role {
            bray_ir::MirGeneratedLifecycleRole::Quiesce => MirOperationKind::Quiesce {
                place: child,
                collection: None,
            },
            bray_ir::MirGeneratedLifecycleRole::Destroy => {
                let inherited = self
                    .abandoned
                    .as_ref()
                    .expect("generated destruction retains its invocation argument");

                let abandoned = builder
                    .push_block(source.clone(), MirBlockKind::Ordinary)
                    .map_err(|cause| self.capacity_error(cause))?;

                let ordinary = builder
                    .push_block(source.clone(), MirBlockKind::Ordinary)
                    .map_err(|cause| self.capacity_error(cause))?;

                let continuation = builder
                    .push_block(source.clone(), MirBlockKind::Ordinary)
                    .map_err(|cause| self.capacity_error(cause))?;

                builder.set_terminator(
                    block,
                    source.clone(),
                    MirTerminatorKind::Branch {
                        condition: inherited.clone(),
                        then_edge: MirEdge::new(abandoned, []),
                        else_edge: MirEdge::new(ordinary, []),
                    },
                );

                self.push_lifecycle_operation(
                    builder,
                    abandoned,
                    source,
                    MirOperationKind::Destroy {
                        collection: None,
                        place: child.clone(),
                        abandoned: Some(inherited.clone()),
                    },
                )?;

                self.push_lifecycle_operation(
                    builder,
                    ordinary,
                    source,
                    MirOperationKind::Cleanup {
                        collection: None,
                        phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                        place: child,
                    },
                )?;

                for end in [abandoned, ordinary] {
                    let end = outcome
                        .check(builder, end, source)
                        .map_err(|cause| self.capacity_error(cause))?;

                    builder.set_terminator(
                        end,
                        source.clone(),
                        MirTerminatorKind::Goto(MirEdge::new(continuation, [])),
                    );
                }

                return Ok(continuation);
            }
            bray_ir::MirGeneratedLifecycleRole::Cleanup(phase) => MirOperationKind::Cleanup {
                collection: None,
                phase,
                place: child,
            },
            bray_ir::MirGeneratedLifecycleRole::StaticFinalize => {
                panic!("lifecycle cleanup sequence does not support role {role:?}")
            }
        };

        self.push_lifecycle_operation(builder, block, source, operation)?;

        outcome
            .check(builder, block, source)
            .map_err(|cause| self.capacity_error(cause))
    }
}
