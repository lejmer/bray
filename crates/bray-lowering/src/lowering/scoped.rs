use bray_bound_tree::{
    BoundCallResult, BoundExpressionId, BoundPatternId, SelectedReceiver, SemanticOccurrence,
    SemanticSelection, StorageBinding, StorageBindingTarget,
};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirCallableReference, MirImmediateValue, MirOperand, MirOperationKind,
    MirSourceAnchor, MirStoreKind,
};
use bray_symbols::{BorrowKind, ReceiverMode, TypeData, TypeId};

use super::{LoweringError, block::LoweredExpression, lowerer::Lowerer};

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
            .occurrence_plans(SemanticOccurrence::ScopeEnter(expression))
            .next()
            .expect("selected enter retains its receiver access")
            .access();

        self.lower_materialized_access_place_with(
            initializer,
            access,
            current,
            |lowerer, current, place| {
                let source = lowerer.expression_source(expression);
                let mut projections = place.projections().to_vec();

                let reached = lowerer.append_reached_dereference(
                    place.ty(),
                    receiver_target,
                    &mut projections,
                );

                let place = bray_ir::MirPlace::new(place.storage(), projections, reached);

                let value = crate::lifecycle_call::lower_lifecycle_call(
                    callable,
                    input,
                    place,
                    borrow,
                    invocation_result,
                    false,
                    |operation, ty| {
                        lowerer.push_operation(
                            current,
                            Self::retained_source(&source),
                            operation,
                            ty,
                        )
                    },
                )?;

                let (current, value) = lowerer.finish_typed_call_panic_check(
                    expression,
                    current,
                    &source,
                    &MirOperand::Value(value),
                    invocation_result.ty(),
                    None,
                )?;

                let (current, value) =
                    if matches!(invocation_result, BoundCallResult::LazyFuture(_)) {
                        let awaited = lowerer.lower_awaited_frame(
                            SemanticOccurrence::ScopeEnter(expression),
                            value,
                            result,
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
                            expression, current, &source, &value, result, None,
                        )?
                    } else {
                        (current, value)
                    };

                let (current, value) = if result != capability_type {
                    let (success, capability, error, failure) =
                        lowerer.branch_result_value(expression, current, &source, value, result)?;

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
        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;

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

        let guard = self.new_initialization_guard(current, &source, boolean, &[], true)?;

        self.scoped_guards.insert(expression, guard);

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
            .occurrence_plans(SemanticOccurrence::ScopeExit(expression))
            .next()
            .expect("selected exit retains its capability access")
            .access();

        let place = self.place_for_access(access, false)?;

        let guard = self
            .scoped_guards
            .get(&expression)
            .map(Self::retained_place)
            .expect("successful scoped entry retains its active protocol guard");

        let clear = Self::retained_place(&guard);

        self.guarded_cleanup_region(
            block,
            source,
            place,
            Some(guard),
            None,
            |lowerer, block, place, value| {
                // Clear protocol activity before invoking user code, independently of capability destruction.
                lowerer.push_operation(
                    block,
                    Self::retained_source(source),
                    MirOperationKind::Store {
                        kind: MirStoreKind::Assign,
                        destination: Self::retained_place(&clear),
                        value: MirOperand::Immediate {
                            value: MirImmediateValue::Boolean(false),
                            ty: clear.ty(),
                        },
                    },
                    None,
                )?;

                let ordinary_shield = lowerer.cleanup_outcome.is_none()
                    && matches!(invocation_result, BoundCallResult::LazyFuture(_));

                if ordinary_shield {
                    lowerer.cleanup_shield(
                        block,
                        source,
                        bray_runtime_interface::RuntimeAbiRole::CleanupShieldEnter,
                    )?;
                }

                let call = crate::lifecycle_call::lower_lifecycle_call(
                    callable,
                    parameter,
                    place,
                    None,
                    invocation_result,
                    true,
                    |operation, ty| {
                        lowerer.push_operation(block, Self::retained_source(source), operation, ty)
                    },
                )?;

                let mut continuations = Vec::new();
                let mut block = block;
                let mut completion = MirOperand::Value(call);

                if matches!(invocation_result, BoundCallResult::LazyFuture(_)) {
                    block = lowerer.check_scoped_exit_call(block, source, ordinary_shield, &mut continuations)?;

                    let awaited = lowerer.lower_awaited_frame(
                        SemanticOccurrence::ScopeExit(expression), completion, result, block,
                        Self::retained_source(source),
                    )?;

                    block = awaited.block.expect("shielded scope exit resumes to its cleanup dispatcher");
                    completion = awaited.value.expect("shielded scope exit publishes its completion result");
                }

                block = lowerer.check_scoped_exit_call(block, source, ordinary_shield, &mut continuations)?;

                if ordinary_shield {
                    lowerer.cleanup_shield(block, source, bray_runtime_interface::RuntimeAbiRole::CleanupShieldLeave)?;
                }

                if lowerer.type_representation(result) == Some(RepresentationRole::Result) {
                    let (success, _, error, failure) = lowerer.branch_result_value(expression, block, source, completion, result)?;

                    let joined = lowerer.builder.push_block(Self::retained_source(source), lowerer.builder.block_kind(block))?;

                    lowerer.set_terminator(success, Self::retained_source(source), bray_ir::MirTerminatorKind::Goto(bray_ir::MirEdge::new(joined, [])))?;

                    let error = if lowerer.cleanup_outcome.is_none() && lowerer.scoped_exit_completes_result(expression, exit) {
                        let pending = pending.expect("normal fallible with completion retains its pending result storage");
                        let previous = lowerer.builder.push_storage(Self::retained_source(source), bray_ir::MirStorageKind::Temporary, pending.ty())?;
                        let previous = bray_ir::MirPlace::new(previous, [], pending.ty());

                        lowerer.push_operation(error, Self::retained_source(source), MirOperationKind::Store {
                            kind: MirStoreKind::Initialize, destination: Self::retained_place(&previous),
                            value: MirOperand::Move(Self::retained_place(pending)),
                        }, None)?;

                        let failure = lowerer.construct_result(error, source, pending.ty(), false, failure)?;

                        lowerer.push_operation(error, Self::retained_source(source), MirOperationKind::Store { kind: MirStoreKind::Initialize, destination: Self::retained_place(pending), value: failure }, None)?;

                        // Publish ownership before discarding the former success value. A disposal
                        // panic then cleans the error through the existing pending-result path.
                        lowerer.push_guarded_cleanup(error, source, bray_ir::MirCleanupPhase::LifecycleResolution, previous, None, None, false, None)?.0
                    } else {
                        lowerer.push_operation(error, Self::retained_source(source), MirOperationKind::Async(bray_ir::MirAsyncOperation::TransferCleanupIncident { incident: failure, runtime: lowerer.runtime_reference(bray_runtime_interface::RuntimeAbiRole::CleanupIncidentTransfer) }), None)?;

                        error
                    };

                    lowerer.set_terminator(error, Self::retained_source(source), bray_ir::MirTerminatorKind::Goto(bray_ir::MirEdge::new(joined, [])))?;
                    block = joined;
                }

                for continuation in continuations.into_iter().rev() {
                    lowerer.set_terminator(block, Self::retained_source(source), bray_ir::MirTerminatorKind::Goto(bray_ir::MirEdge::new(continuation, [])))?;
                    block = continuation;
                }

                Ok((block, value))
            },
        )
        .map(|(block, _)| block)
    }

    fn check_scoped_exit_call(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        shielded: bool,
        continuations: &mut Vec<MirBlockId>,
    ) -> Result<MirBlockId, LoweringError> {
        if let Some(outcome) = &self.cleanup_outcome {
            let (completed, continuation) =
                outcome.check_completion(&mut self.builder, block, source)?;

            continuations.push(continuation);

            Ok(completed)
        } else {
            self.check_ordinary_cleanup(block, source, None, shielded)
        }
    }

    fn scoped_exit_completes_result(
        &self,
        expression: BoundExpressionId,
        exit: Option<bray_bound_tree::AnyBoundNodeId>,
    ) -> bool {
        let Some(bray_bound_tree::BoundExpression::Structured(scoped)) =
            self.input.unit().view().expression(expression)
        else {
            panic!("selected scoped use retains its with expression");
        };

        let [body] = scoped.blocks() else {
            panic!("selected scoped use retains its body");
        };

        let syntax = self
            .input
            .unit()
            .view()
            .block(*body)
            .expect("scoped body belongs to its unit")
            .origin()
            .source_anchor()
            .syntax();

        match exit {
            Some(bray_bound_tree::AnyBoundNodeId::Block(block)) => block == *body,
            Some(bray_bound_tree::AnyBoundNodeId::Expression(transfer)) => {
                matches!(self.input.unit().view().expression(transfer), Some(bray_bound_tree::BoundExpression::ControlTransfer(transfer)) if transfer.kind() == bray_bound_tree::BoundControlTransferKind::Yield && transfer.target() == Some(syntax))
            }
            _ => false,
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
