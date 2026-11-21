use std::borrow::Borrow;
use std::sync::Arc;

use bray_bound_tree::{
    AnyBoundNodeId, StorageExitDecision, BoundExecutionSite, StorageAccessId,
    StorageCleanupSchedule, StorageExitPoint, StorageScope,
};
use bray_ir::{MirBlockId, MirCleanupPhase, MirPlace, MirSourceAnchor};
use bray_symbols::DependencyFailureKind;

use super::ordinary::OrdinaryCleanup;
use crate::lowering::LoweringError;
use crate::lowering::arrays::CleanupPartCursor;
use crate::lowering::inputs::{InputExit, InputTemporary};
use crate::lowering::lowerer::Lowerer;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::lowering) struct CleanupStep<Temporary = InputTemporary> {
    occurrence: Option<BoundExecutionSite>,
    operation: CleanupOperation<Temporary>,
}

pub(super) struct ScheduledCleanup<'a> {
    pub(super) scope: Option<StorageScope>,
    pub(super) depth: usize,
    pub(super) step: CleanupStep<&'a InputTemporary>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum CleanupOperation<Temporary> {
    ScopedExit {
        expression: bray_bound_tree::BoundExpressionId,
        discharge_only: bool,
    },
    Place {
        place: MirPlace,
        source: MirSourceAnchor,
    },
    Destroy {
        place: MirPlace,
        source: MirSourceAnchor,
    },
    Access {
        access: StorageAccessId,
        place: MirPlace,
        completed: bool,
    },
    Parts {
        cursor: CleanupPartCursor,
        source: MirSourceAnchor,
    },
    Conditional {
        exit: StorageExitPoint,
        schedule: StorageCleanupSchedule,
        selected: std::sync::Arc<[StorageAccessId]>,
    },
    Temporary(Temporary),
}

impl<Temporary> CleanupStep<Temporary> {
    pub(super) fn conditional(
        occurrence: Option<BoundExecutionSite>,
        exit: StorageExitPoint,
        schedule: &StorageCleanupSchedule,
        selected: Arc<[StorageAccessId]>,
    ) -> Option<Self> {
        (!selected.is_empty()).then(|| Self {
            occurrence,
            operation: CleanupOperation::Conditional { exit, schedule: schedule.clone(), selected },
        })
    }
}

impl CleanupStep {
    pub(in crate::lowering) fn parts(
        occurrence: Option<BoundExecutionSite>,
        cursor: CleanupPartCursor,
        source: &MirSourceAnchor,
    ) -> Self {
        Self { occurrence, operation: CleanupOperation::Parts { cursor, source: Lowerer::retained_source(source) } }
    }

    pub(in crate::lowering) fn temporary(
        occurrence: Option<BoundExecutionSite>,
        temporary: InputTemporary,
    ) -> Self {
        Self { occurrence, operation: CleanupOperation::Temporary(temporary) }
    }

    pub(super) fn destroy_after_finalizer(
        place: &MirPlace,
        occurrence: Option<BoundExecutionSite>,
        source: &MirSourceAnchor,
    ) -> Self {
        Self {
            occurrence,
            operation: CleanupOperation::Destroy {
                place: Lowerer::retained_place(place),
                source: Lowerer::retained_source(source),
            },
        }
    }

    pub(super) fn abandonment(place: &MirPlace, occurrence: Option<BoundExecutionSite>, source: &MirSourceAnchor) -> Self {
        Self {
            occurrence,
            operation: CleanupOperation::Place {
                place: Lowerer::retained_place(place),
                source: Lowerer::retained_source(source),
            },
        }
    }
}

impl CleanupStep<&InputTemporary> {
    pub(super) fn into_owned(self) -> CleanupStep {
        let operation = match self.operation {
            CleanupOperation::ScopedExit { expression, discharge_only } => CleanupOperation::ScopedExit { expression, discharge_only },
            CleanupOperation::Place { place, source } => CleanupOperation::Place { place, source },
            CleanupOperation::Destroy { place, source } => CleanupOperation::Destroy { place, source },
            CleanupOperation::Access { access, place, completed } => CleanupOperation::Access { access, place, completed },
            CleanupOperation::Parts { cursor, source } => CleanupOperation::Parts { cursor, source },
            CleanupOperation::Conditional { exit, schedule, selected } => CleanupOperation::Conditional { exit, schedule, selected },
            CleanupOperation::Temporary(temporary) => CleanupOperation::Temporary(temporary.clone()),
        };

        CleanupStep { occurrence: self.occurrence, operation }
    }
}

