use bray_bound_tree::BoundCallResult;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCall, MirCallPanicEdge, MirCallTarget, MirCapacityError,
    MirCleanupEdge, MirCleanupPhase, MirEdge, MirImmediateValue, MirOperand, MirOperationKind,
    MirPlace, MirRuntimeReference, MirSourceAnchor, MirStorageKind, MirStoreKind,
    MirTerminatorKind, MirUnitBuilder,
};
use bray_runtime_interface::{RuntimeAbiRole, RuntimeAbiVersion};
use bray_symbols::TypeId;

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
        let outcome = Self::allocate(builder, source, boolean, report, unit, runtime_abi)?;
        outcome.begin(builder, block, source)?;

        Ok(outcome)
    }

    pub(crate) fn allocate(
        builder: &mut MirUnitBuilder,
        source: &MirSourceAnchor,
        boolean: TypeId,
        report: TypeId,
        unit: TypeId,
        runtime_abi: RuntimeAbiVersion,
    ) -> Result<Self, MirCapacityError> {
        // Every generated operation retains the same Arc-backed source provenance.
        let mut storage = |ty| {
            builder
                .push_storage(source.clone(), MirStorageKind::Temporary, ty)
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
    ) -> Result<(), MirCapacityError> {
        self.store(
            builder,
            block,
            source,
            &self.cancelled,
            Self::boolean(true, self.cancelled.ty()),
        )
    }

    /// Adds incident continuations after the block's last fallible cleanup operation.
    pub(crate) fn check(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<MirBlockId, MirCapacityError> {
        let kind = builder.block_kind(block);
        let panicked = builder.push_block(source.clone(), kind)?;
        let cancelled = builder.push_block(source.clone(), kind)?;

        let completed = check_call_outcome(
            builder,
            block,
            source,
            panicked,
            cancelled,
            self.incoming_report.clone(),
        )?;

        self.store(
            builder,
            cancelled,
            source,
            &self.cancelled,
            Self::boolean(true, self.cancelled.ty()),
        )?;

        builder.set_terminator(
            cancelled,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(completed, [])),
        );

        self.retain_panic(builder, panicked, source, completed)?;

        Ok(completed)
    }

    fn retain_panic(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        completed: MirBlockId,
    ) -> Result<(), MirCapacityError> {
        let kind = builder.block_kind(block);
        let primary = builder.push_block(source.clone(), kind)?;
        let suppressed = builder.push_block(source.clone(), kind)?;

        builder.set_terminator(
            block,
            source.clone(),
            MirTerminatorKind::Branch {
                condition: MirOperand::Copy(self.panicked.clone()),
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

        self.store(
            builder,
            primary,
            source,
            &self.panicked,
            Self::boolean(true, self.panicked.ty()),
        )?;

        builder.set_terminator(
            primary,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(completed, [])),
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
    ) -> Result<MirBlockId, MirCapacityError> {
        self.end_shield(builder, block, source)?;
        let kind = builder.block_kind(block);
        let no_panic = builder.push_block(source.clone(), kind)?;
        let completed = builder.push_block(source.clone(), kind)?;

        builder.set_terminator(
            block,
            source.clone(),
            MirTerminatorKind::Branch {
                condition: MirOperand::Copy(self.panicked.clone()),
                then_edge: MirEdge::new(panicked, []),
                else_edge: MirEdge::new(no_panic, []),
            },
        );

        builder.set_terminator(
            no_panic,
            source.clone(),
            MirTerminatorKind::Branch {
                condition: MirOperand::Copy(self.cancelled.clone()),
                then_edge: MirEdge::new(cancelled, []),
                else_edge: MirEdge::new(completed, []),
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
) -> Result<MirBlockId, MirCapacityError> {
    let kind = builder.block_kind(block);

    let completed = builder.push_block(source.clone(), kind)?;

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
            completed: MirEdge::new(completed, []),
            panicked: MirCallPanicEdge::new(panicked, report),
            cancelled: MirEdge::new(cancelled, []),
        },
    );

    Ok(completed)
}
