use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use bray_bound_tree::{AnalysisOperationId, AnyBoundNodeId, BoundExecutionSite, BoundExpressionId, StorageCleanupRemainder, StorageExitPoint};
use bray_symbols::DependencyFailureKind;

use super::id::AnalysisEdgeId;
use super::model::{AnalysisEdge, AnalysisEdgeKind, AnalysisOperationKind, AnalysisRefinement, AnalysisResultPhase, ControlFlowGraph};
use bray_bound_tree::StorageDeferredProducer;

/// Distinguishes a reached fault from a continuation admitted to perform its suffix.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::analysis) enum CleanupProgress {
    Fault(DependencyFailureKind, StorageCleanupRemainder),
    Resolving {
        kind: DependencyFailureKind,
        resume: Option<Arc<CleanupResume>>,
    },
    Replacement {
        kind: DependencyFailureKind,
        expression: BoundExpressionId,
        remaining: Option<StorageCleanupRemainder>,
    },
    Repeating {
        remaining: StorageCleanupRemainder,
        entered: Option<Arc<Self>>,
    },
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::analysis) struct CleanupResume {
    remainder: StorageCleanupRemainder,
    outer: Option<Arc<Self>>,
}

impl CleanupProgress {
    pub(in crate::analysis) fn failure(&self) -> DependencyFailureKind {
        match self {
            Self::Fault(kind, _) | Self::Resolving { kind, .. } | Self::Replacement { kind, .. } => *kind,
            Self::Repeating { .. } => unreachable!("array repetition resolves within the current cleanup operation"),
        }
    }

    pub(in crate::analysis) fn resume(progress: &mut Option<Self>, point: Option<StorageExitPoint>, operation: AnalysisOperationId) -> Option<StorageCleanupRemainder> {
        if matches!(progress, Some(Self::Repeating { remaining, .. }) if remaining.point == point && remaining.after.operation == operation) {
            let Some(Self::Repeating { remaining, entered }) = progress.take() else { unreachable!() };

            *progress = entered.map(Arc::unwrap_or_clone);

            return Some(remaining);
        }

        progress.as_mut()?.take_remainder(point, operation)
    }

    fn take_remainder(&mut self, point: Option<StorageExitPoint>, operation: AnalysisOperationId) -> Option<StorageCleanupRemainder> {
        if let Self::Replacement { remaining, .. } = self {
            return remaining.as_ref().is_some_and(|remainder| remainder.after.operation == operation)
                .then(|| remaining.take()).flatten();
        }

        let Self::Resolving { resume, .. } = self else { return None; };

        let head = resume.as_ref().filter(|head| point.is_some() && head.remainder.point == point)?;
        let result = head.remainder.clone();

        *resume = head.outer.clone();

        Some(result)
    }

    fn remainder(&self) -> Option<&StorageCleanupRemainder> {
        match self {
            Self::Resolving { resume: Some(head), .. } => Some(&head.remainder),
            Self::Replacement { remaining, .. } => remaining.as_ref(),
            Self::Repeating { remaining, .. } => Some(remaining),
            _ => None,
        }
    }

    pub(in crate::analysis) fn repeat(active: Option<&Self>, remaining: StorageCleanupRemainder) -> Self {
        Self::Repeating { remaining, entered: active.cloned().map(Arc::new) }
    }

    pub(in crate::analysis) fn during(
        active: Option<&Self>,
        failure: DependencyFailureKind,
        remainder: StorageCleanupRemainder,
    ) -> Self {
        if remainder.point.is_none() {
            let expression = remainder.after.occurrence.expression().expect("replacement cleanup retains its assignment");

            let kind = match active {
                None => failure,
                Some(Self::Replacement { kind, expression: current, .. }) if *current == expression => primary_failure(*kind, failure),
                _ => unreachable!("replacement resumes its own cleanup before leaving the assignment"),
            };

            return Self::Replacement { kind, expression, remaining: Some(remainder) };
        }

        match active {
            Some(Self::Resolving { kind, resume }) => Self::Resolving {
                kind: primary_failure(*kind, failure),
                resume: Some(Arc::new(CleanupResume { remainder, outer: resume.clone() })),
            },
            None => Self::Fault(failure, remainder),
            Some(Self::Fault(..)) => unreachable!("a fault must enter cleanup before running its continuation"),
            Some(Self::Replacement { .. }) => unreachable!("replacement installs the new value before lexical cleanup"),
            Some(Self::Repeating { .. }) => unreachable!("element cleanup consumes the repetition cursor before running its actions"),
        }
    }
}

