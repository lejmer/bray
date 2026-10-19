use super::model::CleanupOutcome;
use crate::continuation::{forward_cleanup_edge, forward_cleanup_result};
use bray_bound_tree::BoundCallResult;
use bray_ir::{
    MirBlockId, MirCall, MirCallTarget, MirCapacityError, MirCleanupState, MirEdge, MirOperand,
    MirOperationKind, MirRuntimeReference, MirSourceAnchor, MirTerminatorKind, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;

impl CleanupOutcome {
    /// Forwards normal incidents to the collection selected by checked ownership flow.
    pub(crate) fn forward_incidents(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        target: Option<&Self>,
        value: &mut Option<MirOperand>,
    ) -> Result<MirBlockId, MirCapacityError> {
        let completed = builder.push_block(source.clone(), builder.block_kind(block))?;
        let transfer = builder.push_block(source.clone(), builder.block_kind(block))?;

        let incoming = MirEdge::new(completed, value.take());

        let (to_transfer, from_transfer) =
            forward_cleanup_edge(builder, source, transfer, &incoming)?;

        let (to_completed, from_completed) =
            forward_cleanup_edge(builder, source, completed, &incoming)?;

        *value = from_completed.arguments().first().cloned();

        self.switch(
            builder,
            block,
            source,
            [(MirCleanupState::Incidents, to_transfer)],
            to_completed,
        );

        match target {
            Some(target) => target.retain_report(
                builder,
                transfer,
                source,
                from_transfer,
                MirCleanupState::Incidents,
                self.report(),
            )?,
            None => {
                // The owning flow proves that this collection cannot forward incidents here.
                builder.set_terminator(transfer, source.clone(), MirTerminatorKind::Unreachable);
            }
        }

        Ok(completed)
    }

    /// Retains a normal report already transferred into the incoming slot.
    pub(crate) fn retain_completed_report(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        value: &mut Option<MirOperand>,
    ) -> Result<MirBlockId, MirCapacityError> {
        let completed = builder.push_block(source.clone(), builder.block_kind(block))?;
        let edge = forward_cleanup_result(builder, source, completed, value)?;

        self.retain_failure(builder, block, source, edge, MirCleanupState::Incidents)?;

        Ok(completed)
    }

    /// Transfers a normal-only invocation outcome without checking impossible failures.
    pub(crate) fn take_completed_report(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        value: &mut Option<MirOperand>,
    ) -> Result<MirBlockId, MirCapacityError> {
        let report = self.incoming_report(builder, source)?;
        let reported = builder.push_block(source.clone(), builder.block_kind(block))?;
        let completed = forward_cleanup_result(builder, source, reported, value)?;

        builder.set_terminator(
            block,
            source.clone(),
            MirTerminatorKind::TakeCallReport { completed, report },
        );

        self.retain_completed_report(builder, reported, source, value)
    }

    /// Joins an explicitly propagated tail report through the same checked-call policy.
    pub(crate) fn retain_report(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        completed: MirEdge,
        incoming: MirCleanupState,
        report: MirOperand,
    ) -> Result<(), MirCapacityError> {
        let slot = self.incoming_report(builder, source)?;

        self.store(builder, block, source, &slot, report)?;

        self.retain_failure(builder, block, source, completed, incoming)
    }

    pub(super) fn retain_failure(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        completed: MirEdge,
        incoming: MirCleanupState,
    ) -> Result<(), MirCapacityError> {
        let incoming_report = self.incoming_report(builder, source)?;
        let kind = builder.block_kind(block);
        let primary = builder.push_block(source.clone(), kind)?;
        let suppressed = builder.push_block(source.clone(), kind)?;
        let retained = builder.push_block(source.clone(), kind)?;

        let (to_primary, primary_result) =
            forward_cleanup_edge(builder, source, primary, &completed)?;

        let (to_suppressed, suppressed_result) =
            forward_cleanup_edge(builder, source, suppressed, &completed)?;

        let (to_retained, retained_result) =
            forward_cleanup_edge(builder, source, retained, &primary_result)?;

        self.switch(
            builder,
            block,
            source,
            [(MirCleanupState::Normal, to_primary)],
            to_suppressed,
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
            MirTerminatorKind::Goto(to_retained),
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
            MirTerminatorKind::Goto(MirEdge::new(
                retained,
                suppressed_result.arguments().iter().cloned(),
            )),
        );

        let stronger: &[MirCleanupState] = match incoming {
            MirCleanupState::Normal => {
                panic!("retaining a report requires an incident, cancellation or panic outcome")
            }
            MirCleanupState::Incidents => &[MirCleanupState::Cancelled, MirCleanupState::Panicked],
            MirCleanupState::Cancelled => &[MirCleanupState::Panicked],
            MirCleanupState::Panicked => &[],
        };

        let (commit, completed) = if stronger.is_empty() {
            (retained, retained_result)
        } else {
            let commit = builder.push_block(source.clone(), kind)?;

            let (to_commit, commit_result) =
                forward_cleanup_edge(builder, source, commit, &retained_result)?;

            self.switch(
                builder,
                retained,
                source,
                stronger
                    .iter()
                    .map(|state| (*state, retained_result.clone())),
                to_commit,
            );

            (commit, commit_result)
        };

        self.store_state(builder, commit, source, incoming)?;

        builder.set_terminator(commit, source.clone(), MirTerminatorKind::Goto(completed));

        Ok(())
    }
}
