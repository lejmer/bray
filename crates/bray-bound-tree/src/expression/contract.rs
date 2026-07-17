use super::{
    CheckedConstruction, CheckedConversion, CheckedExpressionFactEntry, CheckedExpressionResult,
    CheckedIndexSelection, CheckedLiteral, CheckedMemberSelection, CheckedOperatorSelection,
};
use crate::{AnyBoundNodeId, BoundExpressionId};

/// A focused checked-expression fact domain.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedExpressionFactKind {
    /// Result type and recovery state.
    Result,
    /// Canonical literal value.
    Literal,
    /// Selected source operator operation.
    Operator,
    /// Resolved member target.
    Member,
    /// Selected indexing contract.
    Index,
    /// Resolved construction target and inputs.
    Construction,
    /// Exact invocation target, ABI, and arguments.
    Invocation,
    /// Checked conversion plan.
    Conversion,
}

/// A contract violation that prevents checked-expression fact publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckedExpressionFactBuildError {
    /// One fact names an expression from another bound unit.
    ForeignExpression {
        /// The focused fact domain being validated.
        kind: CheckedExpressionFactKind,
        /// The foreign expression occurrence.
        expression: BoundExpressionId,
    },
    /// One fact names no committed expression in the canonical bound unit.
    MissingExpression {
        /// The focused fact domain being validated.
        kind: CheckedExpressionFactKind,
        /// The absent expression occurrence.
        expression: BoundExpressionId,
    },
    /// One focused domain supplied more than one fact for an expression.
    DuplicateFact {
        /// The duplicated focused fact domain.
        kind: CheckedExpressionFactKind,
        /// The duplicated expression occurrence.
        expression: BoundExpressionId,
    },
    /// An expression reachable from the unit root has no checked result.
    MissingResult(BoundExpressionId),
    /// A fact was supplied for a committed expression outside the canonical unit root.
    UnreachableFact {
        /// The focused fact domain being validated.
        kind: CheckedExpressionFactKind,
        /// The committed but unreachable expression occurrence.
        expression: BoundExpressionId,
    },
    /// A valid expression lacks a required category-specific semantic decision.
    MissingRequiredFact {
        /// The required focused fact domain.
        kind: CheckedExpressionFactKind,
        /// The expression requiring the fact.
        expression: BoundExpressionId,
    },
    /// A category-specific fact was attached to an incompatible bound expression.
    WrongExpressionCategory {
        /// The incompatible focused fact domain.
        kind: CheckedExpressionFactKind,
        /// The expression receiving the fact.
        expression: BoundExpressionId,
    },
    /// A recovered bound node was incorrectly published as semantically valid.
    RecoveredExpressionMarkedValid(BoundExpressionId),
    /// Category-specific facts disagree with each other or the checked result.
    InconsistentFact {
        /// The inconsistent focused fact domain.
        kind: CheckedExpressionFactKind,
        /// The expression carrying inconsistent data.
        expression: BoundExpressionId,
    },
    /// Canonical traversal encountered a missing committed relationship.
    MissingBoundNode(AnyBoundNodeId),
    /// Canonical traversal stopped before visiting the complete bound unit.
    TraversalStopped,
}

/// Typed expression facts awaiting validation against one canonical bound unit.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CheckedExpressionFactInput {
    pub(super) results: Vec<CheckedExpressionFactEntry<CheckedExpressionResult>>,
    pub(super) literals: Vec<CheckedExpressionFactEntry<CheckedLiteral>>,
    pub(super) operators: Vec<CheckedExpressionFactEntry<CheckedOperatorSelection>>,
    pub(super) members: Vec<CheckedExpressionFactEntry<CheckedMemberSelection>>,
    pub(super) indexes: Vec<CheckedExpressionFactEntry<CheckedIndexSelection>>,
    pub(super) constructions: Vec<CheckedExpressionFactEntry<CheckedConstruction>>,
    pub(super) conversions: Vec<CheckedExpressionFactEntry<CheckedConversion>>,
}

impl CheckedExpressionFactInput {
    /// Creates an input with complete expression result facts.
    pub fn new(
        results: impl IntoIterator<Item = CheckedExpressionFactEntry<CheckedExpressionResult>>,
    ) -> Self {
        Self {
            results: results.into_iter().collect(),
            ..Self::default()
        }
    }

    /// Adds canonical literal values.
    pub fn with_literals(
        mut self,
        literals: impl IntoIterator<Item = CheckedExpressionFactEntry<CheckedLiteral>>,
    ) -> Self {
        self.literals.extend(literals);
        self
    }

    /// Adds selected unary and binary operator operations.
    pub fn with_operators(
        mut self,
        operators: impl IntoIterator<Item = CheckedExpressionFactEntry<CheckedOperatorSelection>>,
    ) -> Self {
        self.operators.extend(operators);
        self
    }

    /// Adds resolved member targets.
    pub fn with_members(
        mut self,
        members: impl IntoIterator<Item = CheckedExpressionFactEntry<CheckedMemberSelection>>,
    ) -> Self {
        self.members.extend(members);
        self
    }

    /// Adds selected indexing contracts.
    pub fn with_indexes(
        mut self,
        indexes: impl IntoIterator<Item = CheckedExpressionFactEntry<CheckedIndexSelection>>,
    ) -> Self {
        self.indexes.extend(indexes);
        self
    }

    /// Adds resolved value-construction targets and inputs.
    pub fn with_constructions(
        mut self,
        constructions: impl IntoIterator<Item = CheckedExpressionFactEntry<CheckedConstruction>>,
    ) -> Self {
        self.constructions.extend(constructions);
        self
    }

    /// Adds checked explicit conversion plans.
    pub fn with_conversions(
        mut self,
        conversions: impl IntoIterator<Item = CheckedExpressionFactEntry<CheckedConversion>>,
    ) -> Self {
        self.conversions.extend(conversions);
        self
    }
}
