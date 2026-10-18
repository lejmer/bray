use bray_bound_tree::BoundCallResult;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCall, MirCallFailureEdge, MirCallTarget, MirCapacityError,
    MirCleanupEdge, MirCleanupPhase, MirEdge, MirImmediateValue, MirOperand, MirOperationKind,
    MirPlace, MirRuntimeReference, MirSourceAnchor, MirStorageKind, MirStoreKind,
    MirTerminatorKind, MirUnitBuilder,
};
use bray_runtime_interface::{RuntimeAbiRole, RuntimeAbiVersion};
use bray_symbols::TypeId;

pub(crate) enum CleanupFailure {
    Panic(MirOperand),
    Cancellation(Option<MirOperand>),
}

/// Retains cleanup incidents until every selected obligation has been resolved.
#[derive(Clone)]
pub(crate) struct CleanupOutcome {
    panicked: MirPlace,
    cancelled: MirPlace,
    report: MirPlace,
    incoming_report: MirPlace,
    unit: TypeId,
    runtime_abi: RuntimeAbiVersion,
}

// MIR instructions own their provenance and place paths. Clones below share the Arc-backed
// paths while this builder retains the same storage identities for later incident branches.
impl CleanupOutcome {
    /// Resolves every operation before the caller dispatches the collected outcome.
    pub(crate) fn resolve(
        &self,
        builder: &mut MirUnitBuilder,
        mut block: MirBlockId,
        source: &MirSourceAnchor,
        operations: impl IntoIterator<Item = MirOperationKind>,
    ) -> Result<MirBlockId, MirCapacityError> {
        for operation in operations {
            builder.push_operation(block, source.clone(), operation, None)?;
            block = self.check(builder, block, source)?;
        }

        Ok(block)
    }

