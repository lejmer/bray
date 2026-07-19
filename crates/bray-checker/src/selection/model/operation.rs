use std::sync::Arc;

use bray_base::shared_slice;
use bray_bound_tree::{
    BoundExpressionId, ConstructionDefaultProvider, ConstructionInputId, ConstructionTarget,
    SelectedOperation, SelectionKind,
};
use bray_symbols::{
    CallableInstanceData, CallableSignature, ImplementationSelectionKey, TraitSymbolId,
};
use bray_symbols::{CallablePosition, SymbolKey, SymbolName, TypeId};

use super::{ImplementationSelectionEvidence, SelectionCandidateKey};

/// Whether a binder-enumerated candidate can participate before type compatibility is checked.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OperationCandidateState {
    /// The candidate is visible, target-available, and statically permitted.
    Available,
    /// The candidate exists but is not visible from the requesting context.
    Inaccessible,
    /// The candidate is excluded by target availability or static constraints.
    Unavailable,
    /// The candidate surface contains prior semantic recovery.
    Recovered,
}

/// One declaration input participating in construction mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstructionInputSurface {
    input: ConstructionInputId,
    name: SymbolName,
    position: CallablePosition,
    ty: TypeId,
    default: Option<ConstructionDefaultProvider>,
    ordinal: u32,
}

/// Compiler-known callable contract evidence for one trait-backed operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraitOperationEvidence {
    requirement: ImplementationSelectionKey,
    trait_definition: TraitSymbolId,
    callable: CallableInstanceData,
    signature: CallableSignature,
}

impl TraitOperationEvidence {
    /// Creates evidence tying one requirement to an exact compiler-known callable contract.
    pub const fn new(
        requirement: ImplementationSelectionKey,
        trait_definition: TraitSymbolId,
        callable: CallableInstanceData,
        signature: CallableSignature,
    ) -> Self {
        Self {
            requirement,
            trait_definition,
            callable,
            signature,
        }
    }

    /// Returns the implementation requirement described by this contract.
    pub const fn requirement(&self) -> ImplementationSelectionKey {
        self.requirement
    }

    /// Returns the exact compiler-known trait definition.
    pub const fn trait_definition(&self) -> TraitSymbolId {
        self.trait_definition
    }

    /// Returns the exact substituted compiler-known callable member.
    pub const fn callable(&self) -> CallableInstanceData {
        self.callable
    }

    /// Returns the callable member's checked signature.
    pub const fn signature(&self) -> &CallableSignature {
        &self.signature
    }
}

impl ConstructionInputSurface {
    /// Creates one declaration-ordered construction input.
    pub const fn new(
        input: ConstructionInputId,
        name: SymbolName,
        position: CallablePosition,
        ty: TypeId,
        default: Option<ConstructionDefaultProvider>,
        ordinal: u32,
    ) -> Self {
        Self {
            input,
            name,
            position,
            ty,
            default,
            ordinal,
        }
    }

    /// Returns the exact field or parameter identity.
    pub const fn input(&self) -> ConstructionInputId {
        self.input
    }

    /// Returns the source-visible construction input name.
    pub const fn name(&self) -> &SymbolName {
        &self.name
    }

    /// Returns whether this input permits positional construction syntax.
    pub const fn position(&self) -> CallablePosition {
        self.position
    }

    /// Returns the exact checked input type.
    pub const fn ty(&self) -> TypeId {
        self.ty
    }

    /// Returns the declaration-owned default provider when omission is permitted.
    pub const fn default(&self) -> Option<ConstructionDefaultProvider> {
        self.default
    }

    /// Returns the input's declaration-order ordinal.
    pub const fn ordinal(&self) -> u32 {
        self.ordinal
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::selection) enum OperationCandidatePlan {
    Exact {
        operation: SelectedOperation,
        operand_types: Arc<[TypeId]>,
    },
    Construction {
        target: ConstructionTarget,
        result_type: TypeId,
        inputs: Arc<[ConstructionInputSurface]>,
    },
}

/// One binder-enumerated operation candidate in deterministic semantic-key order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationCandidate {
    key: SelectionCandidateKey,
    plan: OperationCandidatePlan,
    implementation_selections: Arc<[ImplementationSelectionEvidence]>,
    trait_operations: Arc<[TraitOperationEvidence]>,
    state: OperationCandidateState,
}

impl OperationCandidate {
    /// Creates one declaration-backed non-construction operation candidate.
    pub fn symbol(
        key: SymbolKey,
        operation: SelectedOperation,
        operand_types: impl IntoIterator<Item = TypeId>,
        state: OperationCandidateState,
    ) -> Self {
        Self::exact(key.into(), operation, operand_types, state)
    }

