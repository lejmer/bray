use std::collections::HashMap;
use std::sync::Arc;

use bray_bound_tree::{AnyBoundNodeId, AsyncScopeExitPlan, StorageAccessId};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge, MirOperand,
    MirSourceAnchor, MirSwitchCase, MirTerminatorKind,
};
use bray_symbols::ConstantValueId;

use super::control::{CleanupDestination, TerminalState};
use crate::cleanup_outcome::CleanupOutcome;
use crate::lowering::LoweringError;
use crate::lowering::inputs::{InputExit, InputTemporary};
use crate::lowering::lowerer::Lowerer;

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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum AbnormalCleanupStep {
    Place {
        place: bray_ir::MirPlace,
        source: MirSourceAnchor,
    },
    Access {
        access: StorageAccessId,
        place: bray_ir::MirPlace,
        completed: bool,
    },
    Temporary(InputTemporary),
}

#[derive(Debug, Eq, Hash, PartialEq)]
struct AbnormalCleanupSuffix {
    phase: MirCleanupPhase,
    step: AbnormalCleanupStep,
    continuation: MirBlockId,
}

impl Lowerer<'_> {
    pub(in crate::lowering) fn create_cleanup_outcome(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<CleanupOutcome, LoweringError> {
        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;
        let report = self.representation_type(RepresentationRole::PanicReport)?;
        let unit = self.representation_type(RepresentationRole::Unit)?;

        CleanupOutcome::new(
            &mut self.builder,
            block,
            source,
            boolean,
            report,
            unit,
            self.input.target().runtime_abi(),
        )
        .map_err(Into::into)
    }

    pub(in crate::lowering) fn suspension_cleanup_edge(
        &mut self,
        source: &MirSourceAnchor,
        exit: AnyBoundNodeId,
    ) -> Result<MirCleanupEdge, LoweringError> {
        let plans = self.cleanup_plans(0, exit);

        let broadcast = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::CleanupBroadcast,
        )?;

        let outcome = self.create_cleanup_outcome(broadcast, source)?;

        outcome.initialize_cancellation(&mut self.builder, broadcast, source)?;
        self.cleanup_outcome = Some(outcome);

        let lifecycle = self.resolve_cleanup(
            broadcast,
            source,
            &plans,
            None,
            &std::collections::BTreeMap::new(),
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
            MirEdge::new(broadcast, []),
        ))
    }

    pub(in crate::lowering) fn finish_abnormal_cleanup(
        &mut self,
        current: MirBlockId,
        source: &MirSourceAnchor,
        report: Option<MirOperand>,
        destination: CleanupDestination,
        plans: &[AsyncScopeExitPlan],
        abandoned: Option<&bray_ir::MirPlace>,
    ) -> Result<(), LoweringError> {
        let panicking = report.is_some();

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

        if let Some(report) = report {
            machine
                .outcome
                .initialize_panic(&mut self.builder, current, source, report)?;
        } else {
            machine
                .outcome
                .initialize_cancellation(&mut self.builder, current, source)?;
        }

        let temporaries = self.input_cleanup(input_exit);

        let lifecycle_steps = self.abnormal_cleanup_steps(
            source,
            MirCleanupPhase::LifecycleResolution,
            plans,
            &temporaries,
            abandoned,
        )?;

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
        let selector_type = self.representation_type(RepresentationRole::ScalarU32)?;

        let outcome = CleanupOutcome::allocate(
            &mut self.builder,
            source,
            boolean,
            report,
            unit,
            self.input.target().runtime_abi(),
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

        self.set_destination(
            target,
            source,
            destination,
            panicking.then(|| machine.outcome.report()),
        )?;

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

    fn abnormal_cleanup_steps(
        &mut self,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        plans: &[AsyncScopeExitPlan],
        temporaries: &[InputTemporary],
        abandoned: Option<&bray_ir::MirPlace>,
    ) -> Result<Vec<AbnormalCleanupStep>, LoweringError> {
        let mut steps = Vec::new();

        if let Some(place) = abandoned {
            steps.push(AbnormalCleanupStep::Place {
                place: place.clone(),
                source: Self::retained_source(source),
            });
        }

        let mut temporaries = temporaries.iter().rev().peekable();

        for plan in plans {
            let depth = self
                .active_scopes
                .iter()
                .position(|scope| *scope == plan.scope())
                .unwrap_or_else(|| {
                    panic!(
                        "lowering contract violation: MissingBoundNode {value:?}",
                        value = plan.scope()
                    )
                })
                + 1;

            Self::push_abnormal_temporary_steps(&mut steps, &mut temporaries, phase, depth);

            for access in match phase {
                MirCleanupPhase::TaskCancellation => plan.cancellation_broadcast(),
                MirCleanupPhase::LifecycleResolution => plan.lifecycle_resolution(),
            } {
                steps.push(AbnormalCleanupStep::Access {
                    access: *access,
                    place: self.place_for_access(*access, false)?,
                    completed: self.input.finalizer_is_complete(plan.exit(), *access),
                });
            }
        }

        Self::push_abnormal_temporary_steps(&mut steps, &mut temporaries, phase, 0);

        Ok(steps)
    }

    fn push_abnormal_temporary_steps(
        steps: &mut Vec<AbnormalCleanupStep>,
        temporaries: &mut std::iter::Peekable<std::iter::Rev<std::slice::Iter<'_, InputTemporary>>>,
        phase: MirCleanupPhase,
        scope_depth: usize,
    ) {
        loop {
            let Some(temporary) =
                temporaries.next_if(|temporary| temporary.scope_depth >= scope_depth)
            else {
                break;
            };

            if temporary.requires_cleanup(phase) {
                steps.push(AbnormalCleanupStep::Temporary(temporary.clone()));
            }
        }
    }

    fn shared_abnormal_cleanup_suffix(
        &mut self,
        machine: &mut AbnormalCleanupMachine,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        steps: Vec<AbnormalCleanupStep>,
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

            let end = match &step {
                AbnormalCleanupStep::Place {
                    place,
                    source: cleanup_source,
                } => {
                    self.push_guarded_cleanup(
                        entry,
                        cleanup_source,
                        phase,
                        place.clone(),
                        None,
                        None,
                        false,
                        None,
                    )?
                    .0
                }
                AbnormalCleanupStep::Access {
                    access,
                    place,
                    completed,
                } => {
                    self.push_cleanup_place(
                        entry,
                        source,
                        phase,
                        *access,
                        place.clone(),
                        *completed,
                        None,
                    )?
                    .0
                }
                AbnormalCleanupStep::Temporary(temporary) => {
                    self.push_input_cleanup_temporary(entry, phase, temporary)?
                }
            };

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
        mut broadcast: MirBlockId,
        source: &MirSourceAnchor,
        plans: &[AsyncScopeExitPlan],
        abandoned: Option<&bray_ir::MirPlace>,
        failures: &std::collections::BTreeMap<
            bray_bound_tree::BoundBlockId,
            (MirBlockId, MirBlockId, bray_ir::MirPlace),
        >,
        input_exit: InputExit,
    ) -> Result<MirBlockId, LoweringError> {
        let temporaries = self.input_cleanup(input_exit);

        let mut lifecycle = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        if let Some(place) = abandoned {
            broadcast = self
                .push_guarded_cleanup(
                    broadcast,
                    source,
                    MirCleanupPhase::TaskCancellation,
                    Self::retained_place(place),
                    None,
                    None,
                    false,
                    None,
                )?
                .0;
        }

        let (broadcast, _) = self.push_cleanup_operations(
            broadcast,
            source,
            MirCleanupPhase::TaskCancellation,
            plans,
            &temporaries,
            None,
            failures,
        )?;

        self.set_terminator(
            broadcast,
            Self::retained_source(source),
            MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                MirCleanupPhase::LifecycleResolution,
                MirEdge::new(lifecycle, []),
            )),
        )?;

        if let Some(place) = abandoned {
            lifecycle = self
                .push_guarded_cleanup(
                    lifecycle,
                    source,
                    MirCleanupPhase::LifecycleResolution,
                    Self::retained_place(place),
                    None,
                    None,
                    false,
                    None,
                )?
                .0;
        }

        self.push_cleanup_operations(
            lifecycle,
            source,
            MirCleanupPhase::LifecycleResolution,
            plans,
            &temporaries,
            None,
            failures,
        )
        .map(|(block, _)| block)
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

        let completed =
            outcome.dispatch(&mut self.builder, lifecycle, source, panicked, cancelled)?;

        self.set_terminator(
            completed,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(cancelled, [])),
        )?;

        self.set_destination(cancelled, source, destination, None)?;

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
