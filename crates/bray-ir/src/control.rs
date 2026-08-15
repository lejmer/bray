use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{ConstantTermId, ConstantValueId, StructSymbolId, TypeId, UnionVariantSymbolId};

use crate::{
    MirBlockId, MirCallableReference, MirFrameStateId, MirOperand, MirPlace, MirSourceAnchor,
};

/// Trusted inline assembly whose checked labels may leave the ordinary control-flow path.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirInlineAssemblyTerminator {
    contract: bray_bound_tree::InlineAssemblyContract,
    inputs: MirOperand,
    inputs_type: TypeId,
    output_type: TypeId,
    normal: MirBlockId,
    alternates: Arc<[MirBlockId]>,
    symbols: Arc<[MirCallableReference]>,
}

impl MirInlineAssemblyTerminator {
    /// Creates one explicit assembly terminator with normal and alternate successors.
    pub fn new(
        contract: bray_bound_tree::InlineAssemblyContract,
        inputs: MirOperand,
        inputs_type: TypeId,
        output_type: TypeId,
        normal: MirBlockId,
        alternates: impl IntoIterator<Item = MirBlockId>,
        symbols: impl IntoIterator<Item = MirCallableReference>,
    ) -> Self {
        Self {
            contract,
            inputs,
            inputs_type,
            output_type,
            normal,
            alternates: shared_slice(alternates),
            symbols: shared_slice(symbols),
        }
    }

    /// Returns the exactly checked assembly contract.
    pub const fn contract(&self) -> bray_bound_tree::InlineAssemblyContract {
        self.contract
    }

    /// Returns the structural input tuple.
    pub const fn inputs(&self) -> &MirOperand {
        &self.inputs
    }

    /// Returns the structural input tuple type.
    pub const fn inputs_type(&self) -> TypeId {
        self.inputs_type
    }

    /// Returns the structural output tuple supplied to normal continuation.
    pub const fn output_type(&self) -> TypeId {
        self.output_type
    }

    /// Returns the sole ordinary continuation block.
    pub const fn normal(&self) -> MirBlockId {
        self.normal
    }

    /// Returns alternate successor blocks in checked label-constraint order.
    pub fn alternates(&self) -> &[MirBlockId] {
        &self.alternates
    }

    /// Returns closed callable symbols referenced by assembly operands in descriptor order.
    pub fn symbols(&self) -> &[MirCallableReference] {
        &self.symbols
    }
}

/// One source-independent structural condition tested by MIR control flow.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirPatternPredicate {
    /// The subject equals one canonical typed literal value.
    Literal(ConstantValueId),
    /// The subject equals one checked open or closed constant term.
    Constant(ConstantTermId),
    /// The nullable subject is absent.
    NullableAbsent,
    /// The nullable subject is present.
    NullablePresent,
    /// The named union has one active variant.
    ActiveUnionVariant(UnionVariantSymbolId),
    /// The subject has one named product shape.
    ProductShape(StructSymbolId),
    /// The subject has one tuple arity.
    TupleShape(u32),
    /// The subject has one fixed array length.
    ArrayShape(u32),
    /// The subject is available through owned indirection.
    OwnedTarget,
}

impl From<bray_bound_tree::PatternPredicate> for MirPatternPredicate {
    fn from(predicate: bray_bound_tree::PatternPredicate) -> Self {
        match predicate {
            bray_bound_tree::PatternPredicate::Literal(literal) => Self::Literal(literal.value()),
            bray_bound_tree::PatternPredicate::Constant(term) => Self::Constant(term),
            bray_bound_tree::PatternPredicate::NullableAbsent => Self::NullableAbsent,
            bray_bound_tree::PatternPredicate::NullablePresent => Self::NullablePresent,
            bray_bound_tree::PatternPredicate::ActiveUnionVariant(variant) => {
                Self::ActiveUnionVariant(variant)
            }
            bray_bound_tree::PatternPredicate::ProductShape(product) => Self::ProductShape(product),
            bray_bound_tree::PatternPredicate::TupleShape(arity) => Self::TupleShape(arity),
            bray_bound_tree::PatternPredicate::ArrayShape(length) => Self::ArrayShape(length),
            bray_bound_tree::PatternPredicate::OwnedTarget => Self::OwnedTarget,
        }
    }
}

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
    completed_variant: bray_symbols::UnionVariantSymbolId,
    completed: MirEdge,
    panicked_variant: bray_symbols::UnionVariantSymbolId,
    panicked: MirCleanupEdge,
    cancelled_variant: bray_symbols::UnionVariantSymbolId,
    cancelled: MirCleanupEdge,
}

