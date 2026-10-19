use bray_bound_tree::LifecycleCallable;
use bray_ir::{
    MirBlockId, MirCallableReference, MirCleanupPhase, MirEdge, MirOperand, MirOperationKind,
    MirPlace, MirSourceAnchor,
};

use crate::cleanup_outcome::CleanupOutcome;
use crate::lowering::LoweringError;
use crate::lowering::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn push_cleanup_action(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        place: MirPlace,
        release: Option<bray_bound_tree::StorageProtocolCall>,
        completed: bool,
    ) -> Result<MirBlockId, LoweringError> {
        if let Some(abandoned) = self
            .receiver_cleanup
            .as_ref()
            .and_then(|receiver| receiver.mode)
        {
            if phase == MirCleanupPhase::LifecycleResolution && release.is_none() {
                let operation = if abandoned
                    || completed
                    || self.input.type_finalizer_is_complete(place.ty())
                {
                    MirOperationKind::Destroy {
                        collection: None,
                        place,
                        abandoned: Some(MirOperand::Immediate {
                            value: bray_ir::MirImmediateValue::Boolean(abandoned),
                            ty: self.representation_type(
                                bray_compiler_known::RepresentationRole::ScalarBool,
                            )?,
                        }),
                    }
                } else {
                    MirOperationKind::Cleanup {
                        phase,
                        place,
                        collection: None,
                    }
                };

                return self.resolve_structural_cleanup(block, source, operation);
            }
        }

        self.push_ordinary_cleanup_action(block, source, phase, place, release, completed)
    }

    fn push_ordinary_cleanup_action(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        place: MirPlace,
        release: Option<bray_bound_tree::StorageProtocolCall>,
        completed: bool,
    ) -> Result<MirBlockId, LoweringError> {
        let owner = place.ty();
        let completed = completed || self.input.type_finalizer_is_complete(owner);

        if release.is_none()
            && phase == MirCleanupPhase::LifecycleResolution
            && self.input.unit_kind().protected_frame().is_some()
            && self.type_representation(owner)
                == Some(bray_compiler_known::RepresentationRole::Future)
        {
            return self.push_inactive_cleanup(block, source, place);
        }

        if release.is_none()
            && !completed
            && phase == MirCleanupPhase::LifecycleResolution
            && let Some(callable) = self.input.cleanup_type(owner).and_then(|ty| ty.finalizer())
        {
            return self.push_finalizer(block, source, place, callable);
        }

        if let Some(release) = release {
            self.set_storage_initialized(block, source, &place, false)?;
            self.push_storage_protocol_call(block, source, &place, release, true)?;

            return if let Some(outcome) = &self.cleanup_outcome {
                Ok(outcome.check(&mut self.builder, block, source)?)
            } else {
                Ok(self
                    .check_ordinary_cleanup(block, source, Some(owner), false, None)?
                    .0)
            };
        }

        self.resolve_structural_cleanup(
            block,
            source,
            if completed && phase == MirCleanupPhase::LifecycleResolution {
                MirOperationKind::Destroy {
                    collection: None,
                    place,
                    abandoned: None,
                }
            } else {
                MirOperationKind::Cleanup {
                    phase,
                    place,
                    collection: None,
                }
            },
        )
    }

    fn resolve_structural_cleanup(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        operation: MirOperationKind,
    ) -> Result<MirBlockId, LoweringError> {
        let local = self.cleanup_outcome.is_none();

        let outcome = match &self.cleanup_outcome {
            // Structural traversal borrows the existing collection's immutable storage identities.
            Some(outcome) => outcome.clone(),
            None => self.create_cleanup_outcome(block, source)?,
        };

        let retention = if matches!(
            operation,
            MirOperationKind::Cleanup {
                phase: MirCleanupPhase::TaskCancellation,
                ..
            }
        ) {
            None
        } else {
            self.cleanup_retention(source)?
        };

        let outcome = outcome.with_retention(retention);
        let completed = outcome.resolve(&mut self.builder, block, source, [operation])?;

        if local {
            self.finish_finalizer_outcome(completed, source, &outcome)
        } else {
            Ok(completed)
        }
    }

    fn push_inactive_cleanup(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
    ) -> Result<MirBlockId, LoweringError> {
        let occurrence = self
            .cleanup_occurrence
            .expect("inactive capture cleanup retains its ownership-end occurrence");

        let local = self.cleanup_outcome.is_none();

        let outcome = match &self.cleanup_outcome {
            // Nested resolution shares the enclosing outcome's immutable storage paths.
            Some(outcome) => outcome.clone(),
            None => self.create_cleanup_outcome(block, source)?,
        };

        let awaited = self.lower_awaited_frame(
            occurrence,
            MirOperand::Move(place),
            None,
            block,
            Self::retained_source(source),
        )?;

        let block = awaited
            .block
            .expect("inactive capture cleanup resumes its enclosing owner");

        let block = outcome.check_abandonment(&mut self.builder, block, source)?;

        if local {
            self.finish_finalizer_outcome(block, source, &outcome)
        } else {
            Ok(block)
        }
    }

    fn push_finalizer(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: LifecycleCallable,
    ) -> Result<MirBlockId, LoweringError> {
        let local = self.cleanup_outcome.is_none();

        let outcome = match &self.cleanup_outcome {
            // Nested resolution shares the enclosing outcome's immutable storage paths.
            Some(outcome) => outcome.clone(),
            None => self.create_cleanup_outcome(block, source)?,
        };

        let invocation = callable.result;

        let receiver = self.input.semantic_values().type_data(callable.receiver);

        let borrow = match receiver.as_ref() {
            bray_symbols::TypeData::Borrow { kind, .. } => Some(*kind),
            _ => None,
        };

        let value = crate::lifecycle_call::lower_lifecycle_call(
            MirCallableReference::new(callable.callable, callable.abi),
            callable.receiver,
            Self::retained_place(&place),
            borrow,
            invocation,
            true,
            None,
            |operation, ty| {
                self.push_operation(block, Self::retained_source(source), operation, ty)
            },
        )?;

        let (block, value) = match invocation {
            bray_bound_tree::BoundCallResult::Immediate(_) => (block, MirOperand::Value(value)),
            bray_bound_tree::BoundCallResult::LazyFuture(_) => {
                let occurrence = self
                    .cleanup_occurrence
                    .expect("async finalization retains its ownership-end occurrence");

                let awaited = self.lower_awaited_frame(
                    occurrence,
                    MirOperand::Value(value),
                    Some(invocation.completion_type()),
                    block,
                    Self::retained_source(source),
                )?;

                (
                    awaited
                        .block
                        .expect("shielded finalization resumes cleanup"),
                    awaited
                        .value
                        .expect("shielded finalization publishes its completion result"),
                )
            }
        };

        let retention = self.lifecycle_retention(source, invocation)?;

        let outcome = outcome.with_retention(retention);

        let payloads = crate::operand::result_payloads(
            self.input.semantic_values(),
            self.input.available_compiler_known_symbols(),
            callable.result.completion_type(),
        );

        let (block, abandoned) = outcome.retain_finalizer_result(
            &mut self.builder,
            block,
            source,
            value,
            &place,
            (
                MirCallableReference::new(callable.callable, callable.abi),
                invocation,
            ),
            payloads,
        )?;

        // Keep the owner broadcastable throughout finalizer mutation. Destruction
        // retires its guard only after every finalizer outcome has reached this point.
        self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::Destroy {
                collection: None,
                place,
                abandoned: Some(abandoned),
            },
            None,
        )?;

        let block = outcome.check(&mut self.builder, block, source)?;

        if local {
            self.finish_finalizer_outcome(block, source, &outcome)
        } else {
            Ok(block)
        }
    }

    pub(in crate::lowering) fn finish_finalizer_outcome(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        outcome: &CleanupOutcome,
    ) -> Result<MirBlockId, LoweringError> {
        // Failure handlers own their report storage independently of this local outcome.
        let (panicked, cancelled, report) = self
            .cleanup_failure_targets
            .clone()
            .expect("ordinary finalization retains the surrounding cleanup failure targets");

        let mut transfers = [panicked, cancelled];

        for target in &mut transfers {
            let transfer = self.builder.push_block(
                Self::retained_source(source),
                self.builder.block_kind(block),
            )?;

            self.push_operation(
                transfer,
                Self::retained_source(source),
                MirOperationKind::Store {
                    kind: bray_ir::MirStoreKind::Initialize,
                    destination: Self::retained_place(&report),
                    value: outcome.report(),
                },
                None,
            )?;

            crate::continuation::transfer_cleanup(
                &mut self.builder,
                transfer,
                source,
                MirEdge::new(*target, []),
            );

            *target = transfer;
        }

        let [panicked, cancelled] = transfers;

        Ok(outcome.dispatch(
            &mut self.builder,
            block,
            source,
            panicked,
            cancelled,
            &mut None,
        )?)
    }
}
