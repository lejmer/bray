use bray_ir::{MirBlockId, MirCapacityError, MirCleanupState, MirEdge, MirOperand, MirSourceAnchor, MirTerminatorKind, MirUnitBuilder};
use crate::continuation::{check_call_outcome};
use super::model::CleanupOutcome;

impl CleanupOutcome {
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

        let (completed, _, continuation) = self.check_completion(builder, block, source, None, false)?;

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
            MirEdge::new(continuation, []),
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
        reports: bool,
    ) -> Result<(MirBlockId, Option<MirOperand>, MirBlockId), MirCapacityError> {
        let incoming_report = self.incoming_report(builder, source)?;
        let kind = builder.block_kind(block);
        let panicked = builder.push_block(source.clone(), kind)?;
        let cancelled = builder.push_block(source.clone(), kind)?;
        let continuation = builder.push_block(source.clone(), kind)?;

        let (completed, mut value) = check_call_outcome(
            builder,
            block,
            source,
            panicked,
            cancelled,
            incoming_report.clone(),
            reports.then(|| incoming_report.clone()),
            value,
        )?;

        let completed = if reports {
            self.retain_completed_report(builder, completed, source, &mut value)?
        } else { completed };

        self.retain_failure(
            builder,
            cancelled,
            source,
            MirEdge::new(continuation, []),
            MirCleanupState::Cancelled,
        )?;

        self.retain_failure(
            builder,
            panicked,
            source,
            MirEdge::new(continuation, []),
            MirCleanupState::Panicked,
        )?;

        Ok((completed, value, continuation))
    }
}
