use std::collections::BTreeMap;

use bray_bound_tree::{AnyBoundNodeId, AsyncScopeExitPlan, BoundBlockId};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCallPanicEdge, MirCleanupEdge, MirCleanupPhase, MirEdge,
    MirOperand, MirOperationKind, MirPlace, MirSourceAnchor, MirStorageKind, MirStoreKind,
    MirTerminatorKind,
};
use bray_symbols::TypeId;

use super::control::{CleanupDestination, TerminalState};
use crate::lowering::LoweringError;
use crate::lowering::construction::ConstructionExit;
use crate::lowering::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn finish_ordinary_cleanup(
        &mut self,
        current: MirBlockId,
        source: &MirSourceAnchor,
        destination: CleanupDestination,
        value: Option<(MirOperand, TypeId)>,
        exit: AnyBoundNodeId,
        plans: &[AsyncScopeExitPlan],
        construction_exit: ConstructionExit,
    ) -> Result<(), LoweringError> {
        let pending = match value {
            Some((value, ty)) => {
                let kind = if matches!(destination, CleanupDestination::Return) {
                    MirStorageKind::Return
                } else {
                    MirStorageKind::Temporary
                };

                let storage = self
                    .builder
                    .push_storage(Self::retained_source(source), kind, ty)?;

                let place = MirPlace::new(storage, [], ty);

                self.push_operation(
                    current,
                    Self::retained_source(source),
                    MirOperationKind::Store {
                        kind: MirStoreKind::Initialize,
                        destination: Self::retained_place(&place),
                        value,
                    },
                    None,
                )?;

                Some(place)
            }
            None => None,
        };

        if let Some(pending) = &pending {
            self.cleanup_retained_storages.push(pending.storage());
        }

        let failures = self.ordinary_cleanup_failures(source, exit, plans, pending.as_ref())?;

        let broadcast = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::CleanupBroadcast,
        )?;

        self.set_terminator(
            current,
            Self::retained_source(source),
            MirTerminatorKind::BeginCleanup(MirCleanupEdge::new(
                MirCleanupPhase::TaskCancellation,
                MirEdge::new(broadcast, []),
            )),
        )?;

        let lifecycle =
            self.resolve_cleanup(broadcast, source, plans, None, &failures, construction_exit)?;

        if pending.is_some() {
            self.cleanup_retained_storages.pop();
        }

        self.set_destination(
            lifecycle,
            source,
            destination,
            pending
                .as_ref()
                .map(|place| MirOperand::Move(Self::retained_place(place))),
        )
    }

    fn ordinary_cleanup_failures(
        &mut self,
        source: &MirSourceAnchor,
        exit: AnyBoundNodeId,
        plans: &[AsyncScopeExitPlan],
        pending: Option<&MirPlace>,
    ) -> Result<BTreeMap<BoundBlockId, (MirBlockId, MirBlockId, TypeId)>, LoweringError> {
        let report_type = self.representation_type(RepresentationRole::PanicReport)?;

        let cancelled = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let destination = if self.input.unit_kind().protected_frame().is_some() {
            CleanupDestination::Goto(self.terminal_state_block(source, TerminalState::Cancelled)?)
        } else {
            CleanupDestination::PropagateCancellation
        };

        let remaining = self.cleanup_plans(0, exit)?;
        self.finish_abnormal_cleanup(cancelled, source, None, destination, &remaining, pending)?;

        let mut handlers = BTreeMap::new();
        let mut failures = BTreeMap::new();

        for plan in plans {
            let index = self
                .active_scopes
                .iter()
                .position(|scope| *scope == plan.scope())
                .ok_or(LoweringError::MissingBoundNode(plan.scope().into()))?;

            let catch = self
                .catch_targets
                .iter()
                .rev()
                .find(|target| target.scope_depth <= index)
                .map(|target| (target.block, target.scope_depth));

            let panicked = if let Some(block) = handlers.get(&catch) {
                *block
            } else {
                let panicked = self.builder.push_block(
                    Self::retained_source(source),
                    MirBlockKind::LifecycleResolution,
                )?;

                let report = self.builder.push_block_parameter(
                    panicked,
                    Self::retained_source(source),
                    report_type,
                )?;

                let (destination, depth) =
                    match catch {
                        Some((block, depth)) => (CleanupDestination::Goto(block), depth),
                        None if self.input.unit_kind().protected_frame().is_some() => (
                            CleanupDestination::Goto(self.terminal_state_block(
                                source,
                                TerminalState::Panicked(report_type),
                            )?),
                            0,
                        ),
                        None => (CleanupDestination::PropagatePanic, 0),
                    };

                let remaining = self.cleanup_plans(depth, exit)?;

                self.finish_abnormal_cleanup(
                    panicked,
                    source,
                    Some(MirOperand::Value(report)),
                    destination,
                    &remaining,
                    pending,
                )?;

                handlers.insert(catch, panicked);

                panicked
            };

            failures.insert(plan.scope(), (panicked, cancelled, report_type));
        }

        Ok(failures)
    }

    pub(super) fn check_ordinary_cleanup(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<MirBlockId, LoweringError> {
        let Some((panicked, cancelled, report_type)) = self.cleanup_failure_targets else {
            return Ok(block);
        };

        let kind = self.builder.block_kind(block)?;

        let completed = self
            .builder
            .push_block(Self::retained_source(source), kind)?;

        let (panicked, cancelled) = if kind == MirBlockKind::CleanupBroadcast {
            let panic_bridge = self
                .builder
                .push_block(Self::retained_source(source), kind)?;

            let cancel_bridge = self
                .builder
                .push_block(Self::retained_source(source), kind)?;

            let report = self.builder.push_block_parameter(
                panic_bridge,
                Self::retained_source(source),
                report_type,
            )?;

            self.set_terminator(
                panic_bridge,
                Self::retained_source(source),
                MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                    MirCleanupPhase::LifecycleResolution,
                    MirEdge::new(panicked, [MirOperand::Value(report)]),
                )),
            )?;

            self.set_terminator(
                cancel_bridge,
                Self::retained_source(source),
                MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                    MirCleanupPhase::LifecycleResolution,
                    MirEdge::new(cancelled, []),
                )),
            )?;

            (panic_bridge, cancel_bridge)
        } else {
            (panicked, cancelled)
        };

        self.set_terminator(
            block,
            Self::retained_source(source),
            MirTerminatorKind::CheckCallOutcome {
                completed: MirEdge::new(completed, []),
                panicked: MirCallPanicEdge::new(panicked, report_type),
                cancelled: MirEdge::new(cancelled, []),
            },
        )?;

        Ok(completed)
    }
}