/// Reason a protected frame voluntarily suspended.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MirSuspensionKind {
    /// The frame is waiting for a directly composed child frame.
    Awaited,
    /// The frame yielded so another ready task can run.
    Yield,
}

impl MirSuspensionKind {
    /// Returns the stable machine-readable suspension name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Awaited => "awaited",
            Self::Yield => "yield",
        }
    }
}

impl MirRunResultEdges {
    /// Creates the three distinct run-result successors.
    pub fn new(
        completed: (bray_symbols::UnionVariantSymbolId, MirEdge),
        panicked: (bray_symbols::UnionVariantSymbolId, MirCleanupEdge),
        cancelled: (bray_symbols::UnionVariantSymbolId, MirCleanupEdge),
    ) -> Self {
        Self {
            completed_variant: completed.0,
            completed: completed.1,
            panicked_variant: panicked.0,
            panicked: panicked.1,
            cancelled_variant: cancelled.0,
            cancelled: cancelled.1,
        }
    }

    /// Returns the completed variant selected by checked lowering.
    pub const fn completed_variant(&self) -> bray_symbols::UnionVariantSymbolId {
        self.completed_variant
    }

    /// Returns the normal completion successor.
    pub const fn completed(&self) -> &MirEdge {
        &self.completed
    }

    /// Returns the panicked variant selected by checked lowering.
    pub const fn panicked_variant(&self) -> bray_symbols::UnionVariantSymbolId {
        self.panicked_variant
    }

    /// Returns the panic cleanup successor.
    pub const fn panicked(&self) -> &MirCleanupEdge {
        &self.panicked
    }

