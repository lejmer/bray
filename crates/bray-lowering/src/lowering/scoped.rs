use bray_bound_tree::{
    BoundCallResult, BoundExecutionSite, BoundExpressionId, BoundPatternId, SelectedReceiver,
    SemanticSelection, StorageBinding, StorageBindingTarget,
};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirCallableReference, MirOperand, MirOperationKind, MirSourceAnchor, MirStoreKind,
};
use bray_symbols::{BorrowKind, ReceiverMode, TypeData, TypeId};

use super::{LoweringError, block::LoweredExpression, lowerer::Lowerer};

/// Entry activity and the separately admitted exit duty have distinct failure paths.
pub(super) struct ScopedCleanup {
    pub(super) entered: bray_ir::MirPlace,
    pub(super) admitted: bray_ir::MirPlace,
}

impl Lowerer<'_> {
    pub(super) fn lower_scope_enter(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        completion: MirBlockId,
        completion_type: TypeId,
    ) -> Result<LoweredExpression, LoweringError> {
        let Some(SemanticSelection::ScopedUse(scoped)) =
            self.input.semantic_selections().expression(expression)
        else {
            panic!("with occurrence {expression:?} retains its selected lifecycle pair");
        };

        let receiver = scoped
            .enter()
            .1
            .receiver()
            .expect("selected enter retains its receiver");

        let input = SelectedReceiver::new(
            scoped.initializer(),
            receiver.parameter(),
            receiver.mode(),
            scoped.source_type(),
            receiver.ty(),
        )
        .input_type(self.input.semantic_values())?;

        let borrow = match receiver.mode() {
            ReceiverMode::Shared => Some(BorrowKind::Shared),
            ReceiverMode::Mutable => Some(BorrowKind::Mutable),
            ReceiverMode::Consuming | ReceiverMode::ConsumingMutable => None,
        };

        let result = scoped.enter().1.result();
        let capability_type = scoped.capability_type();
        let invocation_result = scoped.enter_result();
        let receiver_target = receiver.ty();

        let callable_type = self
            .input
            .semantic_values()
            .type_data(scoped.enter().1.callable_type());

        let TypeData::Callable(callable_type) = callable_type.as_ref() else {
            panic!("selected enter retains its callable type");
        };

        let callable = MirCallableReference::new(scoped.enter().0, callable_type.abi());
        let initializer = scoped.initializer();

        let access = self
            .input
            .storage_plan()
            .occurrence_plans(BoundExecutionSite::ScopedEnter(expression))
            .next()
            .expect("selected enter retains its receiver access")
            .access();

        self.lower_materialized_access_place_with(
            initializer,
            access,
            current,
            |lowerer, current, place| {
                let source = lowerer.expression_source(expression);
                let current = lowerer.admit_scoped_exit(expression, current, &source)?;

                let entry_owner = matches!(invocation_result, BoundCallResult::LazyFuture(_))
                    .then_some(bray_ir::MirOutgoingOwner::Invocation {
                        callable,
                        result: invocation_result,
                    });

                let current = match &entry_owner {
                    // The selected async entry uses the ordinary factory admission contract.
                    Some(owner) => {
                        lowerer.admit_outgoing(expression, current, &source, owner.clone())?
                    }
                    None => current,
                };

                let mut projections = place.projections().to_vec();

                let reached = lowerer.append_reached_dereference(
                    place.ty(),
                    receiver_target,
                    &mut projections,
                );

                let place = bray_ir::MirPlace::new(place.storage(), projections, reached);

                let arguments = [bray_ir::MirCallArgument::positional(
                    0,
                    MirOperand::Move(Self::retained_place(&place)),
                )];

                let current = lowerer.lower_declared_requirements(
                    BoundExecutionSite::ScopedEnter(expression),
                    current,
                    &source,
                    callable,
                    &arguments,
                    None,
                )?;

                let value = crate::lifecycle_call::lower_lifecycle_call(
                    callable,
                    input,
                    place,
                    borrow,
                    invocation_result,
                    false,
                    None,
                    |operation, ty| {
                        lowerer.push_operation(
                            current,
                            Self::retained_source(&source),
                            operation,
                            ty,
                        )
                    },
                )?;

                if let Some(owner) = entry_owner {
                    lowerer.discharge_outgoing(current, &source, owner)?;
                }

                let (current, value) = if matches!(invocation_result, BoundCallResult::Immediate(_))
                {
                    lowerer.finish_typed_call_panic_check(
                        BoundExecutionSite::ScopedEnter(expression),
                        current,
                        &source,
                        &MirOperand::Value(value),
                        invocation_result.ty(),
                        None,
                        crate::lowering::expression::CallOutcome::Selected,
                    )?
                } else {
                    (current, MirOperand::Value(value))
                };

                let (current, value) =
                    if matches!(invocation_result, BoundCallResult::LazyFuture(_)) {
                        let awaited = lowerer.lower_awaited_frame(
                            BoundExecutionSite::ScopedEnter(expression),
                            value,
                            Some(result),
                            current,
                            Self::retained_source(&source),
                        )?;

                        let current = awaited
                            .block
                            .expect("scope entry await resumes before capability initialization");

                        let value = awaited
                            .value
                            .expect("scope entry await publishes its completion");

                        lowerer.finish_typed_call_panic_check(
                            BoundExecutionSite::ScopedEnter(expression),
                            current,
                            &source,
                            &value,
                            result,
                            None,
                            crate::lowering::expression::CallOutcome::Selected,
                        )?
                    } else {
                        (current, value)
                    };

                if !lowerer.invocation_completes(BoundExecutionSite::ScopedEnter(expression)) {
                    lowerer.set_terminator(
                        current,
                        Self::retained_source(&source),
                        bray_ir::MirTerminatorKind::Unreachable,
                    )?;

                    return Ok(LoweredExpression::terminated(source));
                }

                let (current, value) = if result != capability_type {
                    let (success, capability, error, failure) =
                        lowerer.branch_result_value(current, &source, value, result)?;

                    let failure_type = lowerer.builder.operand_type(&failure);

                    let MirOperand::Value(failure) = failure else {
                        panic!("Result branch publishes its projected payload value");
                    };

                    let (error, failure) = lowerer.discharge_scoped_exit(
                        error,
                        &source,
                        expression,
                        Some((failure, failure_type)),
                    )?;

                    let failure = MirOperand::Value(
                        failure
                            .expect("guarded discharge forwards its pending payload")
                            .0,
                    );

                    let failure = lowerer.construct_result(
                        error,
                        &source,
                        completion_type,
                        false,
                        failure,
                    )?;

                    lowerer.set_terminator(
                        error,
                        Self::retained_source(&source),
                        bray_ir::MirTerminatorKind::Goto(bray_ir::MirEdge::new(
                            completion,
                            [failure],
                        )),
                    )?;

                    (success, capability)
                } else {
                    (current, value)
                };

                Ok(LoweredExpression::continuing(current, Some(value), source))
            },
        )
    }

    pub(super) fn bind_scoped_capability(
        &mut self,
        expression: BoundExpressionId,
        pattern: BoundPatternId,
        value: MirOperand,
        current: MirBlockId,
    ) -> Result<MirBlockId, LoweringError> {
        let StorageBinding::Access(access) = self
            .input
            .storage_plan()
            .binding(StorageBindingTarget::PatternSubject(pattern))
            .expect("scoped pattern observes its capability storage")
        else {
            panic!("scoped pattern retains its capability access");
        };

        let place = self.place_for_access(access, false)?;
        let source = self.expression_source(expression);

        self.push_operation(
            current,
            Self::retained_source(&source),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: Self::retained_place(&place),
                value,
            },
            None,
        )?;

        let guard = Self::retained_place(
            &self
                .scoped_guards
                .get(&expression)
                .expect("entered scope retains its cleanup duty")
                .entered,
        );

        self.set_boolean_guard(current, &source, &guard, true)?;
        self.mark_scope_entered(expression);

        self.lower_pattern_bindings(pattern, MirOperand::Copy(place), current)
    }

    pub(super) fn push_scope_exit(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        expression: BoundExpressionId,
        exit: Option<bray_bound_tree::AnyBoundNodeId>,
        pending: Option<&bray_ir::MirPlace>,
    ) -> Result<MirBlockId, LoweringError> {
        let Some(SemanticSelection::ScopedUse(scoped)) =
            self.input.semantic_selections().expression(expression)
        else {
            panic!("scoped cleanup {expression:?} retains its selected lifecycle pair");
        };

        let callable_type = self
            .input
            .semantic_values()
            .type_data(scoped.exit().1.callable_type());

        let TypeData::Callable(callable_type) = callable_type.as_ref() else {
            panic!("selected exit retains its callable type");
        };

        let callable = MirCallableReference::new(scoped.exit().0, callable_type.abi());
        let parameter = scoped.exit().1.parameters()[0].ty();
        let result = scoped.exit().1.result();
        let invocation_result = scoped.exit_result();

        let access = self
            .input
            .storage_plan()
            .occurrence_plans(BoundExecutionSite::ScopedExit(expression))
            .next()
            .expect("selected exit retains its capability access")
            .access();

        let place = self.place_for_access(access, false)?;

        let guard = self
            .scoped_guards
            .get(&expression)
            .map(|state| Self::retained_place(&state.entered))
            .expect("scoped entry retains its protocol guard");

        let clear = Self::retained_place(&guard);

        let block = self
            .guarded_cleanup_region(
                block,
                source,
                place,
                Some(guard),
                None,
                |lowerer, block, place, value| {
                    // Clear protocol activity before invoking user code, independently of capability destruction.
                    lowerer.set_boolean_guard(block, source, &clear, false)?;

                    let ordinary_shield = lowerer.cleanup_outcome.is_none()
                        && matches!(invocation_result, BoundCallResult::LazyFuture(_));

                    if ordinary_shield {
                        lowerer.cleanup_shield(
                            block,
                            source,
                            bray_runtime_interface::RuntimeAbiRole::CleanupShieldEnter,
                        )?;
                    }

                    let mut continuations = Vec::new();

                    let block = lowerer.lower_scoped_exit_requirements(
                        expression,
                        block,
                        source,
                        callable,
                        &place,
                        ordinary_shield,
                        &mut continuations,
                    )?;

                    let call = crate::lifecycle_call::lower_lifecycle_call(
                        callable,
                        parameter,
                        place,
                        None,
                        invocation_result,
                        true,
                        None,
                        |operation, ty| {
                            lowerer.push_operation(
                                block,
                                Self::retained_source(source),
                                operation,
                                ty,
                            )
                        },
                    )?;

                    let mut block = block;
                    let mut completion = MirOperand::Value(call);

                    if matches!(invocation_result, BoundCallResult::LazyFuture(_)) {
                        let awaited = lowerer.lower_awaited_frame(
                            BoundExecutionSite::ScopedExit(expression),
                            completion,
                            Some(result),
                            block,
                            Self::retained_source(source),
                        )?;

                        block = awaited
                            .block
                            .expect("shielded scope exit resumes to its cleanup dispatcher");

                        completion = awaited
                            .value
                            .expect("shielded scope exit publishes its completion result");
                    }

                    (block, completion) = lowerer.check_scoped_exit_call(
                        expression,
                        block,
                        source,
                        ordinary_shield,
                        &mut continuations,
                        true,
                        completion,
                    )?;

                    if ordinary_shield {
                        lowerer.cleanup_shield(
                            block,
                            source,
                            bray_runtime_interface::RuntimeAbiRole::CleanupShieldLeave,
                        )?;
                    }

                    if lowerer.type_representation(result) == Some(RepresentationRole::Result) {
                        let (success, _, error, failure) =
                            lowerer.branch_result_value(block, source, completion, result)?;

                        let joined = lowerer.builder.push_block(
                            Self::retained_source(source),
                            lowerer.builder.block_kind(block),
                        )?;

                        lowerer.set_terminator(
                            success,
                            Self::retained_source(source),
                            bray_ir::MirTerminatorKind::Goto(bray_ir::MirEdge::new(joined, [])),
                        )?;

                        let error = if lowerer.cleanup_outcome.is_none()
                            && bray_bound_tree::scoped_exit_completes_result(
                                lowerer.input.unit().view(),
                                expression,
                                exit,
                            ) {
                            let pending = pending.expect(
                            "normal fallible with completion retains its pending result storage",
                        );

                            lowerer.replace_scoped_result(error, source, pending, failure)?
                        } else {
                            let local = lowerer.cleanup_outcome.is_none();

                            let outcome = match &lowerer.cleanup_outcome {
                                // The nested incident shares the enclosing outcome's storage paths.
                                Some(outcome) => outcome.clone(),
                                None => lowerer.create_cleanup_outcome(error, source)?,
                            };

                            let retention =
                                lowerer.lifecycle_retention(source, invocation_result)?;

                            let outcome = outcome.with_retention(retention);

                            let error = outcome.retain_incident(
                                &mut lowerer.builder,
                                error,
                                source,
                                failure,
                                (callable, invocation_result),
                            )?;

                            if local {
                                lowerer.finish_finalizer_outcome(error, source, &outcome)?
                            } else {
                                error
                            }
                        };

                        lowerer.set_terminator(
                            error,
                            Self::retained_source(source),
                            bray_ir::MirTerminatorKind::Goto(bray_ir::MirEdge::new(joined, [])),
                        )?;

                        block = joined;
                    }

                    for continuation in continuations.into_iter().rev() {
                        lowerer.set_terminator(
                            block,
                            Self::retained_source(source),
                            bray_ir::MirTerminatorKind::Goto(bray_ir::MirEdge::new(
                                continuation,
                                [],
                            )),
                        )?;

                        block = continuation;
                    }

                    Ok((block, value))
                },
            )?
            .0;

        self.discharge_scoped_exit(block, source, expression, None)
            .map(|(block, _)| block)
    }

    fn replace_scoped_result(
        &mut self,
        error: MirBlockId,
        source: &MirSourceAnchor,
        pending: &bray_ir::MirPlace,
        failure: MirOperand,
    ) -> Result<MirBlockId, LoweringError> {
        let previous = self.builder.push_storage(
            Self::retained_source(source),
            bray_ir::MirStorageKind::Temporary,
            pending.ty(),
        )?;

        let previous = bray_ir::MirPlace::new(previous, [], pending.ty());

        self.push_operation(
            error,
            Self::retained_source(source),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: Self::retained_place(&previous),
                value: MirOperand::Move(Self::retained_place(pending)),
            },
            None,
        )?;

        let failure = self.construct_result(error, source, pending.ty(), false, failure)?;

        self.push_operation(
            error,
            Self::retained_source(source),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: Self::retained_place(pending),
                value: failure,
            },
            None,
        )?;

        // Publish ownership before discarding the former success value. A disposal
        // panic then cleans the error through the existing pending-result path.
        Ok(self
            .push_guarded_cleanup(
                error,
                source,
                bray_ir::MirCleanupPhase::LifecycleResolution,
                previous,
                None,
                None,
                false,
                None,
            )?
            .0)
    }

    pub(super) fn check_scoped_exit_call(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        shielded: bool,
        continuations: &mut Vec<MirBlockId>,
        selected_outcome: bool,
        value: MirOperand,
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        let reports = selected_outcome
            && self
                .input
                .storage_flow()
                .completion_reports(BoundExecutionSite::ScopedExit(expression));

        if let Some(outcome) = &self.cleanup_outcome {
            let (completed, value, continuation) =
                outcome.check_completion(&mut self.builder, block, source, Some(value), reports)?;

            continuations.push(continuation);

            Ok((
                completed,
                value.expect("completed scoped exit retains its result"),
            ))
        } else {
            let (completed, value) = self.check_ordinary_cleanup(
                block,
                source,
                None,
                shielded,
                Some(value),
                selected_outcome.then_some(BoundExecutionSite::ScopedExit(expression)),
            )?;

            Ok((
                completed,
                value.expect("completed scoped exit retains its result"),
            ))
        }
    }

    pub(super) fn cleanup_shield(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        role: bray_runtime_interface::RuntimeAbiRole,
    ) -> Result<(), LoweringError> {
        let unit = self.representation_type(RepresentationRole::Unit)?;

        self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::Call(bray_ir::MirCall::protocol(
                bray_ir::MirCallTarget::Runtime(self.runtime_reference(role)),
                BoundCallResult::Immediate(unit),
                [],
                [],
            )),
            Some(unit),
        )?;

        Ok(())
    }
}