    pub(crate) fn new(
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        boolean: TypeId,
        report: TypeId,
        unit: TypeId,
        runtime_abi: RuntimeAbiVersion,
    ) -> Result<Self, MirCapacityError> {
        let outcome = Self::allocate(
            builder,
            source,
            boolean,
            report,
            unit,
            runtime_abi,
            MirStorageKind::Temporary,
        )?;

        outcome.begin(builder, block, source)?;

        Ok(outcome)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "cleanup storage retains its three semantic types, ABI and callback lifetime"
    )]
    pub(crate) fn allocate(
        builder: &mut MirUnitBuilder,
        source: &MirSourceAnchor,
        boolean: TypeId,
        report: TypeId,
        unit: TypeId,
        runtime_abi: RuntimeAbiVersion,
        kind: MirStorageKind,
    ) -> Result<Self, MirCapacityError> {
        // Every generated operation retains the same Arc-backed source provenance.
        let mut storage = |ty| {
            builder
                .push_storage(source.clone(), kind.clone(), ty)
                .map(|id| MirPlace::new(id, [], ty))
        };

        Ok(Self {
            panicked: storage(boolean)?,
            cancelled: storage(boolean)?,
            report: storage(report)?,
            incoming_report: storage(report)?,
            unit,
            runtime_abi,
        })
    }

    pub(crate) fn begin(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<(), MirCapacityError> {
        for flag in [&self.panicked, &self.cancelled] {
            self.store(
                builder,
                block,
                source,
                flag,
                Self::boolean(false, flag.ty()),
            )?;
        }

        self.shield(builder, block, source, RuntimeAbiRole::CleanupShieldEnter)
    }

    pub(crate) fn report(&self) -> MirOperand {
        // The move operand owns its path while this builder retains the storage identity.
        MirOperand::Move(self.report.clone())
    }

    pub(crate) fn initialize_panic(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        report: MirOperand,
    ) -> Result<(), MirCapacityError> {
        self.store(builder, block, source, &self.report, report)?;

        self.store(
            builder,
            block,
            source,
            &self.panicked,
            Self::boolean(true, self.panicked.ty()),
        )
    }

    pub(crate) fn end_shield(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<(), MirCapacityError> {
        self.shield(builder, block, source, RuntimeAbiRole::CleanupShieldLeave)
    }

    pub(crate) fn initialize_cancellation(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        report: Option<MirOperand>,
    ) -> Result<(), MirCapacityError> {
        self.store(
            builder,
            block,
            source,
            &self.report,
            report.unwrap_or(MirOperand::Immediate {
                value: MirImmediateValue::EmptyPanicReport,
                ty: self.report.ty(),
            }),
        )?;

        self.store(
            builder,
            block,
            source,
            &self.cancelled,
            Self::boolean(true, self.cancelled.ty()),
        )
    }

    /// Collects a completed finalizer's active error while retaining every failed-call outcome.
    pub(crate) fn retain_finalizer_result(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        value: MirOperand,
        owner: &MirPlace,
        producer: (bray_ir::MirCallableReference, BoundCallResult),
        payloads: Option<([TypeId; 2], bray_symbols::CompilerKnownResultRepresentation)>,
    ) -> Result<(MirBlockId, MirOperand), MirCapacityError> {
        let (completed, value, failed) =
            self.check_completion(builder, block, source, Some(value))?;

        let value = value.expect("completed finalizer retains its result");
        let continuation = builder.push_block(source.clone(), builder.block_kind(block))?;

        let abandoned =
            builder.push_block_parameter(continuation, source.clone(), self.panicked.ty())?;

        builder.set_terminator(
            failed,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(
                continuation,
                [Self::boolean(true, self.panicked.ty())],
            )),
        );

        if let Some((types, representation)) = payloads {
            let (branch, [(success, _), (error, incident)]) = crate::operand::branch_result(
                builder,
                completed,
                source,
                value,
                types,
                representation,
            )?;

            builder.set_terminator(completed, source.clone(), branch);

            let error = self.retain_incident(builder, error, source, incident, producer)?;

            for branch in [success, error] {
                builder.set_terminator(
                    branch,
                    source.clone(),
                    MirTerminatorKind::Goto(MirEdge::new(
                        continuation,
                        [Self::boolean(branch == error, self.panicked.ty())],
                    )),
                );
            }
        } else {
            builder.set_terminator(
                completed,
                source.clone(),
                MirTerminatorKind::Goto(MirEdge::new(
                    continuation,
                    [Self::boolean(false, self.panicked.ty())],
                )),
            );
        }

        let pending = builder.push_block(source.clone(), builder.block_kind(continuation))?;
        let destruction = builder.push_block(source.clone(), builder.block_kind(continuation))?;

        let disposition =
            builder.push_block_parameter(destruction, source.clone(), self.panicked.ty())?;

        builder.set_terminator(
            continuation,
            source.clone(),
            MirTerminatorKind::Branch {
                condition: MirOperand::Value(abandoned),
                then_edge: MirEdge::new(pending, []),
                else_edge: MirEdge::new(destruction, [Self::boolean(false, self.panicked.ty())]),
            },
        );

        // Finalizer mutation can introduce child runs after the earlier broadcast.
        // Resolve that current graph before terminal-only destruction starts.
        let pending = self.quiesce(builder, pending, source, owner)?;

        builder.set_terminator(
            pending,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(
                destruction,
                [Self::boolean(true, self.panicked.ty())],
            )),
        );

        Ok((destruction, MirOperand::Value(disposition)))
    }

    /// Resolves an error's child work before transferring its still-owned payload.
    pub(crate) fn retain_incident(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        incident: MirOperand,
        producer: (bray_ir::MirCallableReference, BoundCallResult),
    ) -> Result<MirBlockId, MirCapacityError> {
        let ty = builder.operand_type(&incident);
        let storage = builder.push_storage(source.clone(), MirStorageKind::Temporary, ty)?;
        let place = MirPlace::new(storage, [], ty);

        builder.push_operation(
            block,
            source.clone(),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: place.clone(),
                value: incident,
            },
            None,
        )?;

        let block = self.quiesce(builder, block, source, &place)?;

        builder.push_operation(
            block,
            source.clone(),
            MirOperationKind::Async(bray_ir::MirAsyncOperation::TransferCleanupIncident {
                incident: place,
                producer: producer.0,
                invocation: producer.1,
                runtime: MirRuntimeReference::new(
                    RuntimeAbiRole::CleanupIncidentTransfer,
                    self.runtime_abi,
                ),
            }),
            None,
        )?;

        Ok(block)
    }

    fn quiesce(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        place: &MirPlace,
    ) -> Result<MirBlockId, MirCapacityError> {
        let broadcast = builder.push_block(source.clone(), MirBlockKind::CleanupBroadcast)?;
        let resolution = builder.push_block(source.clone(), MirBlockKind::LifecycleResolution)?;

        builder.set_terminator(
            block,
            source.clone(),
            MirTerminatorKind::BeginCleanup(MirCleanupEdge::new(
                MirCleanupPhase::TaskCancellation,
                MirEdge::new(broadcast, []),
            )),
        );

        builder.push_operation(
            broadcast,
            source.clone(),
            MirOperationKind::Cleanup {
                phase: MirCleanupPhase::TaskCancellation,
                place: place.clone(),
            },
            None,
        )?;

        let broadcast = self.check(builder, broadcast, source)?;

        builder.set_terminator(
            broadcast,
            source.clone(),
            MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                MirCleanupPhase::LifecycleResolution,
                MirEdge::new(resolution, []),
            )),
        );

        builder.push_operation(
            resolution,
            source.clone(),
            MirOperationKind::Quiesce(place.clone()),
            None,
        )?;

        // Quiescence retains storage on every outcome. Its incidents remain owned
        // by the enclosing cleanup before destruction or payload transfer proceeds.
        self.check(builder, resolution, source)
    }

    /// Adds incident continuations after the block's last fallible cleanup operation.
    pub(crate) fn check(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<MirBlockId, MirCapacityError> {
        let (completed, _, continuation) = self.check_completion(builder, block, source, None)?;

        builder.set_terminator(
            completed,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(continuation, [])),
        );

        Ok(continuation)
    }

    /// Retains failures from inactive capture cleanup without propagating its expected cancellation.
    pub(crate) fn check_abandonment(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<MirBlockId, MirCapacityError> {
        let kind = builder.block_kind(block);
        let panicked = builder.push_block(source.clone(), kind)?;
        let continuation = builder.push_block(source.clone(), kind)?;

        // Abandoned child retirement moves non-panic reports into the existing
        // run incident chain before delivering its empty cancellation outcome.
        let (completed, _) = check_call_outcome(
            builder,
            block,
            source,
            panicked,
            continuation,
            self.incoming_report.clone(),
            None,
        )?;

        builder.set_terminator(
            completed,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(continuation, [])),
        );

        self.retain_failure(builder, panicked, source, continuation, &self.panicked)?;

        Ok(continuation)
    }

    /// Returns the successful result path and the continuation shared with retained incidents.
    pub(crate) fn check_completion(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        value: Option<MirOperand>,
    ) -> Result<(MirBlockId, Option<MirOperand>, MirBlockId), MirCapacityError> {
        let kind = builder.block_kind(block);
        let panicked = builder.push_block(source.clone(), kind)?;
        let cancelled = builder.push_block(source.clone(), kind)?;
        let continuation = builder.push_block(source.clone(), kind)?;

        let (completed, value) = check_call_outcome(
            builder,
            block,
            source,
            panicked,
            cancelled,
            self.incoming_report.clone(),
            value,
        )?;

        self.retain_failure(builder, cancelled, source, continuation, &self.cancelled)?;

        self.retain_failure(builder, panicked, source, continuation, &self.panicked)?;

        Ok((completed, value, continuation))
    }

    fn retain_failure(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        completed: MirBlockId,
        flag: &MirPlace,
    ) -> Result<(), MirCapacityError> {
        let kind = builder.block_kind(block);
        let primary = builder.push_block(source.clone(), kind)?;
        let suppressed = builder.push_block(source.clone(), kind)?;
        let no_panic = builder.push_block(source.clone(), kind)?;
        let retained = builder.push_block(source.clone(), kind)?;

        builder.set_terminator(
            block,
            source.clone(),
            MirTerminatorKind::Branch {
                condition: MirOperand::Copy(self.panicked.clone()),
                then_edge: MirEdge::new(suppressed, []),
                else_edge: MirEdge::new(no_panic, []),
            },
        );

        builder.set_terminator(
            no_panic,
            source.clone(),
            MirTerminatorKind::Branch {
                condition: MirOperand::Copy(self.cancelled.clone()),
                then_edge: MirEdge::new(suppressed, []),
                else_edge: MirEdge::new(primary, []),
            },
        );

        self.store(
            builder,
            primary,
            source,
            &self.report,
            MirOperand::Move(self.incoming_report.clone()),
        )?;

        builder.set_terminator(
            primary,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(retained, [])),
        );

        let merged = builder.push_operation(
            suppressed,
            source.clone(),
            MirOperationKind::Call(MirCall::protocol(
                MirCallTarget::Runtime(MirRuntimeReference::new(
                    RuntimeAbiRole::PanicReportSuppression,
                    self.runtime_abi,
                )),
                BoundCallResult::Immediate(self.report.ty()),
                [
                    self.report(),
                    MirOperand::Move(self.incoming_report.clone()),
                ],
                [],
            )),
            Some(self.report.ty()),
        )?;

        let value = merged
            .result()
            .expect("value-producing MIR operation must publish a result");

        self.store(
            builder,
            suppressed,
            source,
            &self.report,
            MirOperand::Value(value),
        )?;

        builder.set_terminator(
            suppressed,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(retained, [])),
        );

        self.store(
            builder,
            retained,
            source,
            flag,
            Self::boolean(true, flag.ty()),
        )?;

        builder.set_terminator(
            retained,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(completed, [])),
        );

        Ok(())
    }

    /// Branches to caller-owned completion paths, with panic taking precedence.
    pub(crate) fn dispatch(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        panicked: MirBlockId,
        cancelled: MirBlockId,
        value: &mut Option<MirOperand>,
    ) -> Result<MirBlockId, MirCapacityError> {
        self.end_shield(builder, block, source)?;

        let kind = builder.block_kind(block);
        let no_panic = builder.push_block(source.clone(), kind)?;
        let completed = builder.push_block(source.clone(), kind)?;
        let to_no_panic = forward_cleanup_result(builder, source, no_panic, value)?;
        let to_completed = forward_cleanup_result(builder, source, completed, value)?;

        builder.set_terminator(
            block,
            source.clone(),
            MirTerminatorKind::Branch {
                condition: MirOperand::Copy(self.panicked.clone()),
                then_edge: MirEdge::new(panicked, []),
                else_edge: to_no_panic,
            },
        );

        builder.set_terminator(
            no_panic,
            source.clone(),
            MirTerminatorKind::Branch {
                condition: MirOperand::Copy(self.cancelled.clone()),
                then_edge: MirEdge::new(cancelled, []),
                else_edge: to_completed,
            },
        );

        Ok(completed)
    }

    fn shield(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        role: RuntimeAbiRole,
    ) -> Result<(), MirCapacityError> {
        builder.push_operation(
            block,
            source.clone(),
            MirOperationKind::Call(MirCall::protocol(
                MirCallTarget::Runtime(MirRuntimeReference::new(role, self.runtime_abi)),
                BoundCallResult::Immediate(self.unit),
                [],
                [],
            )),
            Some(self.unit),
        )?;

        Ok(())
    }

    fn store(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        destination: &MirPlace,
        value: MirOperand,
    ) -> Result<(), MirCapacityError> {
        builder.push_operation(
            block,
            source.clone(),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: destination.clone(),
                value,
            },
            None,
        )?;

        Ok(())
    }

    const fn boolean(value: bool, ty: TypeId) -> MirOperand {
        MirOperand::Immediate {
            value: MirImmediateValue::Boolean(value),
            ty,
        }
    }
}

