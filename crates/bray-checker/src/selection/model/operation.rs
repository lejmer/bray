use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};
use bray_bound_tree::{BoundExpressionId, BoundOperator};
use bray_symbols::{
    AnySymbolId, CallableInstanceData, ImplementationInstanceId, ImplementationSelectionKey,
    StructSymbolId, SymbolKey, TypeId, UnionVariantSymbolId,
};

use super::{SelectionCandidateKey, SelectionKind};

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

/// The exact semantic target of a member selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemberTarget {
    member: AnySymbolId,
    result_type: TypeId,
    witnesses: Arc<[ImplementationInstanceId]>,
}

impl MemberTarget {
    /// Creates one exact member target and its selected implementation witnesses.
    pub fn new(
        member: AnySymbolId,
        result_type: TypeId,
        witnesses: impl IntoIterator<Item = ImplementationInstanceId>,
    ) -> Self {
        Self {
            member,
            result_type,
            witnesses: sorted_unique_shared_slice(witnesses),
        }
    }

    /// Returns the exact selected member.
    pub const fn member(&self) -> AnySymbolId {
        self.member
    }

    /// Returns the member access result type.
    pub const fn result_type(&self) -> TypeId {
        self.result_type
    }

    /// Returns implementation witnesses in canonical semantic order.
    pub fn witnesses(&self) -> &[ImplementationInstanceId] {
        &self.witnesses
    }
}

/// The exact implementation of a source unary or binary operator.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OperatorTarget {
    /// Compiler-defined behavior for a non-overloadable operator.
    BuiltIn(BoundOperator),
    /// A selected compiler-known operator trait member and implementation witness.
    Trait {
        /// The exact substituted callable member.
        callable: CallableInstanceData,
        /// The exact implementation witness.
        witness: ImplementationInstanceId,
    },
}

/// The exact implementation of element or slice access.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum IndexTarget {
    /// Built-in fixed-array element projection.
    ArrayElement,
    /// Built-in slice element projection.
    SliceElement,
    /// Built-in fixed-array slice projection.
    ArraySlice,
    /// Built-in slice projection.
    Slice,
    /// A selected custom indexing contract callable.
    Custom(CallableInstanceData),
}

/// The exact declaration or type-form behavior used for construction.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConstructionTarget {
    /// A named struct and its field surface.
    Struct(StructSymbolId),
    /// A union variant and its payload surface.
    UnionVariant(UnionVariantSymbolId),
    /// A selected type-form construction callable.
    TypeForm(CallableInstanceData),
}

/// The exact rule used by an explicit conversion expression.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConversionTarget {
    /// Source and target are the same semantic type.
    Identity,
    /// A compiler-defined total value-preserving scalar conversion.
    BuiltInScalar,
    /// A compiler-defined recursive structural conversion.
    BuiltInComposite,
    /// A selected `ConvertTo<Target>` member and implementation witness.
    Trait {
        /// The exact substituted conversion member.
        callable: CallableInstanceData,
        /// The exact implementation witness.
        witness: ImplementationInstanceId,
    },
}

/// One exact operation whose operands have already been associated by binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectedOperation {
    /// A receiver-associated member.
    Member(MemberTarget),
    /// A unary or binary operator.
    Operator {
        /// The selected implementation.
        target: OperatorTarget,
        /// The expression result type.
        result_type: TypeId,
    },
    /// Element or slice indexing.
    Index {
        /// The selected indexing contract.
        target: IndexTarget,
        /// The expression result type.
        result_type: TypeId,
    },
    /// Struct, union variant, or type-form construction.
    Construction {
        /// The selected construction behavior.
        target: ConstructionTarget,
        /// The constructed type.
        result_type: TypeId,
    },
    /// An explicit conversion.
    Conversion {
        /// The selected conversion rule.
        target: ConversionTarget,
        /// The explicit target type.
        result_type: TypeId,
    },
    /// A trait implementation witness selected for an exact requirement.
    Implementation {
        /// The exact subject and trait application being satisfied.
        requirement: ImplementationSelectionKey,
        /// The selected implementation witness.
        witness: ImplementationInstanceId,
    },
}

impl SelectedOperation {
    /// Returns this operation's closed selection category.
    pub const fn kind(&self) -> SelectionKind {
        match self {
            Self::Member(_) => SelectionKind::Member,
            Self::Operator { .. } => SelectionKind::Operator,
            Self::Index { .. } => SelectionKind::Index,
            Self::Construction { .. } => SelectionKind::Construction,
            Self::Conversion { .. } => SelectionKind::Conversion,
            Self::Implementation { .. } => SelectionKind::Implementation,
        }
    }

    /// Returns the selected operation's result type when it produces a value.
    pub const fn result_type(&self) -> Option<TypeId> {
        match self {
            Self::Member(target) => Some(target.result_type()),
            Self::Operator { result_type, .. }
            | Self::Index { result_type, .. }
            | Self::Construction { result_type, .. }
            | Self::Conversion { result_type, .. } => Some(*result_type),
            Self::Implementation { .. } => None,
        }
    }
}

/// One binder-enumerated operation candidate in deterministic semantic-key order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationCandidate {
    key: SelectionCandidateKey,
    operation: SelectedOperation,
    operand_types: Arc<[TypeId]>,
    state: OperationCandidateState,
}

impl OperationCandidate {
    /// Creates one declaration-backed operation candidate.
    pub fn symbol(
        key: SymbolKey,
        operation: SelectedOperation,
        operand_types: impl IntoIterator<Item = TypeId>,
        state: OperationCandidateState,
    ) -> Self {
        Self::new(key.into(), operation, operand_types, state)
    }

    /// Creates the compiler-defined operation candidate for a request.
    pub fn built_in(
        operation: SelectedOperation,
        operand_types: impl IntoIterator<Item = TypeId>,
        state: OperationCandidateState,
    ) -> Self {
        Self::new(
            SelectionCandidateKey::BuiltIn,
            operation,
            operand_types,
            state,
        )
    }

    fn new(
        key: SelectionCandidateKey,
        operation: SelectedOperation,
        operand_types: impl IntoIterator<Item = TypeId>,
        state: OperationCandidateState,
    ) -> Self {
        Self {
            key,
            operation,
            operand_types: shared_slice(operand_types),
            state,
        }
    }

    /// Returns the stable semantic key used for deterministic ordering.
    pub const fn key(&self) -> &SelectionCandidateKey {
        &self.key
    }

    /// Returns the exact operation this candidate would select.
    pub const fn operation(&self) -> &SelectedOperation {
        &self.operation
    }

    /// Returns expected operand types in the request's operand order.
    pub fn operand_types(&self) -> &[TypeId] {
        &self.operand_types
    }

    /// Returns this candidate's non-type participation state.
    pub const fn state(&self) -> OperationCandidateState {
        self.state
    }

    pub(in crate::selection) fn into_parts(
        self,
    ) -> (
        SelectionCandidateKey,
        SelectedOperation,
        Arc<[TypeId]>,
        OperationCandidateState,
    ) {
        (self.key, self.operation, self.operand_types, self.state)
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

    /// Returns the expression occurrence that owns the selection.
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
