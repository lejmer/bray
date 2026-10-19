use super::model::CleanupOutcome;
use crate::continuation::forward_cleanup_result;
use bray_bound_tree::BoundCallResult;
use bray_ir::{
    MirBlockId, MirCall, MirCallTarget, MirCapacityError, MirCleanupState, MirEdge,
    MirImmediateValue, MirOperand, MirOperationKind, MirPlace, MirRuntimeReference,
    MirSourceAnchor, MirStoreKind, MirTerminatorKind, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::TypeId;

impl CleanupOutcome {
    /// Initializes retained source reports without shielding ordinary execution.
    pub(crate) fn initialize_pending(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<(), MirCapacityError> {
        self.store_state(builder, block, source, MirCleanupState::Normal)?;

        self.store(
            builder,
            block,
            source,
            &self.report,
            MirOperand::Immediate {
                value: MirImmediateValue::EmptyPanicReport,
                ty: self.report.ty(),
            },
        )
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

    pub(super) fn state_value(&self, state: MirCleanupState) -> bray_symbols::ConstantValueId {
        self.states[usize::try_from(state.code()).expect("cleanup state discriminant fits usize")]
    }

    pub(super) fn store_state(
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

    pub(super) fn switch(
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

    pub(super) fn shield(
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

    pub(super) fn store(
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

    pub(super) const fn boolean(value: bool, ty: TypeId) -> MirOperand {
        MirOperand::Immediate {
            value: MirImmediateValue::Boolean(value),
            ty,
        }
    }
}