/// Routes a cleanup call to its immediate parent's handlers without leaving the broadcast phase implicitly.
pub(crate) fn check_call_outcome(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    panicked: MirBlockId,
    cancelled: MirBlockId,
    report: MirPlace,
    value: Option<MirOperand>,
) -> Result<(MirBlockId, Option<MirOperand>), MirCapacityError> {
    let kind = builder.block_kind(block);

    let completed = builder.push_block(source.clone(), kind)?;

    let parameter = value
        .as_ref()
        .map(|value| {
            builder.push_block_parameter(completed, source.clone(), builder.operand_type(value))
        })
        .transpose()?;

    let (panicked, cancelled) =
        if kind == MirBlockKind::CleanupBroadcast && builder.block_kind(panicked) != kind {
            let panic_bridge = builder.push_block(source.clone(), kind)?;

            let cancel_bridge = builder.push_block(source.clone(), kind)?;

            builder.set_terminator(
                panic_bridge,
                source.clone(),
                MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                    MirCleanupPhase::LifecycleResolution,
                    MirEdge::new(panicked, []),
                )),
            );

            builder.set_terminator(
                cancel_bridge,
                source.clone(),
                MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                    MirCleanupPhase::LifecycleResolution,
                    MirEdge::new(cancelled, []),
                )),
            );

            (panic_bridge, cancel_bridge)
        } else {
            (panicked, cancelled)
        };

    builder.set_terminator(
        block,
        source.clone(),
        MirTerminatorKind::CheckCallOutcome {
            completed: MirEdge::new(completed, value),
            panicked: MirCallFailureEdge::new(panicked, report.clone()),
            cancelled: MirCallFailureEdge::new(cancelled, report),
        },
    );

    Ok((completed, parameter.map(MirOperand::Value)))
}

fn forward_cleanup_result(
    builder: &mut MirUnitBuilder,
    source: &MirSourceAnchor,
    target: MirBlockId,
    value: &mut Option<MirOperand>,
) -> Result<MirEdge, MirCapacityError> {
    let incoming = value.take();

    if let Some(incoming) = &incoming {
        let ty = builder.operand_type(incoming);
        let parameter = builder.push_block_parameter(target, source.clone(), ty)?;

        *value = Some(MirOperand::Value(parameter));
    }

    Ok(MirEdge::new(target, incoming))
}
