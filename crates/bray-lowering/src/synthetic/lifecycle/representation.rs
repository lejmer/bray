use bray_ir::{
    MirBlockKind, MirEdge, MirHelperReference, MirOperand, MirOperationKind, MirPlace,
    MirProjectionKind, MirRuntimeReference, MirSourceAnchor, MirTerminatorKind, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::TypeId;

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};

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

        self.expand_lifecycle_action(
            builder,
            block,
            source,
            role,
            place,
            runtime_abi,
            action,
            None,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "lifecycle expansion joins the checked action and retirement with the runtime ABI and MIR context"
    )]
    pub(super) fn expand_lifecycle_action(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
        action: bray_bound_tree::LifecycleAction,
        retirement: Option<MirOperationKind>,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        use bray_bound_tree::LifecycleAction;

        match action {
            LifecycleAction::None => {
                if let Some(retirement) = retirement {
                    self.push_lifecycle_operation(builder, block, source, retirement)?;
                }

                Ok(block)
            }
            LifecycleAction::Call(callable) => self
                .push_checked_lifecycle_call(builder, block, source, place, callable)
                .map(|(block, _)| block),
            LifecycleAction::Resolve => {
                self.push_owner_resolution(builder, block, source, place, retirement)
            }
            LifecycleAction::Future => {
                let outcome = self.cleanup_outcome(builder, block, source)?;

                let operation = match role {
                    bray_ir::MirGeneratedLifecycleRole::Quiesce => {
                        return self.compose_cleanup_frame(
                            builder,
                            block,
                            source,
                            MirOperand::Copy(place),
                            bray_ir::MirFrameComposition::Quiesce,
                            &outcome,
                        );
                    }
                    bray_ir::MirGeneratedLifecycleRole::Cleanup(
                        bray_ir::MirCleanupPhase::LifecycleResolution,
                    ) => {
                        if let Some(retirement) = retirement {
                            self.push_lifecycle_operation(builder, block, source, retirement)?;
                        }

                        if self.frame.is_some() {
                            return self.compose_cleanup_frame(
                                builder,
                                block,
                                source,
                                MirOperand::Move(place),
                                bray_ir::MirFrameComposition::Abandon,
                                &outcome,
                            );
                        }

                        // Synchronous represented-part helpers inherit the caller's
                        // checked admission of synchronous inactive captures.
                        bray_ir::MirAsyncOperation::ResolveInactiveFrame {
                            frame: MirOperand::Move(place),
                            runtime: MirRuntimeReference::new(
                                RuntimeAbiRole::InactiveFrameResolution,
                                runtime_abi,
                            ),
                        }
                    }
                    bray_ir::MirGeneratedLifecycleRole::Cleanup(
                        bray_ir::MirCleanupPhase::TaskCancellation,
                    ) => bray_ir::MirAsyncOperation::BroadcastInactiveFrame {
                        frame: MirOperand::Copy(place),
                        runtime: MirRuntimeReference::new(
                            RuntimeAbiRole::InactiveFrameBroadcast,
                            runtime_abi,
                        ),
                    },
                    bray_ir::MirGeneratedLifecycleRole::Destroy(_) => {
                        bray_ir::MirAsyncOperation::DestroyInactiveFrame {
                            frame: MirOperand::Move(place),
                            runtime: MirRuntimeReference::new(
                                RuntimeAbiRole::InactiveFrameDestruction,
                                runtime_abi,
                            ),
                        }
                    }
                    bray_ir::MirGeneratedLifecycleRole::ReleaseBuffer => {
                        unreachable!("buffer release selects its raw-buffer representation")
                    }
                    bray_ir::MirGeneratedLifecycleRole::StaticFinalize => {
                        unreachable!("inactive computations have no whole-value finalizer")
                    }
                };

                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Async(operation),
                )?;

                outcome
                    .check(builder, block, source, true)
                    .map_err(|cause| self.capacity_error(cause))
            }
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
            action @ (LifecycleAction::Generator(_)
            | LifecycleAction::RawBuffer(_)
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
                retirement,
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
        let kind = builder.block_kind(block);

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

    pub(super) fn push_union_lifecycle_operations(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        variants: &[(bray_symbols::UnionVariantSymbolId, std::sync::Arc<[TypeId]>)],
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let kind = builder.block_kind(block);

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
        children: impl IntoIterator<Item = Result<MirPlace, C::Error>>,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let children = children.into_iter().collect::<Result<Vec<_>, _>>()?;

        if children.is_empty() {
            return Ok(block);
        }

        let outcome = self.cleanup_outcome(builder, block, source)?;
        let mut remaining = outcome.clone();
        let mut retained = Vec::with_capacity(children.len());

        for (index, child) in children.iter().enumerate() {
            retained.push(remaining.clone());

            if index + 1 < children.len()
                && matches!(role, bray_ir::MirGeneratedLifecycleRole::Destroy(_))
            {
                // Each callback adds one still-owned sibling to a shared prefix.
                remaining = self.retain_cleanup_owner(builder, source, child, &remaining)?;
            }
        }

        let mut block = block;

        for (child, retained) in children.into_iter().zip(retained).rev() {
            block = self.push_child_lifecycle_operation(
                builder, block, source, role, child, &retained, None,
            )?;
        }

        self.finish_cleanup_outcome(builder, block, source, &outcome, &mut None)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "child lifecycle emission retains selected outcome and retirement with the MIR context"
    )]
    pub(super) fn push_child_lifecycle_operation(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        child: MirPlace,
        outcome: &crate::cleanup_outcome::CleanupOutcome,
        retirement: Option<MirOperationKind>,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        if retirement.is_some()
            && matches!(
                role,
                bray_ir::MirGeneratedLifecycleRole::Destroy(
                    bray_ir::MirDestructionDisposition::Ordinary
                )
            )
        {
            return self.push_child_resolution(builder, block, source, child, outcome, retirement);
        }

        let operation = match role {
            bray_ir::MirGeneratedLifecycleRole::Quiesce => MirOperationKind::Quiesce {
                place: child,
                collection: None,
            },
            bray_ir::MirGeneratedLifecycleRole::Destroy(
                bray_ir::MirDestructionDisposition::Abandoned,
            ) => MirOperationKind::Destroy {
                place: child,
                collection: None,
                abandoned: self.abandoned.clone(),
            },
            bray_ir::MirGeneratedLifecycleRole::Destroy(
                bray_ir::MirDestructionDisposition::Ordinary,
            ) => MirOperationKind::Cleanup {
                collection: None,
                phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                place: child,
            },
            bray_ir::MirGeneratedLifecycleRole::Destroy(
                bray_ir::MirDestructionDisposition::Dynamic,
            )
            | bray_ir::MirGeneratedLifecycleRole::ReleaseBuffer => {
                // Both branches retain the compact incoming disposition as frame states grow.
                let inherited = self
                    .abandoned
                    .clone()
                    .expect("generated destruction retains its invocation argument");

                let abandoned = builder
                    .push_block(source.clone(), builder.block_kind(block))
                    .map_err(|cause| self.capacity_error(cause))?;

                let ordinary = builder
                    .push_block(source.clone(), builder.block_kind(block))
                    .map_err(|cause| self.capacity_error(cause))?;

                let continuation = builder
                    .push_block(source.clone(), builder.block_kind(block))
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

                if let Some(retirement) = &retirement {
                    // Mutually exclusive paths share the same compact memory effect operands.
                    self.push_lifecycle_operation(builder, abandoned, source, retirement.clone())?;
                }

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

                let abandoned = outcome
                    .check(builder, abandoned, source, false)
                    .map_err(|cause| self.capacity_error(cause))?;

                let ordinary = if retirement.is_some() {
                    self.push_child_resolution(
                        builder, ordinary, source, child, outcome, retirement,
                    )?
                } else {
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

                    outcome
                        .check(builder, ordinary, source, false)
                        .map_err(|cause| self.capacity_error(cause))?
                };

                for end in [abandoned, ordinary] {
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

        if let Some(retirement) = retirement {
            self.push_lifecycle_operation(builder, block, source, retirement)?;
        }

        self.push_lifecycle_operation(builder, block, source, operation)?;

        outcome
            .check(builder, block, source, false)
            .map_err(|cause| self.capacity_error(cause))
    }

    fn push_child_resolution(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        child: MirPlace,
        outcome: &crate::cleanup_outcome::CleanupOutcome,
        retirement: Option<MirOperationKind>,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let action = self
            .context
            .lifecycle_action(child.ty(), bray_bound_tree::LifecyclePhase::Resolve)?;

        let collection = self.collection.replace(outcome.clone());

        let result = self.expand_lifecycle_action(
            builder,
            block,
            source,
            bray_ir::MirGeneratedLifecycleRole::Cleanup(
                bray_ir::MirCleanupPhase::LifecycleResolution,
            ),
            child,
            builder.target().runtime_abi(),
            action,
            retirement,
        );

        self.collection = collection;

        result
    }
}
