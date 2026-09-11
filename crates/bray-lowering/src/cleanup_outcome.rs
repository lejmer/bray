use bray_bound_tree::BoundCallResult;
use bray_ir::{
    MirBlockId, MirCall, MirCallPanicEdge, MirCallTarget, MirEdge, MirImmediateValue, MirOperand,
    MirOperationKind, MirPlace, MirRuntimeReference, MirSourceAnchor, MirStorageKind, MirStoreKind,
    MirTerminatorKind, MirUnitBuildError, MirUnitBuilder,
};
use bray_runtime_interface::{RuntimeAbiRole, RuntimeAbiVersion};
use bray_symbols::TypeId;

/// Retains cleanup incidents until every selected obligation has been resolved.
pub(crate) struct CleanupOutcome {
    panicked: MirPlace,
    cancelled: MirPlace,
    report: MirPlace,
    unit: TypeId,
    runtime_abi: RuntimeAbiVersion,
}

// MIR instructions own their provenance and place paths. Clones below share the Arc-backed
// paths while this builder retains the same storage identities for later incident branches.
impl CleanupOutcome {
    pub(crate) fn new(
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        boolean: TypeId,
        report: TypeId,
        unit: TypeId,
        runtime_abi: RuntimeAbiVersion,
    ) -> Result<Self, MirUnitBuildError> {
        // Every generated operation retains the same Arc-backed source provenance.
        let mut storage = |ty| {
            builder
                .push_storage(source.clone(), MirStorageKind::Temporary, ty)
                .map(|id| MirPlace::new(id, [], ty))
        };

        let outcome = Self {
            panicked: storage(boolean)?,
            cancelled: storage(boolean)?,
            report: storage(report)?,
            unit,
            runtime_abi,
        };

        for flag in [&outcome.panicked, &outcome.cancelled] {
            outcome.store(builder, block, source, flag, Self::boolean(false, boolean))?;
        }

        outcome.shield(builder, block, source, RuntimeAbiRole::CleanupShieldEnter)?;

        Ok(outcome)
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
    ) -> Result<(), MirUnitBuildError> {
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
    ) -> Result<(), MirUnitBuildError> {
        self.shield(builder, block, source, RuntimeAbiRole::CleanupShieldLeave)
    }

    pub(crate) fn initialize_cancellation(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<(), MirUnitBuildError> {
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
    ) -> Result<MirBlockId, MirUnitBuildError> {
        let kind = builder.block_kind(block)?;
        let completed = builder.push_block(source.clone(), kind)?;
        let panicked = builder.push_block(source.clone(), kind)?;
        let cancelled = builder.push_block(source.clone(), kind)?;
        let report = builder.push_block_parameter(panicked, source.clone(), self.report.ty())?;

        builder.set_terminator(
            block,
            source.clone(),
            MirTerminatorKind::CheckCallOutcome {
                completed: MirEdge::new(completed, []),
                panicked: MirCallPanicEdge::new(panicked, self.report.ty()),
                cancelled: MirEdge::new(cancelled, []),
            },
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
        )?;

        self.retain_panic(
            builder,
            panicked,
            source,
            MirOperand::Value(report),
            completed,
        )?;

        Ok(completed)
    }

    fn retain_panic(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        report: MirOperand,
        completed: MirBlockId,
    ) -> Result<(), MirUnitBuildError> {
        let kind = builder.block_kind(block)?;
        let primary = builder.push_block(source.clone(), kind)?;
        let suppressed = builder.push_block(source.clone(), kind)?;

        let primary_report =
            builder.push_block_parameter(primary, source.clone(), self.report.ty())?;

        let suppressed_report =
            builder.push_block_parameter(suppressed, source.clone(), self.report.ty())?;

        builder.set_terminator(
            block,
            source.clone(),
            MirTerminatorKind::Branch {
                condition: MirOperand::Copy(self.panicked.clone()),
                // Only the selected edge transfers the newly reported incident.
                then_edge: MirEdge::new(suppressed, [report.clone()]),
                else_edge: MirEdge::new(primary, [report]),
            },
        )?;

        self.store(
            builder,
            primary,
            source,
            &self.report,
            MirOperand::Value(primary_report),
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
        )?;

        let merged = builder.push_operation(
            suppressed,
            source.clone(),
            MirOperationKind::Call(MirCall::protocol(
                MirCallTarget::Runtime(MirRuntimeReference::new(
                    RuntimeAbiRole::PanicReportSuppression,
                    self.runtime_abi,
                )),
                BoundCallResult::Immediate(self.report.ty()),
                [self.report(), MirOperand::Value(suppressed_report)],
                [],
            )),
            Some(self.report.ty()),
        )?;

        let value = merged
            .result()
            .ok_or(MirUnitBuildError::MissingOperationResult(
                merged.operation(),
            ))?;

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
        )
    }

    /// Branches to caller-owned completion paths, with panic taking precedence.
    pub(crate) fn dispatch(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        panicked: MirBlockId,
        cancelled: MirBlockId,
    ) -> Result<MirBlockId, MirUnitBuildError> {
        self.end_shield(builder, block, source)?;
        let kind = builder.block_kind(block)?;
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
        )?;

        builder.set_terminator(
            no_panic,
            source.clone(),
            MirTerminatorKind::Branch {
                condition: MirOperand::Copy(self.cancelled.clone()),
                then_edge: MirEdge::new(cancelled, []),
                else_edge: MirEdge::new(completed, []),
            },
        )?;

        Ok(completed)
    }

    fn shield(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        role: RuntimeAbiRole,
    ) -> Result<(), MirUnitBuildError> {
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
    ) -> Result<(), MirUnitBuildError> {
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