impl ScheduledCleanup<'_> {
    pub(super) fn occurrence(&self) -> Option<BoundExecutionSite> {
        self.step.occurrence
    }

    pub(super) fn crosses(&self, depth: usize, inputs: InputExit) -> bool {
        match &self.step.operation {
            CleanupOperation::Temporary(temporary) => inputs.crosses(temporary),
            _ => self.depth > depth,
        }
    }

    pub(super) fn can_fail(&self, lowerer: &Lowerer<'_>, point: StorageExitPoint, kind: DependencyFailureKind) -> bool {
        match &self.step.operation {
            CleanupOperation::Access { access, .. } => lowerer.input.storage_flow().cleanup_owner_can_fail(point, *access, kind),
            CleanupOperation::Conditional { selected, .. } => selected.iter().any(|access| {
                lowerer.input.storage_flow().cleanup_owner_can_fail(point, *access, kind)
            }),
            CleanupOperation::ScopedExit { expression, .. } => {
                lowerer.input.cleanup_may_fail(point.scope(), point.exit(), kind)
                    || lowerer.input.cleanup_may_fail(StorageScope::ScopedResultReplacement(*expression), point.exit(), kind)
            }
            CleanupOperation::Temporary(temporary) => temporary.planned_access.map_or_else(
                || lowerer.input.cleanup_may_fail(point.scope(), point.exit(), kind),
                |access| lowerer.input.storage_flow().cleanup_owner_can_fail(point, access, kind),
            ),
            _ => lowerer.input.cleanup_may_fail(point.scope(), point.exit(), kind),
        }
    }

    pub(super) fn after_fault(&self) -> Option<CleanupStep> {
        let CleanupOperation::ScopedExit { expression, .. } = &self.step.operation else { return None; };

        Some(CleanupStep {
            occurrence: self.step.occurrence,
            operation: CleanupOperation::ScopedExit { expression: *expression, discharge_only: true },
        })
    }
}

