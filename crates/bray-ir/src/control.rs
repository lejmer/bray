use std::sync::Arc;

use bray_base::shared_slice;
use bray_bound_tree::PatternPredicate;
use bray_symbols::{ConstantValueId, TypeId};

use crate::{
    MirBlockId, MirCallableReference, MirFrameStateId, MirOperand, MirPlace, MirSourceAnchor,
};

/// One control-flow transfer and its block arguments.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirEdge {
    target: MirBlockId,
    arguments: Arc<[MirOperand]>,
}

impl MirEdge {
    /// Creates an edge from its target and ordered block arguments.
    pub fn new(target: MirBlockId, arguments: impl IntoIterator<Item = MirOperand>) -> Self {
        Self {
            target,
            arguments: shared_slice(arguments),
        }
    }

    /// Returns the destination block.
    pub const fn target(&self) -> MirBlockId {
        self.target
    }

    /// Returns destination block arguments in parameter order.
    pub fn arguments(&self) -> &[MirOperand] {
        &self.arguments
    }
}

/// Ordered phase of checked cleanup control flow.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirCleanupPhase {
    /// Request cancellation for every selected task without waiting.
    TaskCancellation,
    /// Resolve task and ordinary lifecycle obligations after cancellation broadcast.
    LifecycleResolution,
}

/// A control-flow edge whose target performs one checked cleanup phase.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirCleanupEdge {
    phase: MirCleanupPhase,
    edge: MirEdge,
}

impl MirCleanupEdge {
    /// Creates a cleanup edge with an explicit phase.
    pub const fn new(phase: MirCleanupPhase, edge: MirEdge) -> Self {
        Self { phase, edge }
    }

    /// Returns the cleanup phase entered by this edge.
    pub const fn phase(&self) -> MirCleanupPhase {
        self.phase
    }

    /// Returns the underlying block transfer.
    pub const fn edge(&self) -> &MirEdge {
        &self.edge
    }
}

/// One constant dispatch case in a MIR switch.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirSwitchCase {
    value: ConstantValueId,
    edge: MirEdge,
}

impl MirSwitchCase {
    /// Creates a dispatch case from its canonical constant and destination.
    pub const fn new(value: ConstantValueId, edge: MirEdge) -> Self {
        Self { value, edge }
    }

    /// Returns the canonical case value.
    pub const fn value(&self) -> ConstantValueId {
        self.value
    }

    /// Returns the selected destination.
    pub const fn edge(&self) -> &MirEdge {
        &self.edge
    }
}

/// Completed, panicked, and cancelled successors of a run-result forwarding operation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirRunResultEdges {
    completed: MirEdge,
    panicked: MirCleanupEdge,
    cancelled: MirCleanupEdge,
}

impl MirRunResultEdges {
    /// Creates the three distinct run-result successors.
    pub const fn new(
        completed: MirEdge,
        panicked: MirCleanupEdge,
        cancelled: MirCleanupEdge,
    ) -> Self {
        Self {
            completed,
            panicked,
            cancelled,
        }
    }

    /// Returns the normal completion successor.
    pub const fn completed(&self) -> &MirEdge {
        &self.completed
    }

    /// Returns the panic cleanup successor.
    pub const fn panicked(&self) -> &MirCleanupEdge {
        &self.panicked
    }

    /// Returns the cancellation cleanup successor.
    pub const fn cancelled(&self) -> &MirCleanupEdge {
        &self.cancelled
    }
}

/// Explicit control-flow operation that ends one MIR block.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirTerminatorKind {
    /// Continue at one destination.
    Goto(MirEdge),
    /// Select one of two destinations from a boolean condition.
    Branch {
        /// Checked boolean condition.
        condition: MirOperand,
        /// Destination when the condition is true.
        then_edge: MirEdge,
        /// Destination when the condition is false.
        else_edge: MirEdge,
    },
    /// Select a successor by applying one checked structural pattern predicate.
    PatternBranch {
        /// Subject value inspected by the predicate.
        subject: MirOperand,
        /// Exact predicate selected by pattern checking.
        predicate: PatternPredicate,
        /// Destination when the predicate matches.
        matched: MirEdge,
        /// Destination when the predicate does not match.
        unmatched: MirEdge,
    },
    /// Advance one selected iteration cursor.
    Iterate {
        /// Mutable cursor storage retained across iteration steps.
        cursor: MirPlace,
        /// Exact selected cursor-advance callable.
        next: MirCallableReference,
        /// Checked element type produced on the item edge.
        element_type: TypeId,
        /// Item block receiving the produced element as its sole parameter.
        item: MirBlockId,
        /// Destination reached on natural exhaustion.
        exhausted: MirEdge,
    },
    /// Dispatch on a checked value using canonical constant cases.
    Switch {
        /// Checked discriminant.
        discriminant: MirOperand,
        /// Cases in deterministic source order.
        cases: Arc<[MirSwitchCase]>,
        /// Destination when no case matches.
        otherwise: MirEdge,
    },
    /// Return from this unit.
    Return(Option<MirOperand>),
    /// End a path that cannot continue.
    Unreachable,
    /// Suspend a protected frame and retain its checked resume state.
    Suspend {
        /// State entered when execution resumes.
        resume_state: MirFrameStateId,
        /// Destination used after the frame is resumed.
        resume: MirEdge,
        /// Cleanup entered when current-run cancellation is observed.
        cancellation: MirCleanupEdge,
        /// Selected private suspension-registration ABI role.
        registration: crate::MirRuntimeReference,
        /// Selected private wake ABI role.
        wake: crate::MirRuntimeReference,
    },
    /// Forward completed, panicked, or cancelled run state without collapsing outcomes.
    ForwardRunResult {
        /// Run-result value being inspected.
        result: MirOperand,
        /// Distinct successor edges.
        edges: MirRunResultEdges,
    },
    /// Enter phase-one cleanup.
    BeginCleanup(MirCleanupEdge),
    /// Continue from phase one into phase-two lifecycle resolution.
    ContinueCleanup(MirCleanupEdge),
    /// Abandon normal continuation with ownership of a panic report.
    Panic {
        /// Panic report being transferred.
        report: MirOperand,
        /// Cleanup entered before propagation.
        cleanup: MirCleanupEdge,
    },
    /// Abandon normal continuation because the current run was cancelled.
    CancelCurrentRun {
        /// Cleanup entered before propagation.
        cleanup: MirCleanupEdge,
    },
}

/// One source-correlated MIR terminator.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirTerminator {
    source: MirSourceAnchor,
    kind: MirTerminatorKind,
}

impl MirTerminator {
    /// Creates a terminator from its provenance and explicit control-flow operation.
    pub const fn new(source: MirSourceAnchor, kind: MirTerminatorKind) -> Self {
        Self { source, kind }
    }

    /// Returns the terminator's source provenance.
    pub const fn source(&self) -> &MirSourceAnchor {
        &self.source
    }

    /// Returns the explicit control-flow operation.
    pub const fn kind(&self) -> &MirTerminatorKind {
        &self.kind
    }
}
