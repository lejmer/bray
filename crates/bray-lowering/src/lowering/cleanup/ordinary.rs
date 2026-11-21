use bray_bound_tree::{AnyBoundNodeId, StorageExitDecision, StorageExitPoint};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirEdge, MirOperand,
    MirOperationKind, MirPlace, MirSourceAnchor, MirStorageKind, MirStoreKind, MirTerminatorKind,
};
use bray_symbols::{DependencyFailureKind, TypeId};

use super::control::{CleanupDestination, TerminalState};
use super::steps::{CleanupStep, ScheduledCleanup};
use crate::cleanup_outcome::CleanupFailure;
use crate::lowering::LoweringError;
use crate::lowering::inputs::InputExit;
use crate::lowering::lowerer::Lowerer;

pub(super) struct OrdinaryCleanup<'a> {
    exit: AnyBoundNodeId,
    broadcast_depth: usize,
    pending: Option<&'a MirPlace>,
    report: Option<MirPlace>,
}

pub(in crate::lowering) struct OrdinaryCleanupFailure {
    report: MirPlace,
    remaining: Vec<CleanupStep>,
}

pub(in crate::lowering) struct CleanupFailureTargets {
    pub(in crate::lowering) panicked: MirBlockId,
    pub(in crate::lowering) cancelled: MirBlockId,
    pub(in crate::lowering) report: MirPlace,
}

pub(in crate::lowering) struct CleanupContinuation {
    failures: [MirBlockId; 2],
    prefix: Vec<CleanupStep>,
}