fn primary_failure(previous: DependencyFailureKind, failure: DependencyFailureKind) -> DependencyFailureKind {
    match (previous, failure) {
        (DependencyFailureKind::Panic, _) | (_, DependencyFailureKind::Panic) => DependencyFailureKind::Panic,
        _ => DependencyFailureKind::Cancellation,
    }
}

pub(in crate::analysis) fn cleanup_successor(
    progress: Option<&CleanupProgress>,
    edge: &AnalysisEdge,
) -> Option<Option<CleanupProgress>> {
    match (edge.refinement(), progress) {
        (
            Some(AnalysisRefinement::CleanupOutcome { scope, exit, failure: Some(expected) }),
            Some(CleanupProgress::Fault(actual, remainder)),
        ) if expected == *actual && remainder.point == Some(StorageExitPoint::new(scope, exit)) => {
            Some(Some(CleanupProgress::Resolving {
                kind: expected,
                resume: Some(Arc::new(CleanupResume { remainder: remainder.clone(), outer: None })),
            }))
        }
        (Some(AnalysisRefinement::CleanupOutcome { failure: Some(_), .. }), _)
        | (_, Some(CleanupProgress::Fault(..))) => None,
        (
            Some(AnalysisRefinement::InvocationFailure(BoundExecutionSite::Cleanup(AnyBoundNodeId::Expression(site)), expected)),
            Some(CleanupProgress::Replacement { kind, expression, remaining: None }),
        ) if expected == *kind && site == *expression => Some(None),
        (Some(AnalysisRefinement::InvocationFailure(BoundExecutionSite::Cleanup(_), _)), _)
        | (_, Some(CleanupProgress::Replacement { .. })) => None,
        (_, Some(CleanupProgress::Repeating { .. })) => unreachable!("array repetition stays within its cleanup operation"),
        (Some(AnalysisRefinement::BeginCleanup(kind)), None) => {
            Some(Some(CleanupProgress::Resolving { kind, resume: None }))
        }
        (
            Some(AnalysisRefinement::InvocationFailure(BoundExecutionSite::ScopedExit(_), kind)),
            Some(CleanupProgress::Resolving { kind: active, resume }),
        ) => {
            Some(Some(CleanupProgress::Resolving { kind: primary_failure(*active, kind), resume: resume.clone() }))
        }
        (
            Some(AnalysisRefinement::InvocationEntry { invocation: BoundExecutionSite::ScopedExit(_), admitted: false }),
            Some(CleanupProgress::Resolving { resume, .. }),
        ) => {
            Some(Some(CleanupProgress::Resolving { kind: DependencyFailureKind::Panic, resume: resume.clone() }))
        }
        (Some(AnalysisRefinement::CaughtResult { .. }), _) => Some(None),
        _ => Some(progress.cloned()),
    }
}

pub(super) trait OutcomeState: Clone {
    fn cleanup_progress(&self) -> Option<&CleanupProgress>;
}

/// Joins reached states at the same cursor of one incoming outcome path.
/// Earlier positions can contribute to later suffixes before those suffixes run.
pub(super) fn continue_cleanup<S: OutcomeState + Eq>(
    operation: AnalysisOperationId,
    mut incoming: Vec<S>,
    mut apply: impl FnMut(&mut S, &mut Vec<S>) -> bool,
    mut merge: impl FnMut(&mut S, &S) -> bool,
    mut reached: impl FnMut(S),
) {
    use std::collections::btree_map::Entry;

    let mut waiting = BTreeMap::new();
    let mut iterations = BTreeMap::new();

    loop {
        for state in incoming.drain(..) {
            let cursor = state.cleanup_progress().and_then(|progress| {
                progress.remainder().map(|remainder| (progress, remainder))
            });

            let Some((progress, remainder)) = cursor
                .filter(|(_, remainder)| remainder.after.operation == operation)
            else {
                reached(state);
                continue;
            };

            let position = (std::cmp::Reverse(remainder.accesses.len()), remainder.after.part, remainder.after.kind);

            match waiting.entry((position, progress.clone())) {
                Entry::Vacant(entry) => { entry.insert(state); }
                Entry::Occupied(mut entry) if entry.get() != &state => { merge(entry.get_mut(), &state); }
                Entry::Occupied(_) => {}
            }
        }

        let Some((_, mut state)) = waiting.pop_first() else { break; };

        if reach_cleanup_iteration(&mut iterations, &mut state, &mut merge)
            && apply(&mut state, &mut incoming)
        {
            reached(state);
        }
    }
}

