use std::hash::{Hash, Hasher};
use std::sync::Arc;

use bray_base::StableDigestHasher;
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCall, MirCallTarget, MirFrameDescriptor, MirFrameRetention,
    MirFrameState, MirFrameStateId, MirGeneratedLifecycleRole, MirOperand, MirOperationKind,
    MirPlace, MirSourceAnchor, MirStorageId, MirTerminatorKind, MirUnitBuilder,
};
use bray_symbols::{BorrowKind, TypeData, TypeId};

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};

pub(super) fn frame_identity(
    key: &bray_ir::MirUnitKey,
    target: &bray_ir::MirTargetContract,
) -> bray_runtime_interface::ProtectedAsyncFrameId {
    let mut hasher = StableDigestHasher::new();

    hasher.write(b"bray.lifecycle-frame");
    key.hash(&mut hasher);
    target.hash(&mut hasher);

    bray_runtime_interface::ProtectedAsyncFrameId::new(hasher.finalize())
}

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(super) fn begin_lifecycle_frame(
        &mut self,
        builder: &mut MirUnitBuilder,
        entry: MirBlockId,
        source: &MirSourceAnchor,
        owner: MirStorageId,
        role: MirGeneratedLifecycleRole,
    ) -> Result<(), C::Error> {
        // The borrowed graph remains broadcastable while cleanup is suspended.
        // Reuse the shared synchronous helper instead of copying its traversal.
        let broadcast = builder
            .push_block(source.clone(), MirBlockKind::CleanupBroadcast)
            .map_err(|cause| self.capacity_error(cause))?;

        let pointer = builder.storage_type(owner);

        let TypeData::Borrow { target, .. } = *self.context.semantic_values().type_data(pointer)
        else {
            panic!("cleanup frame retains a mutable owner borrow");
        };

        let place = MirPlace::new(owner, [], pointer)
            .project(bray_ir::MirProjectionKind::Dereference, target);

        let (work, finished) = if matches!(role, MirGeneratedLifecycleRole::Destroy(_))
            && matches!(
                self.context.semantic_values().type_data(target).as_ref(),
                TypeData::OwnedIndirection { .. }
            ) {
            let boolean = self
                .context
                .representation_type(RepresentationRole::ScalarBool)?;

            let guard = builder
                .push_storage(source.clone(), bray_ir::MirStorageKind::Temporary, boolean)
                .map_err(|cause| self.capacity_error(cause))?;

            let guard = MirPlace::new(guard, [], boolean);

            // Resume establishes the active-target guard before work. State zero
            // broadcasts the whole borrow; consuming destruction retires it once.
            self.push_lifecycle_operation(
                builder,
                entry,
                source,
                MirOperationKind::Store {
                    kind: bray_ir::MirStoreKind::Initialize,
                    destination: guard.clone(),
                    value: MirOperand::Immediate {
                        ty: boolean,
                        value: bray_ir::MirImmediateValue::Boolean(true),
                    },
                },
            )?;

            self.owner_retirement = Some(MirOperationKind::Store {
                kind: bray_ir::MirStoreKind::Assign,
                destination: guard.clone(),
                value: MirOperand::Immediate {
                    ty: boolean,
                    value: bray_ir::MirImmediateValue::Boolean(false),
                },
            });

            let work = builder
                .push_block(source.clone(), MirBlockKind::CleanupBroadcast)
                .map_err(|cause| self.capacity_error(cause))?;

            let finished = builder
                .push_block(source.clone(), MirBlockKind::CleanupBroadcast)
                .map_err(|cause| self.capacity_error(cause))?;

            builder.set_terminator(
                broadcast,
                source.clone(),
                MirTerminatorKind::Branch {
                    condition: MirOperand::Copy(guard),
                    then_edge: bray_ir::MirEdge::new(work, []),
                    else_edge: bray_ir::MirEdge::new(finished, []),
                },
            );

            (work, Some(finished))
        } else {
            (broadcast, None)
        };

        self.push_lifecycle_operation(
            builder,
            work,
            source,
            MirOperationKind::Cleanup {
                collection: None,
                phase: bray_ir::MirCleanupPhase::TaskCancellation,
                place,
            },
        )?;

        let broadcast_end = self
            .collection
            .as_ref()
            .expect("cleanup frame borrows its collection")
            .check(builder, work, source)
            .map_err(|cause| self.capacity_error(cause))?;

        let broadcast_end = if let Some(finished) = finished {
            builder.set_terminator(
                broadcast_end,
                source.clone(),
                MirTerminatorKind::Goto(bray_ir::MirEdge::new(finished, [])),
            );

            finished
        } else {
            broadcast_end
        };

        builder.set_terminator(
            broadcast_end,
            source.clone(),
            MirTerminatorKind::Return {
                value: None,
                report: None,
            },
        );

        let collection = self
            .collection
            .take()
            .expect("cleanup frame borrows its collection");

        let mut parameters = vec![owner];

        parameters.extend(collection.collection().places().map(MirPlace::storage));

        if let Some(MirOperand::Copy(place)) = &self.abandoned {
            parameters.push(place.storage());
        }

        let retention = MirFrameRetention::new(broadcast, [], parameters);

        // Before first resume the borrowed target is still whole. Active continuations
        // use the guard after consuming destruction has begun to retire that target.
        self.frame_states.push(MirFrameState::new(
            MirFrameStateId::new(0),
            entry,
            retention.clone().with_broadcast(work),
        ));

        self.collection = Some(collection.with_retention(Some(Arc::new(retention))));

        Ok(())
    }

    pub(super) fn retain_cleanup_owner(
        &self,
        builder: &mut MirUnitBuilder,
        source: &MirSourceAnchor,
        owner: &MirPlace,
        outcome: &crate::cleanup_outcome::CleanupOutcome,
    ) -> Result<crate::cleanup_outcome::CleanupOutcome, C::Error> {
        if self.frame.is_none()
            || matches!(
                self.context
                    .lifecycle_action(owner.ty(), bray_bound_tree::LifecyclePhase::Cancel)?,
                bray_bound_tree::LifecycleAction::None
            )
        {
            // The returned view shares the synchronous helper's borrowed collection.
            return Ok(outcome.clone());
        }

        let collection = outcome.collection();

        let retention = collection
            .retention()
            .expect("suspending cleanup retains its source contract");

        let broadcast = builder
            .push_block(source.clone(), MirBlockKind::CleanupBroadcast)
            .map_err(|cause| self.capacity_error(cause))?;

        self.push_lifecycle_operation(
            builder,
            broadcast,
            source,
            MirOperationKind::Cleanup {
                phase: bray_ir::MirCleanupPhase::TaskCancellation,
                // This whole owner stays initialized throughout its selected finalizer.
                place: owner.clone(),
                collection: None,
            },
        )?;

        let end = outcome
            .check(builder, broadcast, source)
            .map_err(|cause| self.capacity_error(cause))?;

        builder.set_terminator(
            end,
            source.clone(),
            MirTerminatorKind::Goto(bray_ir::MirEdge::new(retention.broadcast(), [])),
        );

        // Extend the entry while sharing the checked captures and requirements.
        Ok(outcome.clone().with_retention(Some(Arc::new(
            retention.as_ref().clone().with_broadcast(broadcast),
        ))))
    }

    pub(super) fn finish_lifecycle_frame(
        &mut self,
        builder: &mut MirUnitBuilder,
        source: &MirSourceAnchor,
    ) -> Result<(), C::Error> {
        let unit = self.context.representation_type(RepresentationRole::Unit)?;

        let exits = builder
            .terminated_blocks()
            .filter_map(|(block, terminal)| {
                (builder.block_kind(block) != MirBlockKind::CleanupBroadcast
                    && matches!(terminal.kind(), MirTerminatorKind::Return { .. }))
                .then_some(block)
            })
            .collect::<Vec<_>>();

        for block in exits {
            self.push_lifecycle_operation(
                builder,
                block,
                source,
                MirOperationKind::Async(bray_ir::MirAsyncOperation::PublishTerminalState {
                    state: bray_ir::MirTaskTerminalState::Completed {
                        value: MirOperand::Immediate {
                            ty: unit,
                            value: bray_ir::MirImmediateValue::Unit,
                        },
                        report: None,
                    },
                    runtime: bray_ir::MirRuntimeReference::new(
                        bray_runtime_interface::RuntimeAbiRole::TerminalPublication,
                        builder.target().runtime_abi(),
                    ),
                }),
            )?;
        }

        let abi = builder.target().runtime_abi();

        let descriptor = MirFrameDescriptor::new(
            self.frame.expect("cleanup frame has its stable identity"),
            abi,
            bray_runtime_interface::ProtectedFrameAbiVersions::uniform(abi),
            unit,
            None,
            std::mem::take(&mut self.frame_states),
        )
        .map_err(|cause| self.capacity_error(cause))?;

        builder.set_frame_descriptor(descriptor);

        Ok(())
    }

    pub(super) fn push_lifecycle_frame(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        role: MirGeneratedLifecycleRole,
        place: MirPlace,
    ) -> Result<MirBlockId, C::Error> {
        let role = match role {
            MirGeneratedLifecycleRole::Destroy(_) => {
                MirGeneratedLifecycleRole::Destroy(bray_ir::MirDestructionDisposition::Dynamic)
            }
            role => role,
        };

        let ty = place.ty();
        let outcome = self.cleanup_outcome(builder, block, source)?;
        let mut arguments = vec![self.borrow_frame_argument(builder, block, source, place)?];

        if matches!(
            role,
            MirGeneratedLifecycleRole::Destroy(_) | MirGeneratedLifecycleRole::ReleaseBuffer
        ) {
            arguments.push(
                self.abandoned
                    .clone()
                    .expect("destruction keeps its disposition"),
            );
        }

        for place in outcome.collection().places() {
            arguments.push(self.borrow_frame_argument(builder, block, source, place.clone())?);
        }

        let unit = self.context.representation_type(RepresentationRole::Unit)?;

        let future = self
            .context
            .compiler_known_symbols()
            .unary_representation_type(
                self.context.semantic_values(),
                RepresentationRole::Future,
                unit,
            )
            .map_err(SyntheticLoweringError::SemanticValue)?
            .expect("compiler-known Future represents cleanup frame completion");

        let call = MirCall::protocol(
            MirCallTarget::Lifecycle { role, ty },
            bray_bound_tree::BoundCallResult::LazyFuture(
                bray_bound_tree::BoundFutureConstruction::new(unit, future),
            ),
            arguments,
            [],
        )
        .with_cleanup(true);

        let value = builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame {
                    frame: bray_ir::MirFrameReference::Erased,
                    initializer: bray_ir::MirFrameInitializer::Callable(call),
                }),
                Some(future),
            )
            .map_err(|cause| self.capacity_error(cause))?
            .result()
            .expect("cleanup frame factory publishes its inactive frame");

        let (block, _) =
            self.await_lifecycle_call(builder, block, source, value, unit, &outcome)?;

        outcome
            .check(builder, block, source)
            .map_err(|cause| self.capacity_error(cause))
    }

    fn borrow_frame_argument(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
    ) -> Result<MirOperand, C::Error> {
        let pointer: TypeId = self
            .context
            .semantic_values()
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target: place.ty(),
            })
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let value = builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Borrow {
                    kind: BorrowKind::Mutable,
                    place,
                },
                Some(pointer),
            )
            .map_err(|cause| self.capacity_error(cause))?;

        Ok(MirOperand::Value(
            value
                .result()
                .expect("cleanup frame captures its owner borrow"),
        ))
    }
}