impl Lowerer<'_> {
    #[expect(
        clippy::too_many_arguments,
        reason = "ordinary exits join their checked cleanup plans, destination, input exit, and retained value"
    )]
    pub(super) fn finish_ordinary_cleanup(
        &mut self,
        current: MirBlockId,
        source: &MirSourceAnchor,
        destination: CleanupDestination,
        value: Option<(MirOperand, TypeId)>,
        exit: AnyBoundNodeId,
        plans: &[&StorageExitDecision],
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

        let InputExit::Scope(scope_depth) = input_exit else {
            unreachable!("ordinary scope cleanup has its lexical destination depth");
        };

        let mut ordinary = OrdinaryCleanup {
            exit,
            broadcast_depth: scope_depth,
            pending: pending.as_ref(),
            report: None,
        };

        let broadcast = self.begin_cleanup_broadcast(current, source)?;

        let lifecycle = self.resolve_cleanup(
            broadcast,
            source,
            plans,
            pending.as_ref(),
            Some(&mut ordinary),
            input_exit,
        )?;

        self.set_ordinary_destination(
            lifecycle,
            source,
            destination,
            pending
                .as_ref()
                .map(|place| MirOperand::Move(Self::retained_place(place))),
            scope_depth,
            exit,
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

    pub(super) fn prepare_cleanup_failure(
        &mut self,
        source: &MirSourceAnchor,
        ordinary: &mut OrdinaryCleanup<'_>,
        reached: &ScheduledCleanup<'_>,
    ) -> Result<Option<OrdinaryCleanupFailure>, LoweringError> {
        let Some(scope) = reached.scope else { return Ok(None); };

        let point = StorageExitPoint::new(scope, ordinary.exit);

        let [can_panic, can_cancel] = DependencyFailureKind::ALL.map(|kind| reached.can_fail(self, point, kind));

        if !can_panic && !can_cancel {
            return Ok(None);
        }

        let report = match &ordinary.report {
            Some(report) => Self::retained_place(report),
            None => {
                let ty = self.representation_type(RepresentationRole::PanicReport)?;
                let storage = self.builder.push_storage(Self::retained_source(source), MirStorageKind::Temporary, ty)?;
                let report = MirPlace::new(storage, [], ty);

                ordinary.report = Some(Self::retained_place(&report));

                report
            }
        };

        Ok(Some(OrdinaryCleanupFailure { report, remaining: reached.after_fault().into_iter().collect() }))
    }

    pub(in crate::lowering) fn with_cleanup_remainder<R>(
        &mut self,
        remaining: impl FnOnce() -> Option<CleanupStep>,
        body: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let retained = self.ordinary_cleanup_failure.as_mut().is_some_and(|failure| {
            let Some(remaining) = remaining() else { return false; };

            failure.remaining.push(remaining);

            true
        });

        let result = body(self);

        if retained {
            self.ordinary_cleanup_failure.as_mut().expect("nested cleanup restores the enclosing failure context").remaining.pop();
        }

        result
    }

    pub(in crate::lowering) fn register_cleanup_continuation(
        &mut self,
        source: &MirSourceAnchor,
        failed_finalizer: Option<&MirPlace>,
    ) -> Result<Option<CleanupFailureTargets>, LoweringError> {
        let Some(failure) = &self.ordinary_cleanup_failure else {
            return Ok(None);
        };

        let continuations = self.cleanup_continuations.as_mut()
            .expect("ordinary cleanup retains its continuations");

        let destroy = failed_finalizer.map(|place| CleanupStep::destroy_after_finalizer(place, self.cleanup_occurrence, source));

        let panicked = self.builder.push_block(Self::retained_source(source), MirBlockKind::LifecycleResolution)?;
        let cancelled = self.builder.push_block(Self::retained_source(source), MirBlockKind::LifecycleResolution)?;

        continuations.push(CleanupContinuation {
            failures: [panicked, cancelled],
            prefix: destroy.into_iter().chain(failure.remaining.iter().rev().cloned()).collect(),
        });

        Ok(Some(CleanupFailureTargets {
            panicked, cancelled, report: Self::retained_place(&failure.report),
        }))
    }

    pub(super) fn complete_cleanup_continuation(
        &mut self,
        source: &MirSourceAnchor,
        ordinary: &OrdinaryCleanup<'_>,
        reached: &ScheduledCleanup<'_>,
        following: &[ScheduledCleanup<'_>],
        continuation: CleanupContinuation,
    ) -> Result<(), LoweringError> {
        let report = ordinary.report.as_ref().expect("finalizer continuation retains the failed scope's report");

        self.connect_cleanup_failure(
            source, ordinary, reached, following, continuation.failures, report, &continuation.prefix,
        )
    }

    #[expect(clippy::too_many_arguments, reason = "failure connects one reached cleanup action to the enclosing exit")]
    fn connect_cleanup_failure(
        &mut self,
        source: &MirSourceAnchor,
        ordinary: &OrdinaryCleanup<'_>,
        reached: &ScheduledCleanup<'_>,
        following: &[ScheduledCleanup<'_>],
        [panicked, cancelled]: [MirBlockId; 2],
        report: &MirPlace,
        prefix: &[CleanupStep],
    ) -> Result<(), LoweringError> {
        let scope = reached.scope.expect("a failed cleanup step retains the current scope");
        let point = StorageExitPoint::new(scope, ordinary.exit);

        let [can_panic, can_cancel] = DependencyFailureKind::ALL.map(|kind| reached.can_fail(self, point, kind));

        let catch = self.catch_targets.iter().rev().find(|target| target.scope_depth < reached.depth)
            .map(|target| (target.block, target.scope_depth));

        let (panic, panic_depth) = match catch {
            Some((block, depth)) => (CleanupDestination::Goto(block), depth),
            None if self.input.unit_kind().protected_frame().is_some() => (
                CleanupDestination::Goto(self.terminal_state_block(source, TerminalState::Panicked(report.ty()))?),
                0,
            ),
            None => (CleanupDestination::PropagatePanic, 0),
        };

        let cancellation = if self.input.unit_kind().protected_frame().is_some() {
            CleanupDestination::Goto(self.terminal_state_block(source, TerminalState::Cancelled)?)
        } else {
            CleanupDestination::PropagateCancellation
        };

        let move_report = || MirOperand::Move(Self::retained_place(report));

        for (possible, block, failure, destination, depth) in [
            (can_panic, panicked, CleanupFailure::Panic(move_report()), panic, panic_depth),
            (can_cancel, cancelled, CleanupFailure::Cancellation(Some(move_report())), cancellation, 0),
        ] {
            if !possible {
                self.set_terminator(block, Self::retained_source(source), MirTerminatorKind::Unreachable)?;
                continue;
            }

            let crossing = if matches!(failure, CleanupFailure::Panic(_)) {
                self.abnormal_input_exit(destination)
            } else {
                InputExit::All
            };

            let remaining = prefix.iter().cloned()
                .chain(following.iter().filter(|step| step.crosses(depth, crossing)).map(|step| step.step.clone().into_owned()))
                .collect();

            let newly_exited = if depth < ordinary.broadcast_depth {
                self.input.cleanup_plans(&self.active_scopes[..ordinary.broadcast_depth], depth, ordinary.exit)
            } else {
                Vec::new()
            };

            self.finish_abnormal_cleanup(
                super::CleanupStart::Resume {
                    broadcast_depth: ordinary.broadcast_depth,
                    occurrence: reached.occurrence(),
                    remaining,
                },
                block,
                source,
                failure,
                destination,
                &newly_exited,
                ordinary.pending,
            )?;
        }

        Ok(())
    }

    pub(in crate::lowering) fn check_ordinary_cleanup(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        released_owner: Option<TypeId>,
        shielded: bool,
        mut value: Option<MirOperand>,
        occurrence: Option<bray_bound_tree::BoundExecutionSite>,
    ) -> Result<(MirBlockId, Option<MirOperand>), LoweringError> {
        // The receiver shares immutable collection paths with the surrounding source boundary.
        let outcome = occurrence
            .and_then(|site| self.completion_report_outcome(site))
            .cloned();

        let Some(CleanupFailureTargets { mut panicked, mut cancelled, report }) = self.register_cleanup_continuation(source, None)?
        else {
            let completed = match outcome {
                Some(outcome) => {
                    outcome.take_completed_report(&mut self.builder, block, source, &mut value)?
                }
                None => block,
            };

            return Ok((completed, value));
        };

        if released_owner.is_some() || shielded {
            let kind = self.builder.block_kind(block);

            for target in [&mut panicked, &mut cancelled] {
                let edge = self
                    .builder
                    .push_block(Self::retained_source(source), kind)?;

                if let Some(owner) = released_owner {
                    self.discharge_outgoing_owner(edge, source, owner)?;
                }

                if shielded {
                    self.cleanup_shield(
                        edge,
                        source,
                        bray_runtime_interface::RuntimeAbiRole::CleanupShieldLeave,
                    )?;
                }

                self.set_terminator(
                    edge,
                    Self::retained_source(source),
                    MirTerminatorKind::Goto(MirEdge::new(*target, [])),
                )?;

                *target = edge;
            }
        }

        let completed_report = outcome
            .as_ref()
            .map(|outcome| outcome.incoming_report(&mut self.builder, source))
            .transpose()?;

        let (completed, mut value) = crate::continuation::check_call_outcome(
            &mut self.builder,
            block,
            source,
            panicked,
            cancelled,
            report,
            completed_report,
            value,
        )?;

        let completed = match outcome {
            Some(outcome) => {
                outcome.retain_completed_report(&mut self.builder, completed, source, &mut value)?
            }
            None => completed,
        };

        Ok((completed, value))
    }
}
