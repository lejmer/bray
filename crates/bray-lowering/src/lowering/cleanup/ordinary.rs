use std::collections::BTreeMap;

use bray_bound_tree::{AnyBoundNodeId, AsyncScopeExitPlan, BoundBlockId};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge, MirOperand,
    MirOperationKind, MirPlace, MirSourceAnchor, MirStorageKind, MirStoreKind, MirTerminatorKind,
};
use bray_symbols::TypeId;

use super::control::{CleanupDestination, TerminalState};
use crate::lowering::LoweringError;
use crate::lowering::inputs::InputExit;
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
        input_exit: InputExit,
    ) -> Result<(), LoweringError> {
        let pending = match value {
            Some((value, ty)) => {
                let kind = if matches!(destination, CleanupDestination::Return) {
                    MirStorageKind::Return
                } else {
                    MirStorageKind::Temporary
                };

                let place = self.cleanup_transfer_place(source, kind, ty)?;

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
            self.resolve_cleanup(broadcast, source, plans, None, &failures, input_exit)?;

        self.set_destination(
            lifecycle,
            source,
            destination,
            pending
                .as_ref()
                .map(|place| MirOperand::Move(Self::retained_place(place))),
        )
    }

    fn cleanup_transfer_place(
        &mut self,
        source: &MirSourceAnchor,
        kind: MirStorageKind,
        ty: TypeId,
    ) -> Result<MirPlace, LoweringError> {
        let key = (kind.clone(), ty);

        if let Some(place) = self.cleanup_transfer_places.get(&key) {
            return Ok(Self::retained_place(place));
        }

        let storage = self
            .builder
            .push_storage(Self::retained_source(source), kind, ty)?;

        let place = MirPlace::new(storage, [], ty);

        self.cleanup_transfer_places
            .insert(key, Self::retained_place(&place));

        Ok(place)
    }

    fn ordinary_cleanup_failures(
        &mut self,
        source: &MirSourceAnchor,
        exit: AnyBoundNodeId,
        plans: &[AsyncScopeExitPlan],
        pending: Option<&MirPlace>,
    ) -> Result<BTreeMap<BoundBlockId, (MirBlockId, MirBlockId, MirPlace)>, LoweringError> {
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

        let remaining = self.cleanup_plans(0, exit);

        self.finish_abnormal_cleanup(cancelled, source, None, destination, &remaining, pending)?;

        let mut handlers = BTreeMap::new();
        let mut failures = BTreeMap::new();

        for plan in plans {
            let index = self
                .active_scopes
                .iter()
                .position(|scope| *scope == plan.scope())
                .unwrap_or_else(|| {
                    panic!(
                        "lowering contract violation: MissingBoundNode {value:?}",
                        value = plan.scope()
                    )
                });

            let catch = self
                .catch_targets
                .iter()
                .rev()
                .find(|target| target.scope_depth <= index)
                .map(|target| (target.block, target.scope_depth));

            let (panicked, report) = if let Some((block, report)) = handlers.get(&catch) {
                (*block, Self::retained_place(report))
            } else {
                let panicked = self.builder.push_block(
                    Self::retained_source(source),
                    MirBlockKind::LifecycleResolution,
                )?;

                let report_storage = self.builder.push_storage(
                    Self::retained_source(source),
                    MirStorageKind::Temporary,
                    report_type,
                )?;

                let report = MirPlace::new(report_storage, [], report_type);

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

                let remaining = self.cleanup_plans(depth, exit);

                self.finish_abnormal_cleanup(
                    panicked,
                    source,
                    Some(MirOperand::Move(Self::retained_place(&report))),
                    destination,
                    &remaining,
                    pending,
                )?;

                handlers.insert(catch, (panicked, Self::retained_place(&report)));

                (panicked, report)
            };

            failures.insert(plan.scope(), (panicked, cancelled, report));
        }

        Ok(failures)
    }

    pub(super) fn check_ordinary_cleanup(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        released_owner: Option<TypeId>,
    ) -> Result<MirBlockId, LoweringError> {
        let Some((mut panicked, mut cancelled, report)) = self.cleanup_failure_targets.clone()
        else {
            return Ok(block);
        };

        if let Some(owner) = released_owner {
            let kind = self.builder.block_kind(block);

            for target in [&mut panicked, &mut cancelled] {
                let edge = self
                    .builder
                    .push_block(Self::retained_source(source), kind)?;

                self.discharge_outgoing_owner(edge, source, owner)?;

                self.set_terminator(
                    edge,
                    Self::retained_source(source),
                    MirTerminatorKind::Goto(MirEdge::new(*target, [])),
                )?;

                *target = edge;
            }
        }

        crate::cleanup_outcome::check_call_outcome(
            &mut self.builder,
            block,
            source,
            panicked,
            cancelled,
            report,
        )
        .map_err(Into::into)
    }
}
