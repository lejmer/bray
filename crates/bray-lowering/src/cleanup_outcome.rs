use std::cell::Cell;
use std::rc::Rc;

use bray_bound_tree::BoundCallResult;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCall, MirCallTarget, MirCapacityError, MirCleanupEdge,
    MirCleanupPhase, MirCleanupState, MirEdge, MirImmediateValue, MirOperand, MirOperationKind,
    MirPlace, MirRuntimeReference, MirSourceAnchor, MirStorageKind, MirStoreKind,
    MirTerminatorKind, MirUnitBuilder,
};
use bray_runtime_interface::{RuntimeAbiRole, RuntimeAbiVersion};
use bray_symbols::TypeId;

use crate::continuation::{check_call_outcome, forward_cleanup_result};

pub(crate) enum CleanupFailure {
    Panic(MirOperand),
    Cancellation(Option<MirOperand>),
}

/// Retains cleanup incidents until every selected obligation has been resolved.
#[derive(Clone)]
pub(crate) struct CleanupOutcome {
    state: MirPlace,
    retention: Option<std::sync::Arc<bray_ir::MirFrameRetention>>,
    states: [bray_symbols::ConstantValueId; 4],
    boolean: TypeId,
    report: MirPlace,
    incoming_report: Rc<Cell<Option<bray_ir::MirStorageId>>>,
    incoming_kind: MirStorageKind,
    unit: TypeId,
    runtime_abi: RuntimeAbiVersion,
}

