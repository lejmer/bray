use std::collections::HashMap;
use std::sync::Arc;

use bray_bound_tree::{AnyBoundNodeId, StorageExitDecision, BoundExecutionSite};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge, MirOperand,
    MirSourceAnchor, MirStorageKind, MirSwitchCase, MirTerminatorKind,
};
use bray_symbols::ConstantValueId;

use super::control::{CleanupDestination, TerminalState};
use super::ordinary::OrdinaryCleanup;
use super::steps::CleanupStep;
use crate::cleanup_outcome::{CleanupFailure, CleanupOutcome};
use crate::lowering::LoweringError;
use crate::lowering::inputs::InputExit;
use crate::lowering::lowerer::Lowerer;

pub(in crate::lowering) enum CleanupStart {
    Broadcast,
    Lifecycle,
    Resume {
        broadcast_depth: usize,
        occurrence: Option<BoundExecutionSite>,
        remaining: Vec<CleanupStep>,
    },
}

pub(in crate::lowering) struct AbnormalCleanupMachine {
    outcome: CleanupOutcome,
    selector: bray_ir::MirPlace,
    panic_dispatcher: MirBlockId,
    cancellation_end: MirBlockId,
    cancellation_dispatcher: MirBlockId,
    routes: Vec<AbnormalCleanupRoute>,
    next_route: u64,
    suffixes: HashMap<AbnormalCleanupSuffix, MirBlockId>,
    cancellation_continuations: HashMap<MirBlockId, MirBlockId>,
}

impl AbnormalCleanupMachine {
    pub(in crate::lowering) fn pending_successors(
        &self,
        block: MirBlockId,
    ) -> impl Iterator<Item = MirBlockId> + '_ {
        let panicking = block == self.panic_dispatcher;

        let routes = if panicking || block == self.cancellation_dispatcher {
            self.routes.as_slice()
        } else {
            &[]
        };

        routes
            .iter()
            .filter(move |route| route.panicking == panicking)
            .map(|route| route.target)
    }
}

struct AbnormalCleanupRoute {
    panicking: bool,
    destination: CleanupDestination,
    value: ConstantValueId,
    target: MirBlockId,
}

#[derive(Debug, Eq, Hash, PartialEq)]
struct AbnormalCleanupSuffix {
    phase: MirCleanupPhase,
    step: CleanupStep,
    continuation: MirBlockId,
}