    /// Returns the cancelled variant selected by checked lowering.
    pub const fn cancelled_variant(&self) -> bray_symbols::UnionVariantSymbolId {
        self.cancelled_variant
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
        predicate: MirPatternPredicate,
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
        /// Exact implementation supplying the cursor-advance callable.
        witness: bray_symbols::ImplementationInstanceId,
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
    /// Execute target-gated assembly that may transfer to one typed external label.
    InlineAssembly(MirInlineAssemblyTerminator),
    /// Return from this unit.
    Return(Option<MirOperand>),
    /// End a path that cannot continue.
    Unreachable,
    /// Suspend a protected frame and retain its checked resume state.
    Suspend {
        /// Reason this frame suspended.
        kind: MirSuspensionKind,
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
    /// Transfer an already-cleaned panic report to the nearest native run boundary.
    PropagatePanic {
        /// Owned panic report being propagated.
        report: MirOperand,
        /// Selected private panic-propagation ABI role.
        runtime: crate::MirRuntimeReference,
    },
    /// Transfer cancellation to the nearest native run boundary.
    PropagateCancellation {
        /// Selected private cancellation-entry ABI role.
        runtime: crate::MirRuntimeReference,
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

impl MirTerminatorKind {
    /// Visits every control-flow successor in deterministic operand order.
    pub fn for_each_successor(&self, mut visit: impl FnMut(MirBlockId)) {
        match self {
            Self::Goto(edge) => visit(edge.target()),
            Self::Branch {
                then_edge,
                else_edge,
                ..
            } => {
                visit(then_edge.target());
                visit(else_edge.target());
            }
            Self::PatternBranch {
                matched, unmatched, ..
            } => {
                visit(matched.target());
                visit(unmatched.target());
            }
            Self::Iterate {
                item, exhausted, ..
            } => {
                visit(*item);
                visit(exhausted.target());
            }
            Self::Switch {
                cases, otherwise, ..
            } => {
                for case in cases.iter() {
                    visit(case.edge().target());
                }

                visit(otherwise.target());
            }
            Self::InlineAssembly(assembly) => {
                visit(assembly.normal());

                for alternate in assembly.alternates() {
                    visit(*alternate);
                }
            }
            Self::Suspend {
                resume,
                cancellation,
                ..
            } => {
                visit(resume.target());
                visit(cancellation.edge().target());
            }
            Self::ForwardRunResult { edges, .. } => {
                visit(edges.completed().target());
                visit(edges.panicked().edge().target());
                visit(edges.cancelled().edge().target());
            }
            Self::BeginCleanup(cleanup)
            | Self::ContinueCleanup(cleanup)
            | Self::Panic { cleanup, .. }
            | Self::CancelCurrentRun { cleanup } => visit(cleanup.edge().target()),
            Self::Return(_)
            | Self::Unreachable
            | Self::PropagatePanic { .. }
            | Self::PropagateCancellation { .. } => {}
        }
    }

    /// Returns whether a pattern requires an externally materialized value.
    pub const fn requires_pattern_value_mapping(&self) -> bool {
        matches!(
            self,
            Self::PatternBranch {
                predicate: MirPatternPredicate::Literal(_) | MirPatternPredicate::Constant(_),
                ..
            }
        )
    }

    /// Returns the canonical typed value tested by a literal pattern branch.
    pub const fn pattern_literal_value(&self) -> Option<bray_symbols::ConstantValueId> {
        match self {
            Self::PatternBranch {
                predicate: MirPatternPredicate::Literal(literal),
                ..
            } => Some(*literal),
            _ => None,
        }
    }

    /// Returns the closed constant term tested by a pattern branch, when present.
    pub const fn pattern_constant_term(&self) -> Option<bray_symbols::ConstantTermId> {
        match self {
            Self::PatternBranch {
                predicate: MirPatternPredicate::Constant(term),
                ..
            } => Some(*term),
            Self::Goto(_)
            | Self::Branch { .. }
            | Self::PatternBranch { .. }
            | Self::Iterate { .. }
            | Self::Switch { .. }
            | Self::InlineAssembly(_)
            | Self::Return(_)
            | Self::Unreachable
            | Self::PropagatePanic { .. }
            | Self::PropagateCancellation { .. }
            | Self::Suspend { .. }
            | Self::ForwardRunResult { .. }
            | Self::BeginCleanup(_)
            | Self::ContinueCleanup(_)
            | Self::Panic { .. }
            | Self::CancelCurrentRun { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        InlineAssemblyContract, InlineAssemblyOperand, InlineAssemblyOperandKind,
        MAX_INLINE_ASSEMBLY_OPERANDS,
    };

    use super::{MirInlineAssemblyTerminator, MirTerminatorKind};
    use crate::{MirBlockId, MirImmediateValue, MirOperand, MirUnitId};

    #[test]
    fn inline_assembly_successors_include_normal_and_every_alternate() {
        let unit = MirUnitId::new(7);
        let normal = MirBlockId::from_slot(unit, 1);
        let first = MirBlockId::from_slot(unit, 2);
        let second = MirBlockId::from_slot(unit, 3);
        let ty = crate::test_support::test_type();
        let constant = crate::test_support::test_constant_value();
        let mut operands = [None; MAX_INLINE_ASSEMBLY_OPERANDS];

        operands[0] = Some(InlineAssemblyOperand::new(
            InlineAssemblyOperandKind::Label,
            ty,
            None,
            None,
            None,
            None,
            None,
            0,
            5,
        ));

        let contract = InlineAssemblyContract::try_new(
            constant,
            constant,
            constant,
            constant,
            constant,
            operands,
            1,
            "",
            "label",
        )
        .unwrap_or_else(|| panic!("test assembly contract must validate"));

        let terminator = MirTerminatorKind::InlineAssembly(MirInlineAssemblyTerminator::new(
            contract,
            MirOperand::Immediate {
                value: MirImmediateValue::Unit,
                ty,
            },
            ty,
            ty,
            normal,
            [first, second],
            [],
        ));

        let mut successors = Vec::new();

        terminator.for_each_successor(|successor| successors.push(successor));

        assert_eq!(successors, [normal, first, second]);
    }
}