fn reach_cleanup_iteration<S: OutcomeState>(
    iterations: &mut BTreeMap<CleanupProgress, S>,
    state: &mut S,
    merge: impl FnOnce(&mut S, &S) -> bool,
) -> bool {
    use std::collections::btree_map::Entry;

    let Some(progress @ CleanupProgress::Repeating { .. }) = state.cleanup_progress() else { return true; };

    match iterations.entry(progress.clone()) {
        Entry::Vacant(entry) => { entry.insert(state.clone()); }
        Entry::Occupied(mut entry) => {
            if !merge(entry.get_mut(), state) { return false; }

            state.clone_from(entry.get());
        }
    }

    true
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum OutcomeStep {
    Production(AnalysisEdgeId),
    NormalCall(BoundExecutionSite, usize),
    InvocationFailure(AnalysisEdgeId, usize),
    Frame(BoundExecutionSite, Option<StorageDeferredProducer>),
    TaskCompletion(BoundExecutionSite, bray_bound_tree::StorageTaskOutcome),
}

pub(super) type OutcomePath = Arc<[OutcomeStep]>;

pub(super) fn extend_path(path: Option<&OutcomePath>, step: OutcomeStep) -> OutcomePath {
    let mut steps = path.map(|path| path.as_ref()).unwrap_or_default().to_vec();

    steps.push(step);

    steps.into()
}

pub(super) fn edge_path(
    graph: &ControlFlowGraph,
    edge: &AnalysisEdge,
    path: Option<&OutcomePath>,
    produces_result: bool,
    failure_row: Option<usize>,
) -> Option<OutcomePath> {
    // A loop joins its stable header state instead of recording iteration history.
    if graph.is_loop_header(edge.target()) {
        return None;
    }

    if produces_result || matches!(edge.refinement(), Some(AnalysisRefinement::InvocationVariant(..))) {
        return Some(extend_path(path, OutcomeStep::Production(edge.id())));
    }

    if let Some(AnalysisRefinement::InvocationFailure(invocation, _)) = edge.refinement() {
        let admitted = path.filter(|steps| matches!(steps.last(), Some(OutcomeStep::NormalCall(site, _)) if *site == invocation))
            .map(|steps| Arc::from(&steps[..steps.len() - 1]));

        let path = admitted.as_ref().or(path);

        return failure_row.map_or_else(|| path.cloned(), |row| Some(extend_path(path, OutcomeStep::InvocationFailure(edge.id(), row))));
    }

    path.cloned()
}

pub(super) fn result_production_edges(graph: &ControlFlowGraph, storage: &bray_bound_tree::StoragePlan) -> std::collections::BTreeSet<AnalysisEdgeId> {
    graph.blocks().iter().filter(|block| {
        block.predecessors().len() > 1
            && block.operations().iter().filter_map(|id| graph.operation(*id)).any(|operation| {
                matches!(operation.kind(), AnalysisOperationKind::Result { expression, phase: AnalysisResultPhase::Completion }
                    if storage.value_slot(expression).is_some())
            })
    }).flat_map(|block| block.predecessors().iter().copied()).filter(|id| {
        graph.edge(*id).is_some_and(|edge| matches!(edge.kind(), AnalysisEdgeKind::Yield | AnalysisEdgeKind::LoopBreak))
    }).collect()
}

pub(super) fn apply_call_rows<State: Clone>(
    state: State,
    path: Option<OutcomePath>,
    invocation: BoundExecutionSite,
    rows: &[usize],
    budget: &mut usize,
    mut apply: impl FnMut(State, Option<OutcomePath>, Option<usize>, &mut usize),
) {
    match rows {
        [row] => apply(state, path, Some(*row), budget),
        [earlier @ .., last] if !earlier.is_empty() && earlier.len() <= *budget => {
            *budget -= earlier.len();

            let selected_path = |row| Some(extend_path(path.as_ref(), OutcomeStep::NormalCall(invocation, row)));

            for row in earlier {
                // Mutable checking state belongs to each selected completion.
                apply(state.clone(), selected_path(*row), Some(*row), budget);
            }

            apply(state, selected_path(*last), Some(*last), budget);
        }
        _ => apply(state, path, None, budget),
    }
}

/// Reached states distinguished by the stable operations that produced actual values.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct FlowOutcomes<State> {
    // An empty path is ordinary flow. None is the conservative union.
    entries: OutcomeLane<State>,
    // Widening can combine facts at one saved position. It cannot combine positions.
    cleanup: BTreeMap<CleanupProgress, OutcomeLane<State>>,
}