impl Lowerer<'_> {
    #[expect(clippy::too_many_arguments, reason = "cleanup emission retains checked steps and their ordinary failure continuation")]
    pub(super) fn push_cleanup_operations(
        &mut self,
        mut block: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        plans: &[&StorageExitDecision],
        temporaries: &[InputTemporary],
        pending: Option<&MirPlace>,
        ordinary: Option<&mut OrdinaryCleanup<'_>>,
    ) -> Result<MirBlockId, LoweringError> {
        let previous = self.ordinary_cleanup_failure.take();
        let previous_continuations = self.cleanup_continuations.take();

        let result = (|| {
            if let Some(ordinary) = ordinary {
                let mut steps = Vec::new();

                self.visit_cleanup_steps(phase, plans, temporaries, |_, scope, depth, step| {
                    steps.push(ScheduledCleanup { scope, depth, step });

                    Ok(())
                })?;

                for (index, reached) in steps.iter().enumerate() {
                    let following = &steps[index + 1..];

                    self.ordinary_cleanup_failure = self.prepare_cleanup_failure(source, ordinary, reached)?;
                    self.cleanup_continuations = self.ordinary_cleanup_failure.as_ref().map(|_| Vec::new());

                    let pending_exit = reached.occurrence().map(|occurrence| (occurrence.node(), pending));

                    block = self.push_cleanup_step(block, source, phase, reached.step.clone(), pending_exit)?;

                    self.ordinary_cleanup_failure = None;

                    for continuation in self.cleanup_continuations.take().into_iter().flatten() {
                        self.complete_cleanup_continuation(source, ordinary, reached, following, continuation)?;
                    }
                }
            } else {
                self.visit_cleanup_steps(phase, plans, temporaries, |lowerer, _, _, step| {
                    let pending_exit = step.occurrence.map(|occurrence| (occurrence.node(), pending));

                    block = lowerer.push_cleanup_step(block, source, phase, step, pending_exit)?;

                    Ok(())
                })?;
            }

            Ok(block)
        })();

        self.ordinary_cleanup_failure = previous;
        self.cleanup_continuations = previous_continuations;

        result
    }

    pub(super) fn abnormal_cleanup_steps(
        &mut self,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        plans: &[&StorageExitDecision],
        temporaries: &[InputTemporary],
        abandoned: Option<&MirPlace>,
    ) -> Result<Vec<CleanupStep>, LoweringError> {
        let mut steps = Vec::new();

        if let Some(place) = abandoned {
            let occurrence = plans.last().map(|plan| BoundExecutionSite::Cleanup(plan.exit()))
                .or(self.cleanup_occurrence);

            steps.push(CleanupStep::abandonment(place, occurrence, source));
        }

        self.visit_cleanup_steps(phase, plans, temporaries, |_, _, _, step| {
            steps.push(step.into_owned());

            Ok(())
        })?;

        Ok(steps)
    }

    fn visit_cleanup_steps<'input>(
        &mut self,
        phase: MirCleanupPhase,
        plans: &[&StorageExitDecision],
        pending_inputs: &'input [InputTemporary],
        mut visit: impl FnMut(&mut Self, Option<StorageScope>, usize, CleanupStep<&'input InputTemporary>) -> Result<(), LoweringError>,
    ) -> Result<(), LoweringError> {
        let mut temporaries = pending_inputs.iter().rev().peekable();
        let mut occurrence = self.cleanup_occurrence;

        // The trailing segment owns inputs outside the selected lexical scopes.
        for plan in plans.iter().map(Some).chain(std::iter::once(None)) {
            let scope = plan.map(|plan| plan.scope());

            let depth = if let Some(plan) = plan {
                occurrence = Some(BoundExecutionSite::Cleanup(plan.exit()));

                let depth = self.active_scopes.iter().position(|scope| *scope == plan.scope())
                    .unwrap_or_else(|| panic!("lowering contract violation: MissingBoundNode {value:?}", value = plan.scope()))
                    + 1;

                if phase == MirCleanupPhase::LifecycleResolution
                    && let Some(expression) = plan.scoped_exit()
                {
                    visit(self, scope, depth, CleanupStep {
                        occurrence,
                        operation: CleanupOperation::ScopedExit { expression, discharge_only: false },
                    })?;
                }

                depth
            } else {
                0
            };

            while let Some(temporary) = temporaries.next_if(|temporary| temporary.scope_depth >= depth) {
                if temporary.planned_access.is_none() && temporary.requires_cleanup(phase) {
                    let owned_depth = temporary.scope_depth;

                    let owned_scope = owned_depth.checked_sub(1)
                        .and_then(|index| self.active_scopes.get(index)).copied();

                    visit(self, owned_scope, owned_depth, CleanupStep {
                        occurrence,
                        operation: CleanupOperation::Temporary(temporary),
                    })?;
                }
            }

            let Some(plan) = plan else { break; };

            if phase == MirCleanupPhase::LifecycleResolution
                && let schedule @ StorageCleanupSchedule::Conditional { priority, .. } = plan.lifecycle_schedule()
            {
                let selected = priority.iter().copied().filter(|access| {
                    !self.input.elides_generator_storage(*access)
                        && !self.input_cleanup_owns(*access, phase, pending_inputs)
                }).collect();

                if let Some(step) = CleanupStep::conditional(occurrence, plan.point(), schedule, selected) {
                    visit(self, scope, depth, step)?;
                }

                continue;
            }

            let accesses = match phase {
                MirCleanupPhase::TaskCancellation => plan.cancellation_broadcast(),
                MirCleanupPhase::LifecycleResolution => plan.lifecycle_resolution(),
            };

            for &access in accesses {
                if self.input.elides_generator_storage(access)
                    || self.input_cleanup_owns(access, phase, pending_inputs)
                {
                    continue;
                }

                let operation = if let Some(temporary) = pending_inputs.iter()
                    .find(|temporary| temporary.planned_access == Some(access))
                {
                    CleanupOperation::Temporary(temporary)
                } else {
                    let place = self.place_for_access(access, false)?;
                    let completed = self.input.finalizer_is_complete(plan.exit(), access);

                    CleanupOperation::Access { access, place, completed }
                };

                visit(self, scope, depth, CleanupStep {
                    occurrence,
                    operation,
                })?;
            }
        }

        Ok(())
    }

    pub(super) fn push_cleanup_step(
        &mut self,
        entry: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        step: CleanupStep<impl Borrow<InputTemporary>>,
        pending_exit: Option<(AnyBoundNodeId, Option<&MirPlace>)>,
    ) -> Result<MirBlockId, LoweringError> {
        let previous = std::mem::replace(&mut self.cleanup_occurrence, step.occurrence);

        let result = match step.operation {
            CleanupOperation::ScopedExit { expression, discharge_only: true } => {
                self.discharge_scoped_exit(entry, source, expression, None).map(|(block, _)| block)
            }
            CleanupOperation::ScopedExit { expression, discharge_only: false } => {
                let (exit, pending) = pending_exit.map_or((None, None), |(exit, pending)| (Some(exit), pending));

                self.push_scope_exit(entry, source, expression, exit, pending)
            }
            CleanupOperation::Place { place, source } => self.push_guarded_cleanup(
                entry, &source, phase, place, None, None, false, None,
            ).map(|(block, _)| block),
            CleanupOperation::Destroy { place, source } => {
                assert_eq!(phase, MirCleanupPhase::LifecycleResolution);

                self.destroy_finalized_owner(entry, &source, place, None)
            }
            CleanupOperation::Access { access, place, completed } => self.push_cleanup_place(
                entry, source, phase, access, place, completed, None,
            ).map(|(block, _)| block),
            CleanupOperation::Parts { cursor, source } => self.push_part_cleanup(
                entry, &source, phase, cursor, None, None,
            ).map(|(block, _)| block),
            CleanupOperation::Conditional { exit, schedule, selected } => {
                self.push_conditional_lifecycle(entry, source, exit, &schedule, &selected)
            }
            CleanupOperation::Temporary(temporary) => self.push_input_cleanup_temporary(entry, phase, temporary.borrow()),
        };

        self.cleanup_occurrence = previous;

        result
    }
}