impl Lowerer<'_> {
    pub(super) fn cleanup_state_constants(
        &self,
    ) -> Result<(bray_symbols::TypeId, [bray_symbols::ConstantValueId; 4]), LoweringError> {
        let ty = self.representation_type(RepresentationRole::ScalarU32)?;

        CleanupOutcome::state_constants(self.input.semantic_values(), ty).map_err(Into::into)
    }

    pub(in crate::lowering) fn allocate_cleanup_outcome(
        &mut self,
        source: &MirSourceAnchor,
    ) -> Result<CleanupOutcome, LoweringError> {
        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;
        let report = self.representation_type(RepresentationRole::PanicReport)?;
        let unit = self.representation_type(RepresentationRole::Unit)?;
        let states = self.cleanup_state_constants()?;

        CleanupOutcome::allocate(
            &mut self.builder,
            source,
            boolean,
            report,
            unit,
            self.input.target().runtime_abi(),
            states,
            MirStorageKind::Temporary,
        )
        .map_err(Into::into)
    }

    pub(in crate::lowering) fn create_cleanup_outcome(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<CleanupOutcome, LoweringError> {
        let outcome = self.allocate_cleanup_outcome(source)?;

        outcome.begin(&mut self.builder, block, source)?;

        Ok(outcome)
    }

    pub(in crate::lowering) fn suspension_cleanup_edge(
        &mut self,
        source: &MirSourceAnchor,
        exit: AnyBoundNodeId,
        retention: bray_ir::MirFrameRetention,
    ) -> Result<MirCleanupEdge, LoweringError> {
        let plans = self.cleanup_plans(0, exit);

        let entry = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::CleanupBroadcast,
        )?;

        let outcome = self
            .create_cleanup_outcome(entry, source)?
            .with_retention(Some(Arc::new(retention)));

        let broadcast = self.seed_pending_failure(
            &outcome,
            entry,
            source,
            CleanupFailure::Cancellation(None),
            CleanupDestination::PropagateCancellation,
        )?;

        self.cleanup_outcome = Some(outcome);

        let lifecycle = self.resolve_cleanup(
            broadcast,
            source,
            &plans,
            None,
            None,
            InputExit::All,
        )?;

        let outcome = self
            .cleanup_outcome
            .take()
            .unwrap_or_else(|| {
                panic!(
                    "lowering cleanup contract violated: cancellation exit {exit:?} lost its active cleanup outcome"
                )
            });

        let terminal = self.terminal_state_block(source, TerminalState::Cancelled)?;

        self.finish_cancelled_cleanup(
            lifecycle,
            source,
            outcome,
            CleanupDestination::Goto(terminal),
        )?;

        Ok(MirCleanupEdge::new(
            MirCleanupPhase::TaskCancellation,
            MirEdge::new(entry, []),
        ))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "abnormal exits retain failure identity, cleanup phase, destination, and abandoned storage"
    )]
    pub(in crate::lowering) fn finish_abnormal_cleanup(
        &mut self,
        start: CleanupStart,
        current: MirBlockId,
        source: &MirSourceAnchor,
        failure: CleanupFailure,
        destination: CleanupDestination,
        plans: &[&StorageExitDecision],
        abandoned: Option<&bray_ir::MirPlace>,
    ) -> Result<(), LoweringError> {
        let panicking = matches!(failure, CleanupFailure::Panic(_));

        let input_exit = if panicking {
            self.abnormal_input_exit(destination)
        } else {
            InputExit::All
        };

        let mut machine = match self.abnormal_cleanup_machine.take() {
            Some(machine) => machine,
            None => self.create_abnormal_cleanup_machine(source)?,
        };

        let selector =
            self.register_abnormal_cleanup_route(&mut machine, source, panicking, destination)?;

        self.push_operation(
            current,
            Self::retained_source(source),
            bray_ir::MirOperationKind::Store {
                kind: bray_ir::MirStoreKind::Initialize,
                destination: machine.selector.clone(),
                value: selector,
            },
            None,
        )?;

        machine.outcome.begin(&mut self.builder, current, source)?;

        let current =
            self.seed_pending_failure(&machine.outcome, current, source, failure, destination)?;

        let mut temporaries = self.input_cleanup(input_exit);

        let lifecycle_only = matches!(&start, CleanupStart::Lifecycle);

        let lifecycle_steps = match start {
            CleanupStart::Resume { broadcast_depth, occurrence, remaining } => {
                temporaries.retain(|temporary| temporary.scope_depth <= broadcast_depth);

                let mut steps = Vec::new();

                if let Some(place) = abandoned {
                    steps.push(CleanupStep::abandonment(place, occurrence, source));
                }

                steps.extend(remaining);

                steps.extend(self.abnormal_cleanup_steps(
                    source, MirCleanupPhase::LifecycleResolution, plans, &temporaries, None,
                )?);

                steps
            }
            CleanupStart::Broadcast | CleanupStart::Lifecycle => {
                self.abnormal_cleanup_steps(
                    source, MirCleanupPhase::LifecycleResolution, plans, &temporaries, abandoned,
                )?
            }
        };

        let lifecycle_continuation = if panicking {
            machine.panic_dispatcher
        } else {
            machine.cancellation_end
        };

        let lifecycle = self.shared_abnormal_cleanup_suffix(
            &mut machine,
            source,
            MirCleanupPhase::LifecycleResolution,
            lifecycle_steps,
            lifecycle_continuation,
        )?;

        if lifecycle_only {
            self.set_terminator(
                current,
                Self::retained_source(source),
                MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                    MirCleanupPhase::LifecycleResolution,
                    MirEdge::new(lifecycle, []),
                )),
            )?;

            self.abnormal_cleanup_machine = Some(machine);

            return Ok(());
        }

        let cancellation_continuation =
            self.abnormal_cancellation_continuation(&mut machine, source, lifecycle)?;

        let cancellation_steps = self.abnormal_cleanup_steps(
            source,
            MirCleanupPhase::TaskCancellation,
            plans,
            &temporaries,
            abandoned,
        )?;

        let broadcast = self.shared_abnormal_cleanup_suffix(
            &mut machine,
            source,
            MirCleanupPhase::TaskCancellation,
            cancellation_steps,
            cancellation_continuation,
        )?;

        let cleanup = MirCleanupEdge::new(
            MirCleanupPhase::TaskCancellation,
            MirEdge::new(broadcast, []),
        );

        let terminator = if panicking {
            MirTerminatorKind::Panic {
                report: machine.outcome.report(),
                cleanup,
            }
        } else {
            MirTerminatorKind::CancelCurrentRun { cleanup }
        };

        self.set_terminator(current, Self::retained_source(source), terminator)?;
        self.abnormal_cleanup_machine = Some(machine);

        Ok(())
    }

    fn create_abnormal_cleanup_machine(
        &mut self,
        source: &MirSourceAnchor,
    ) -> Result<AbnormalCleanupMachine, LoweringError> {
        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;
        let report = self.representation_type(RepresentationRole::PanicReport)?;
        let unit = self.representation_type(RepresentationRole::Unit)?;
        let states = self.cleanup_state_constants()?;
        let selector_type = self.representation_type(RepresentationRole::ScalarU32)?;

        let outcome = CleanupOutcome::allocate(
            &mut self.builder,
            source,
            boolean,
            report,
            unit,
            self.input.target().runtime_abi(),
            states,
            bray_ir::MirStorageKind::Temporary,
        )?;

        let selector = self.builder.push_storage(
            Self::retained_source(source),
            bray_ir::MirStorageKind::Temporary,
            selector_type,
        )?;

        let panic_dispatcher = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        outcome.end_shield(&mut self.builder, panic_dispatcher, source)?;

        let cancellation_end = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let panicked = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let cancellation_dispatcher = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let completed = outcome.dispatch(
            &mut self.builder,
            cancellation_end,
            source,
            panicked,
            cancellation_dispatcher,
            &mut None,
        )?;

        self.set_terminator(
            completed,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(cancellation_dispatcher, [])),
        )?;

        let panic_destination = if self.input.unit_kind().protected_frame().is_some() {
            CleanupDestination::Goto(
                self.terminal_state_block(source, TerminalState::Panicked(report))?,
            )
        } else {
            CleanupDestination::PropagatePanic
        };

        self.set_destination(panicked, source, panic_destination, Some(outcome.report()))?;

        Ok(AbnormalCleanupMachine {
            outcome,
            selector: bray_ir::MirPlace::new(selector, [], selector_type),
            panic_dispatcher,
            cancellation_end,
            cancellation_dispatcher,
            routes: Vec::new(),
            next_route: 0,
            suffixes: HashMap::new(),
            cancellation_continuations: HashMap::new(),
        })
    }

    fn register_abnormal_cleanup_route(
        &mut self,
        machine: &mut AbnormalCleanupMachine,
        source: &MirSourceAnchor,
        panicking: bool,
        destination: CleanupDestination,
    ) -> Result<MirOperand, LoweringError> {
        if let Some(route) = machine
            .routes
            .iter()
            .find(|route| route.panicking == panicking && route.destination == destination)
        {
            return Ok(MirOperand::Constant {
                value: route.value,
                ty: machine.selector.ty(),
            });
        }

        machine.next_route = machine
            .next_route
            .checked_add(1)
            .expect("lowering cleanup contract violated: abnormal cleanup route overflow");

        let selector = crate::operand::integer_constant(
            self.input.semantic_values(),
            machine.selector.ty(),
            machine.next_route,
        )?;

        let MirOperand::Constant { value, .. } = selector else {
            unreachable!("integer cleanup selector must be a constant operand")
        };

        let target = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        self.set_destination(target, source, destination, Some(machine.outcome.report()))?;

        machine.routes.push(AbnormalCleanupRoute {
            panicking,
            destination,
            value,
            target,
        });

        Ok(MirOperand::Constant {
            value,
            ty: machine.selector.ty(),
        })
    }

    fn shared_abnormal_cleanup_suffix(
        &mut self,
        machine: &mut AbnormalCleanupMachine,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        steps: Vec<CleanupStep>,
        mut continuation: MirBlockId,
    ) -> Result<MirBlockId, LoweringError> {
        for step in steps.into_iter().rev() {
            let suffix = AbnormalCleanupSuffix {
                phase,
                step: step.clone(),
                continuation,
            };

            if let Some(entry) = machine.suffixes.get(&suffix).copied() {
                continuation = entry;
                continue;
            }

            let entry = self.builder.push_block(
                Self::retained_source(source),
                match phase {
                    MirCleanupPhase::TaskCancellation => MirBlockKind::CleanupBroadcast,
                    MirCleanupPhase::LifecycleResolution => MirBlockKind::LifecycleResolution,
                },
            )?;

            self.cleanup_outcome = Some(machine.outcome.clone());

            let end = self.push_cleanup_step(entry, source, phase, step, None)?;

            self.cleanup_outcome = None;

            self.set_terminator(
                end,
                Self::retained_source(source),
                MirTerminatorKind::Goto(MirEdge::new(continuation, [])),
            )?;

            machine.suffixes.insert(suffix, entry);
            continuation = entry;
        }

        Ok(continuation)
    }

    fn abnormal_cancellation_continuation(
        &mut self,
        machine: &mut AbnormalCleanupMachine,
        source: &MirSourceAnchor,
        lifecycle: MirBlockId,
    ) -> Result<MirBlockId, LoweringError> {
        if let Some(block) = machine.cancellation_continuations.get(&lifecycle).copied() {
            return Ok(block);
        }

        let block = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::CleanupBroadcast,
        )?;

        self.set_terminator(
            block,
            Self::retained_source(source),
            MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                MirCleanupPhase::LifecycleResolution,
                MirEdge::new(lifecycle, []),
            )),
        )?;

        machine.cancellation_continuations.insert(lifecycle, block);

        Ok(block)
    }

    pub(in crate::lowering) fn finish_abnormal_cleanup_dispatchers(
        &mut self,
        source: &MirSourceAnchor,
    ) -> Result<(), LoweringError> {
        let Some(machine) = self.abnormal_cleanup_machine.take() else {
            return Ok(());
        };

        self.set_abnormal_cleanup_dispatcher(
            machine.panic_dispatcher,
            source,
            &machine.selector,
            machine.routes.iter().filter(|route| route.panicking),
        )?;

        self.set_abnormal_cleanup_dispatcher(
            machine.cancellation_dispatcher,
            source,
            &machine.selector,
            machine.routes.iter().filter(|route| !route.panicking),
        )
    }

    fn set_abnormal_cleanup_dispatcher<'route>(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        selector: &bray_ir::MirPlace,
        routes: impl Iterator<Item = &'route AbnormalCleanupRoute>,
    ) -> Result<(), LoweringError> {
        let routes = routes.collect::<Vec<_>>();

        if routes.is_empty() {
            return self
                .set_terminator(
                    block,
                    Self::retained_source(source),
                    MirTerminatorKind::Unreachable,
                )
                .map_err(Into::into);
        }

        let invalid = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        self.set_terminator(
            invalid,
            Self::retained_source(source),
            MirTerminatorKind::Unreachable,
        )?;

        self.set_terminator(
            block,
            Self::retained_source(source),
            MirTerminatorKind::Switch {
                discriminant: MirOperand::Copy(selector.clone()),
                cases: Arc::from(
                    routes
                        .into_iter()
                        .map(|route| {
                            MirSwitchCase::new(route.value, MirEdge::new(route.target, []))
                        })
                        .collect::<Vec<_>>(),
                ),
                otherwise: MirEdge::new(invalid, []),
            },
        )?;

        Ok(())
    }

    pub(super) fn abnormal_input_exit(&self, destination: CleanupDestination) -> InputExit {
        if let CleanupDestination::Goto(block) = destination
            && let Some(index) = self
                .catch_targets
                .iter()
                .position(|target| target.block == block)
        {
            return InputExit::Catch(index + 1);
        }

        InputExit::All
    }

    pub(super) fn resolve_cleanup(
        &mut self,
        broadcast: MirBlockId,
        source: &MirSourceAnchor,
        plans: &[&StorageExitDecision],
        pending: Option<&bray_ir::MirPlace>,
        ordinary: Option<&mut OrdinaryCleanup<'_>>,
        input_exit: InputExit,
    ) -> Result<MirBlockId, LoweringError> {
        let temporaries = self.input_cleanup(input_exit);

        let lifecycle = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let broadcast = self.push_cleanup_operations(
            broadcast,
            source,
            MirCleanupPhase::TaskCancellation,
            plans,
            &temporaries,
            pending,
            None,
        )?;

        self.set_terminator(
            broadcast,
            Self::retained_source(source),
            MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                MirCleanupPhase::LifecycleResolution,
                MirEdge::new(lifecycle, []),
            )),
        )?;

        self.push_cleanup_operations(
            lifecycle,
            source,
            MirCleanupPhase::LifecycleResolution,
            plans,
            &temporaries,
            pending,
            ordinary,
        )
    }

    fn finish_cancelled_cleanup(
        &mut self,
        lifecycle: MirBlockId,
        source: &MirSourceAnchor,
        outcome: CleanupOutcome,
        destination: CleanupDestination,
    ) -> Result<(), LoweringError> {
        let panicked = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let cancelled = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let completed = outcome.dispatch(
            &mut self.builder,
            lifecycle,
            source,
            panicked,
            cancelled,
            &mut None,
        )?;

        self.set_terminator(
            completed,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(cancelled, [])),
        )?;

        self.set_destination(cancelled, source, destination, Some(outcome.report()))?;

        let panic_destination = if self.input.unit_kind().protected_frame().is_some() {
            let report = self.representation_type(RepresentationRole::PanicReport)?;

            CleanupDestination::Goto(
                self.terminal_state_block(source, TerminalState::Panicked(report))?,
            )
        } else {
            CleanupDestination::PropagatePanic
        };

        self.set_destination(panicked, source, panic_destination, Some(outcome.report()))
    }
}