    /// Creates one compiler-defined non-construction operation candidate.
    pub fn built_in(
        operation: SelectedOperation,
        operand_types: impl IntoIterator<Item = TypeId>,
        state: OperationCandidateState,
    ) -> Self {
        Self::exact(
            SelectionCandidateKey::BuiltIn,
            operation,
            operand_types,
            state,
        )
    }

    /// Creates one declaration-backed construction candidate.
    pub fn symbol_construction(
        key: SymbolKey,
        target: ConstructionTarget,
        result_type: TypeId,
        inputs: impl IntoIterator<Item = ConstructionInputSurface>,
        state: OperationCandidateState,
    ) -> Self {
        Self::construction(key.into(), target, result_type, inputs, state)
    }

    /// Creates one compiler-defined construction candidate.
    pub fn built_in_construction(
        target: ConstructionTarget,
        result_type: TypeId,
        inputs: impl IntoIterator<Item = ConstructionInputSurface>,
        state: OperationCandidateState,
    ) -> Self {
        Self::construction(
            SelectionCandidateKey::BuiltIn,
            target,
            result_type,
            inputs,
            state,
        )
    }

    fn exact(
        key: SelectionCandidateKey,
        operation: SelectedOperation,
        operand_types: impl IntoIterator<Item = TypeId>,
        state: OperationCandidateState,
    ) -> Self {
        Self {
            key,
            plan: OperationCandidatePlan::Exact {
                operation,
                operand_types: shared_slice(operand_types),
            },
            implementation_selections: Arc::new([]),
            trait_operations: Arc::new([]),
            state,
        }
    }

    fn construction(
        key: SelectionCandidateKey,
        target: ConstructionTarget,
        result_type: TypeId,
        inputs: impl IntoIterator<Item = ConstructionInputSurface>,
        state: OperationCandidateState,
    ) -> Self {
        Self {
            key,
            plan: OperationCandidatePlan::Construction {
                target,
                result_type,
                inputs: shared_slice(inputs),
            },
            implementation_selections: Arc::new([]),
            trait_operations: Arc::new([]),
            state,
        }
    }

    /// Supplies typed implementation-selection facts used by this operation.
    pub fn with_implementation_selections(
        mut self,
        selections: impl IntoIterator<Item = ImplementationSelectionEvidence>,
    ) -> Self {
        self.implementation_selections = shared_slice(selections);

        self
    }

    /// Supplies exact compiler-known contracts used by trait-backed operations.
    pub fn with_trait_operations(
        mut self,
        operations: impl IntoIterator<Item = TraitOperationEvidence>,
    ) -> Self {
        self.trait_operations = shared_slice(operations);

        self
    }

    /// Returns the stable semantic key used for deterministic ordering.
    pub const fn key(&self) -> &SelectionCandidateKey {
        &self.key
    }

    /// Returns this candidate's non-type participation state.
    pub const fn state(&self) -> OperationCandidateState {
        self.state
    }

    pub(in crate::selection) fn into_parts(
        self,
    ) -> (
        SelectionCandidateKey,
        OperationCandidatePlan,
        Arc<[ImplementationSelectionEvidence]>,
        Arc<[TraitOperationEvidence]>,
        OperationCandidateState,
    ) {
        (
            self.key,
            self.plan,
            self.implementation_selections,
            self.trait_operations,
            self.state,
        )
    }
}

/// Inputs for selecting one non-call semantic operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationSelectionRequest {
    pub(in crate::selection) expression: BoundExpressionId,
    pub(in crate::selection) kind: SelectionKind,
    pub(in crate::selection) operands: Arc<[BoundExpressionId]>,
    pub(in crate::selection) candidates: Vec<OperationCandidate>,
}

impl OperationSelectionRequest {
    /// Creates a selection request from binder-associated operands and candidates.
    pub fn new(
        expression: BoundExpressionId,
        kind: SelectionKind,
        operands: impl IntoIterator<Item = BoundExpressionId>,
        candidates: impl IntoIterator<Item = OperationCandidate>,
    ) -> Self {
        Self {
            expression,
            kind,
            operands: shared_slice(operands),
            candidates: candidates.into_iter().collect(),
        }
    }

    /// Returns the expression occurrence that owns this selection.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the requested semantic category.
    pub const fn kind(&self) -> SelectionKind {
        self.kind
    }

    /// Returns operand occurrences in semantic evaluation order.
    pub fn operands(&self) -> &[BoundExpressionId] {
        &self.operands
    }
}