// MIR instructions own their provenance and place paths. Clones below share the Arc-backed
// paths while this builder retains the same storage identities for later incident branches.
// Builder views share one lazily allocated incoming slot. Structural forwarding alone creates
// no report temporary, and sequential calls in the same collection reuse its scratch storage.
impl CleanupOutcome {
    pub(crate) fn new(
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        boolean: TypeId,
        report: TypeId,
        unit: TypeId,
        runtime_abi: RuntimeAbiVersion,
        states: (TypeId, [bray_symbols::ConstantValueId; 4]),
    ) -> Result<Self, MirCapacityError> {
        let outcome = Self::allocate(
            builder,
            source,
            boolean,
            report,
            unit,
            runtime_abi,
            states,
            MirStorageKind::Temporary,
        )?;

        outcome.begin(builder, block, source)?;

        Ok(outcome)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "cleanup storage retains its semantic types, state constants, ABI and callback lifetime"
    )]
    pub(crate) fn allocate(
        builder: &mut MirUnitBuilder,
        source: &MirSourceAnchor,
        boolean: TypeId,
        report: TypeId,
        unit: TypeId,
        runtime_abi: RuntimeAbiVersion,
        states: (TypeId, [bray_symbols::ConstantValueId; 4]),
        kind: MirStorageKind,
    ) -> Result<Self, MirCapacityError> {
        // Every generated operation retains the same Arc-backed source provenance.
        let mut storage = |ty| {
            builder
                .push_storage(source.clone(), kind.clone(), ty)
                .map(|id| MirPlace::new(id, [], ty))
        };

        Ok(Self {
            state: storage(states.0)?,
            retention: None,
            states: states.1,
            boolean,
            report: storage(report)?,
            incoming_report: Rc::new(Cell::new(None)),
            incoming_kind: kind,
            unit,
            runtime_abi,
        })
    }

    pub(crate) fn from_collection(
        collection: bray_ir::MirCleanupCollection,
        states: [bray_symbols::ConstantValueId; 4],
        boolean: TypeId,
        unit: TypeId,
        runtime_abi: RuntimeAbiVersion,
    ) -> Self {
        Self {
            state: collection.state().clone(),
            retention: collection.retention().cloned(),
            report: collection.report().clone(),
            incoming_report: Rc::new(Cell::new(None)),
            incoming_kind: MirStorageKind::Temporary,
            states,
            boolean,
            unit,
            runtime_abi,
        }
    }

    fn incoming_report(
        &self,
        builder: &mut MirUnitBuilder,
        source: &MirSourceAnchor,
    ) -> Result<MirPlace, MirCapacityError> {
        let storage = if let Some(storage) = self.incoming_report.get() {
            storage
        } else {
            let storage = builder.push_storage(
                source.clone(),
                self.incoming_kind.clone(),
                self.report.ty(),
            )?;

            self.incoming_report.set(Some(storage));

            storage
        };

        Ok(MirPlace::new(storage, [], self.report.ty()))
    }

    pub(crate) fn begin(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<(), MirCapacityError> {
        self.store_state(builder, block, source, MirCleanupState::Normal)?;

        self.shield(builder, block, source, RuntimeAbiRole::CleanupShieldEnter)
    }

    pub(crate) fn collection(&self) -> bray_ir::MirCleanupCollection {
        bray_ir::MirCleanupCollection::new(self.state.clone(), self.report.clone())
            .with_retention(self.retention.clone())
    }

    pub(crate) fn with_retention(
        mut self,
        retention: Option<std::sync::Arc<bray_ir::MirFrameRetention>>,
    ) -> Self {
        // A nested synchronous step must not discard its enclosing frame.
        self.retention = retention.or(self.retention);

        self
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

        self.store_state(builder, block, source, MirCleanupState::Panicked)
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

        self.store_state(builder, block, source, MirCleanupState::Cancelled)
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

        let abandoned = builder.push_block_parameter(continuation, source.clone(), self.boolean)?;

        builder.set_terminator(
            failed,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(
                continuation,
                [Self::boolean(true, self.boolean)],
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
                        [Self::boolean(branch == error, self.boolean)],
                    )),
                );
            }
        } else {
            builder.set_terminator(
                completed,
                source.clone(),
                MirTerminatorKind::Goto(MirEdge::new(
                    continuation,
                    [Self::boolean(false, self.boolean)],
                )),
            );
        }

        let pending = builder.push_block(source.clone(), builder.block_kind(continuation))?;
        let destruction = builder.push_block(source.clone(), builder.block_kind(continuation))?;

        let disposition =
            builder.push_block_parameter(destruction, source.clone(), self.boolean)?;

        builder.set_terminator(
            continuation,
            source.clone(),
            MirTerminatorKind::Branch {
                condition: MirOperand::Value(abandoned),
                then_edge: MirEdge::new(pending, []),
                else_edge: MirEdge::new(destruction, [Self::boolean(false, self.boolean)]),
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
                [Self::boolean(true, self.boolean)],
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
        let initialize = builder.push_block(source.clone(), builder.block_kind(block))?;
        let transfer = builder.push_block(source.clone(), builder.block_kind(block))?;

        self.switch(
            builder,
            block,
            source,
            [(MirCleanupState::Normal, MirEdge::new(initialize, []))],
            MirEdge::new(transfer, []),
        );

        self.store(
            builder,
            initialize,
            source,
            &self.report,
            MirOperand::Immediate {
                value: MirImmediateValue::EmptyPanicReport,
                ty: self.report.ty(),
            },
        )?;

        self.store_state(builder, initialize, source, MirCleanupState::Incidents)?;

        builder.set_terminator(
            initialize,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(transfer, [])),
        );

        builder.push_operation(
            transfer,
            source.clone(),
            MirOperationKind::Async(bray_ir::MirAsyncOperation::TransferCleanupIncident {
                incident: place,
                report: self.report.clone(),
                producer: producer.0,
                invocation: producer.1,
                runtime: MirRuntimeReference::new(
                    RuntimeAbiRole::CleanupIncidentTransfer,
                    self.runtime_abi,
                ),
            }),
            None,
        )?;

        Ok(transfer)
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
                collection: None,
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
            MirOperationKind::Quiesce {
                place: place.clone(),
                collection: None,
            },
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
        if builder.borrow_cleanup_collection(block, &self.collection()) {
            let continuation = builder.push_block(source.clone(), builder.block_kind(block))?;

            builder.set_terminator(
                block,
                source.clone(),
                MirTerminatorKind::Goto(MirEdge::new(continuation, [])),
            );

            return Ok(continuation);
        }

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
        let incoming_report = self.incoming_report(builder, source)?;
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
            incoming_report.clone(),
            None,
        )?;

        builder.set_terminator(
            completed,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(continuation, [])),
        );

        self.retain_failure(
            builder,
            panicked,
            source,
            continuation,
            MirCleanupState::Panicked,
        )?;

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
        let incoming_report = self.incoming_report(builder, source)?;
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
            incoming_report.clone(),
            value,
        )?;

        self.retain_failure(
            builder,
            cancelled,
            source,
            continuation,
            MirCleanupState::Cancelled,
        )?;

        self.retain_failure(
            builder,
            panicked,
            source,
            continuation,
            MirCleanupState::Panicked,
        )?;

        Ok((completed, value, continuation))
    }

    /// Joins an explicitly propagated tail report through the same checked-call policy.
    pub(crate) fn retain_report(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        completed: MirBlockId,
        incoming: MirCleanupState,
        report: MirOperand,
    ) -> Result<(), MirCapacityError> {
        let slot = self.incoming_report(builder, source)?;

        self.store(builder, block, source, &slot, report)?;

        self.retain_failure(builder, block, source, completed, incoming)
    }

    fn retain_failure(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        completed: MirBlockId,
        incoming: MirCleanupState,
    ) -> Result<(), MirCapacityError> {
        let incoming_report = self.incoming_report(builder, source)?;
        let kind = builder.block_kind(block);
        let primary = builder.push_block(source.clone(), kind)?;
        let suppressed = builder.push_block(source.clone(), kind)?;
        let retained = builder.push_block(source.clone(), kind)?;

        self.switch(
            builder,
            block,
            source,
            [(MirCleanupState::Normal, MirEdge::new(primary, []))],
            MirEdge::new(suppressed, []),
        );

        self.store(
            builder,
            primary,
            source,
            &self.report,
            MirOperand::Move(incoming_report.clone()),
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
                [self.report(), MirOperand::Move(incoming_report.clone())],
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

        let stronger: &[MirCleanupState] = match incoming {
            MirCleanupState::Normal => {
                panic!("retaining a report requires an incident, cancellation or panic outcome")
            }
            MirCleanupState::Incidents => &[MirCleanupState::Cancelled, MirCleanupState::Panicked],
            MirCleanupState::Cancelled => &[MirCleanupState::Panicked],
            MirCleanupState::Panicked => &[],
        };

        let commit = if stronger.is_empty() {
            retained
        } else {
            let commit = builder.push_block(source.clone(), kind)?;

            self.switch(
                builder,
                retained,
                source,
                stronger
                    .iter()
                    .map(|state| (*state, MirEdge::new(completed, []))),
                MirEdge::new(commit, []),
            );

            commit
        };

        self.store_state(builder, commit, source, incoming)?;

        builder.set_terminator(
            commit,
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
        let completed = builder.push_block(source.clone(), kind)?;
        let to_completed = forward_cleanup_result(builder, source, completed, value)?;

        self.switch(
            builder,
            block,
            source,
            [
                (MirCleanupState::Panicked, MirEdge::new(panicked, [])),
                (MirCleanupState::Cancelled, MirEdge::new(cancelled, [])),
            ],
            to_completed,
        );

        Ok(completed)
    }

    pub(crate) fn state_constants(
        values: &bray_symbols::SemanticValueStore,
        ty: TypeId,
    ) -> Result<(TypeId, [bray_symbols::ConstantValueId; 4]), bray_symbols::SemanticValueStoreError>
    {
        let value = |state: MirCleanupState| {
            let MirOperand::Constant { value, .. } =
                crate::operand::integer_constant(values, ty, u64::from(state.code()))?
            else {
                unreachable!("integer cleanup state is a constant operand");
            };

            Ok::<_, bray_symbols::SemanticValueStoreError>(value)
        };

        Ok((
            ty,
            [
                value(MirCleanupState::Normal)?,
                value(MirCleanupState::Incidents)?,
                value(MirCleanupState::Cancelled)?,
                value(MirCleanupState::Panicked)?,
            ],
        ))
    }

    fn state_value(&self, state: MirCleanupState) -> bray_symbols::ConstantValueId {
        self.states[usize::try_from(state.code()).expect("cleanup state discriminant fits usize")]
    }

    fn store_state(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        state: MirCleanupState,
    ) -> Result<(), MirCapacityError> {
        self.store(
            builder,
            block,
            source,
            &self.state,
            MirOperand::Constant {
                value: self.state_value(state),
                ty: self.state.ty(),
            },
        )
    }

    fn switch(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        cases: impl IntoIterator<Item = (MirCleanupState, MirEdge)>,
        otherwise: MirEdge,
    ) {
        builder.set_terminator(
            block,
            source.clone(),
            MirTerminatorKind::Switch {
                discriminant: MirOperand::Copy(self.state.clone()),
                cases: cases
                    .into_iter()
                    .map(|(state, edge)| bray_ir::MirSwitchCase::new(self.state_value(state), edge))
                    .collect(),
                otherwise,
            },
        );
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
