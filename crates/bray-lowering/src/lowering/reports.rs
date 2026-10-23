use bray_bound_tree::{AnyBoundNodeId, BoundExpressionId};
use bray_ir::{MirBlockId, MirCleanupState, MirOperand, MirSourceAnchor};

use crate::cleanup_outcome::{CleanupFailure, CleanupOutcome};
use crate::lowering::cleanup::CleanupDestination;
use crate::lowering::{LoweringError, lowerer::Lowerer};

impl Lowerer<'_> {
    pub(in crate::lowering) fn completion_report_outcome(
        &self,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) -> Option<&CleanupOutcome> {
        if !self.input.storage_flow().completion_reports(occurrence) {
            return None;
        }

        Some(
            self.catch_targets
                .last()
                .map_or(self.pending_reports.as_ref(), |catch| {
                    catch.reports.as_ref()
                })
                .unwrap_or_else(|| {
                    panic!(
                        "report-producing completion retains its checked collection: {occurrence:?}"
                    )
                }),
        )
    }

    pub(in crate::lowering) fn take_completion_report(
        &mut self,
        occurrence: bray_bound_tree::BoundExecutionSite,
        block: MirBlockId,
        source: &MirSourceAnchor,
        value: &mut Option<MirOperand>,
    ) -> Result<MirBlockId, LoweringError> {
        // This view shares only the selected collection's immutable storage identities.
        let outcome = self.completion_report_outcome(occurrence).cloned();

        match outcome {
            Some(outcome) => outcome
                .take_completed_report(&mut self.builder, block, source, value)
                .map_err(Into::into),
            None => Ok(block),
        }
    }

    pub(super) fn allocate_pending_reports(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        boundary: Option<BoundExpressionId>,
    ) -> Result<Option<CleanupOutcome>, LoweringError> {
        if !self.input.storage_flow().has_report_collection(boundary) {
            return Ok(None);
        }

        let outcome = self.allocate_cleanup_outcome(source)?;

        outcome.initialize_pending(&mut self.builder, block, source)?;

        Ok(Some(outcome))
    }

    pub(super) fn pending_report(&self) -> Option<MirOperand> {
        self.pending_reports
            .as_ref()
            .filter(|_| self.completion_cleanup.is_none())
            .map(CleanupOutcome::report)
    }

    pub(super) fn forward_cleanup_incidents(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        outcome: &CleanupOutcome,
        value: &mut Option<MirOperand>,
    ) -> Result<MirBlockId, LoweringError> {
        let target = if self.completion_cleanup.is_some() {
            None
        } else {
            self.catch_targets
                .last()
                .map_or(self.pending_reports.as_ref(), |catch| {
                    catch.reports.as_ref()
                })
        };

        outcome
            .forward_incidents(&mut self.builder, block, source, target, value)
            .map_err(Into::into)
    }
    pub(in crate::lowering) fn set_ordinary_destination(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        destination: CleanupDestination,
        mut value: Option<MirOperand>,
        scope_depth: usize,
        exit: AnyBoundNodeId,
    ) -> Result<(), LoweringError> {
        let start = if self.completion_cleanup.is_some() || matches!(exit, AnyBoundNodeId::Block(_))
        {
            self.catch_targets.len()
        } else if let CleanupDestination::Goto(target) = destination
            && let Some(index) = self
                .catch_targets
                .iter()
                .position(|catch| catch.success == target)
        {
            index + 1
        } else {
            self.catch_targets
                .iter()
                .position(|catch| catch.scope_depth >= scope_depth)
                .unwrap_or(self.catch_targets.len())
        };

        let target = if start == 0 {
            self.pending_reports.as_ref()
        } else {
            self.catch_targets[start - 1].reports.as_ref()
        };

        let mut block = block;

        for catch in &self.catch_targets[start..] {
            if let Some(reports) = &catch.reports {
                block = reports.forward_incidents(
                    &mut self.builder,
                    block,
                    source,
                    target,
                    &mut value,
                )?;
            }
        }

        self.set_destination(block, source, destination, value)
    }

    fn pending_failure_target(
        &self,
        destination: CleanupDestination,
        panicking: bool,
    ) -> Option<usize> {
        match destination {
            CleanupDestination::Goto(target) if panicking => self
                .catch_targets
                .iter()
                .position(|catch| catch.block == target),
            CleanupDestination::Goto(_)
            | CleanupDestination::Return
            | CleanupDestination::PropagatePanic
            | CleanupDestination::PropagateCancellation => None,
        }
    }

    pub(in crate::lowering) fn has_pending_failure_reports(
        &self,
        destination: CleanupDestination,
        panicking: bool,
    ) -> bool {
        if self.completion_cleanup.is_some() {
            return false;
        }

        let target = self.pending_failure_target(destination, panicking);

        (target.is_none() && self.pending_reports.is_some())
            || self.catch_targets[target.unwrap_or(0)..]
                .iter()
                .any(|catch| catch.reports.is_some())
    }

    pub(in crate::lowering) fn seed_pending_failure(
        &mut self,
        outcome: &CleanupOutcome,
        block: MirBlockId,
        source: &MirSourceAnchor,
        failure: CleanupFailure,
        destination: CleanupDestination,
    ) -> Result<MirBlockId, LoweringError> {
        let panicking = matches!(failure, CleanupFailure::Panic(_));

        if !self.has_pending_failure_reports(destination, panicking) {
            match failure {
                CleanupFailure::Panic(report) => {
                    outcome.initialize_panic(&mut self.builder, block, source, report)?
                }
                CleanupFailure::Cancellation(report) => {
                    outcome.initialize_cancellation(&mut self.builder, block, source, report)?
                }
            }

            return Ok(block);
        }

        let target = self.pending_failure_target(destination, panicking);

        let (state, report) = match failure {
            CleanupFailure::Panic(report) => (MirCleanupState::Panicked, report),
            CleanupFailure::Cancellation(report) => (
                MirCleanupState::Cancelled,
                report.unwrap_or(bray_ir::MirOperand::Immediate {
                    value: bray_ir::MirImmediateValue::EmptyPanicReport,
                    ty: self.builder.operand_type(&outcome.report()),
                }),
            ),
        };

        let mut value = Some(report);
        let mut block = block;

        if self.completion_cleanup.is_none()
            && target.is_none()
            && let Some(reports) = &self.pending_reports
        {
            block = reports.forward_incidents(
                &mut self.builder,
                block,
                source,
                Some(outcome),
                &mut value,
            )?;
        }

        for catch in self.catch_targets[target.unwrap_or(0)..]
            .iter()
            .filter(|_| self.completion_cleanup.is_none())
        {
            if let Some(reports) = &catch.reports {
                block = reports.forward_incidents(
                    &mut self.builder,
                    block,
                    source,
                    Some(outcome),
                    &mut value,
                )?;
            }
        }

        let completed = self.builder.push_block(
            Self::retained_source(source),
            self.builder.block_kind(block),
        )?;

        outcome.retain_report(
            &mut self.builder,
            block,
            source,
            bray_ir::MirEdge::new(completed, []),
            state,
            value.expect("pending failure retains its report"),
        )?;

        Ok(completed)
    }
}