type OutcomeLane<State> = Vec<(Option<OutcomePath>, State)>;

impl<State> Default for FlowOutcomes<State> {
    fn default() -> Self { Self { entries: Vec::new(), cleanup: BTreeMap::new() } }
}

impl<State: OutcomeState> FlowOutcomes<State> {
    pub(super) const MAX_ALTERNATIVES: usize = 8;

    pub(super) fn entry(state: State) -> Self {
        Self { entries: vec![(Some(Arc::default()), state)], cleanup: BTreeMap::new() }
    }

    pub(super) fn states(&self) -> impl Iterator<Item = &State> {
        self.iter().map(|(_, state)| state)
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (Option<&OutcomePath>, &State)> {
        std::iter::once(&self.entries).chain(self.cleanup.values()).flatten()
            .map(|(path, state)| (path.as_ref(), state))
    }

    pub(super) fn into_states(mut self) -> impl Iterator<Item = (Option<OutcomePath>, State)> {
        self.take().flat_map(|(_, states)| states)
    }

    pub(super) fn joined(&self, mut join: impl FnMut(&mut State, &State)) -> Option<Cow<'_, State>> {
        let mut states = self.states();
        let first = states.next()?;

        let Some(second) = states.next() else { return Some(Cow::Borrowed(first)); };

        // The joined view cannot mutate the selected states retained by the solver.
        let mut joined = first.clone();

        join(&mut joined, second);

        for state in states { join(&mut joined, state); }

        Some(Cow::Owned(joined))
    }

    pub(super) fn take(&mut self) -> impl Iterator<Item = (usize, OutcomeLane<State>)> + use<State> {
        let ordinary = std::mem::take(&mut self.entries);
        let cleanup = std::mem::take(&mut self.cleanup);

        std::iter::once(ordinary).chain(cleanup.into_values())
            .map(|lane| (Self::MAX_ALTERNATIVES - lane.len(), lane))
    }

    pub(super) fn merge(
        &mut self,
        path: Option<OutcomePath>,
        incoming: Cow<'_, State>,
        mut join: impl FnMut(&mut State, &State) -> bool,
    ) -> bool {
        let lane = match incoming.cleanup_progress() {
            Some(progress) => self.cleanup.entry(progress.clone()).or_default(),
            None => &mut self.entries,
        };

        if lane.is_empty() {
            lane.push((path, incoming.into_owned()));

            return true;
        }

        if let [(None, combined)] = lane.as_mut_slice() {
            return join(combined, &incoming);
        }

        if let Some((_, state)) = lane.iter_mut().find(|(current, _)| current == &path) {
            return join(state, &incoming);
        }

        if path.is_none() || lane.len() == Self::MAX_ALTERNATIVES {
            let mut outcomes = std::mem::take(lane).into_iter();

            let (_, mut combined) = outcomes.next().expect("widening has a reached outcome");

            for (_, state) in outcomes {
                join(&mut combined, &state);
            }

            join(&mut combined, &incoming);
            lane.push((None, combined));
        } else {
            lane.push((path, incoming.into_owned()));
            lane.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        }

        true
    }
}
