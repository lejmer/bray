use bray_ir::{MirOperationKind, MirPlace, MirSourceAnchor, MirUnitBuilder};
use bray_symbols::{CallableExecution, TypeData};

use super::super::{SyntheticLowerer, SyntheticLoweringContext};

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(super) fn push_lifecycle_operation(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        operation: MirOperationKind,
    ) -> Result<(), C::Error> {
        builder
            .push_operation(block, source.clone(), operation, None)
            .map_err(|cause| self.capacity_error(cause))?;

        Ok(())
    }

    pub(super) fn push_owner_resolution(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        retirement: Option<MirOperationKind>,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let boolean = self
            .context
            .representation_type(bray_compiler_known::RepresentationRole::ScalarBool)?;

        let outcome = self.cleanup_outcome(builder, block, source)?;

        let action = self
            .context
            .lifecycle_action(place.ty(), bray_bound_tree::LifecyclePhase::Finalize)?;

        let (block, abandoned) = match action {
            bray_bound_tree::LifecycleAction::None => (
                block,
                bray_ir::MirOperand::Immediate {
                    ty: boolean,
                    value: bray_ir::MirImmediateValue::Boolean(false),
                },
            ),
            bray_bound_tree::LifecycleAction::Call(callable) => {
                let finalization = if retirement.is_none() {
                    self.retain_cleanup_owner(builder, source, &place, &outcome)?
                } else {
                    // The initialized prefix or target guard still covers this whole owner.
                    outcome.clone()
                };

                // Invocation borrows this path while destruction retains the same owner.
                self.push_lifecycle_call(
                    builder,
                    block,
                    source,
                    place.clone(),
                    callable,
                    &finalization,
                )?
            }
            action => {
                panic!("selected whole-value finalization must be an invocation, got {action:?}")
            }
        };

        if let Some(retirement) = retirement {
            self.push_lifecycle_operation(builder, block, source, retirement)?;
        }

        self.push_lifecycle_operation(
            builder,
            block,
            source,
            MirOperationKind::Destroy {
                collection: None,
                place,
                abandoned: Some(abandoned),
            },
        )?;

        let end = outcome
            .check(builder, block, source, false)
            .map_err(|cause| self.capacity_error(cause))?;

        self.finish_cleanup_outcome(builder, end, source, &outcome, &mut None)
    }

    pub(super) fn push_lifecycle_call(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: bray_bound_tree::LifecycleCallable,
        outcome: &crate::cleanup_outcome::CleanupOutcome,
    ) -> Result<(bray_ir::MirBlockId, bray_ir::MirOperand), C::Error> {
        // The invocation and failed-owner quiescence retain the same Arc-backed path.
        let value = self.push_selected_call(
            builder,
            block,
            source,
            place.clone(),
            callable,
            callable.result,
        )?;

        let (block, value) = match callable.result {
            bray_bound_tree::BoundCallResult::Immediate(_) => (block, value),
            bray_bound_tree::BoundCallResult::LazyFuture(_) => self.await_lifecycle_call(
                builder,
                block,
                source,
                value,
                callable.result.completion_type(),
                outcome,
            )?,
        };

        let payloads = crate::operand::result_payloads(
            self.context.semantic_values(),
            self.context.compiler_known_symbols(),
            callable.result.completion_type(),
        );

        let (completed, abandoned) = outcome
            .retain_finalizer_result(
                builder,
                block,
                source,
                bray_ir::MirOperand::Value(value),
                &place,
                (
                    bray_ir::MirCallableReference::new(callable.callable, callable.abi),
                    callable.result,
                ),
                payloads,
                self.context
                    .callable_completion_reports(callable.callable)?,
            )
            .map_err(|cause| self.capacity_error(cause))?;

        Ok((completed, abandoned))
    }

    pub(super) fn await_lifecycle_call(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        value: bray_ir::MirValueId,
        completion: bray_symbols::TypeId,
        outcome: &crate::cleanup_outcome::CleanupOutcome,
    ) -> Result<(bray_ir::MirBlockId, bray_ir::MirValueId), C::Error> {
        let (parent, state, cancellation) =
            self.cleanup_continuation(builder, block, source, outcome)?;

        let value = crate::continuation::await_frame(
            builder,
            block,
            source,
            parent,
            bray_ir::MirOperand::Value(value),
            Some(completion),
            &state,
            cancellation,
        )
        .map_err(|cause| self.capacity_error(cause))?
        .expect("awaited finalizer has a completion value");

        let resume = state.entry();

        self.frame_states.push(state);

        Ok((resume, value))
    }

    pub(super) fn compose_cleanup_frame(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        frame: bray_ir::MirOperand,
        composition: bray_ir::MirFrameComposition,
        outcome: &crate::cleanup_outcome::CleanupOutcome,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let (parent, state, cancellation) =
            self.cleanup_continuation(builder, block, source, outcome)?;

        crate::continuation::compose_frame(
            builder,
            block,
            source,
            parent,
            frame,
            composition,
            &state,
            cancellation,
            None,
        )
        .map_err(|cause| self.capacity_error(cause))?;

        let resume = state.entry();

        self.frame_states.push(state);

        outcome
            .check_abandonment(builder, resume, source)
            .map_err(|cause| self.capacity_error(cause))
    }

    fn cleanup_continuation(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        outcome: &crate::cleanup_outcome::CleanupOutcome,
    ) -> Result<
        (
            bray_runtime_interface::ProtectedAsyncFrameId,
            bray_ir::MirFrameState,
            bray_ir::MirCleanupEdge,
        ),
        C::Error,
    > {
        let parent = self.frame.unwrap_or_else(|| {
            panic!("suspending cleanup expands into its owning protected frame: source={source:?}")
        });

        let collection = outcome.collection();

        let retention = collection
            .retention()
            .expect("suspending cleanup retains its established source contract");

        let resume = builder
            .push_block(source.clone(), builder.block_kind(block))
            .map_err(|cause| self.capacity_error(cause))?;

        let cancelled = builder
            .push_block(source.clone(), bray_ir::MirBlockKind::CleanupBroadcast)
            .map_err(|cause| self.capacity_error(cause))?;

        builder.set_terminator(
            cancelled,
            source.clone(),
            bray_ir::MirTerminatorKind::Unreachable,
        );

        let state = bray_ir::MirFrameState::new(
            bray_ir::MirFrameStateId::new(u32::try_from(self.frame_states.len()).map_err(
                |_| self.capacity_error(bray_ir::MirCapacityError::IdentityCapacityExceeded),
            )?),
            resume,
            // Executable states reuse the checked cleanup contract's immutable slices.
            retention.as_ref().clone(),
        );

        let cancellation = bray_ir::MirCleanupEdge::new(
            bray_ir::MirCleanupPhase::TaskCancellation,
            bray_ir::MirEdge::new(cancelled, []),
        );

        Ok((parent, state, cancellation))
    }

    pub(super) fn push_checked_lifecycle_call(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: bray_bound_tree::LifecycleCallable,
    ) -> Result<(bray_ir::MirBlockId, bray_ir::MirValueId), C::Error> {
        if callable.callable.definition().callable_symbol().kind()
            == bray_symbols::SymbolKind::Destructor
        {
            return self.push_destruction_body(builder, block, source, place, callable);
        }

        let result = callable.result;

        let reports = result.execution() == CallableExecution::Synchronous
            && self
                .context
                .callable_completion_reports(callable.callable)?;

        if reports && self.collection.is_none() && self.pending_reports.is_none() {
            let pending = self.allocate_outcome(builder, source)?;

            pending
                .initialize_pending(builder, block, source)
                .map_err(|cause| self.capacity_error(cause))?;

            self.pending_reports = Some(pending);
        }

        let value = self.push_selected_call(builder, block, source, place, callable, result)?;

        if result.execution() == CallableExecution::Synchronous {
            self.check_lifecycle_value(builder, block, source, value, result.ty(), reports)
        } else {
            Ok((block, value))
        }
    }

    pub(super) fn push_selected_call(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: bray_bound_tree::LifecycleCallable,
        result: bray_bound_tree::BoundCallResult,
    ) -> Result<bray_ir::MirValueId, C::Error> {
        let receiver = self.context.semantic_values().type_data(callable.receiver);

        let borrow = match receiver.as_ref() {
            TypeData::Borrow { kind, .. } => Some(*kind),
            _ => None,
        };

        crate::lifecycle_call::lower_lifecycle_call(
            bray_ir::MirCallableReference::new(callable.callable, callable.abi),
            callable.receiver,
            place,
            borrow,
            result,
            true,
            None,
            |operation, result| builder.push_operation(block, source.clone(), operation, result),
        )
        .map_err(|cause| self.capacity_error(cause))
    }
}
