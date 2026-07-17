use std::sync::Arc;

use bray_symbols::TypeId;

use crate::{BoundExpressionId, BoundUnitId, BoundUnitKind};

use super::{
    CheckedConstruction, CheckedConversion, CheckedExpressionFactInput, CheckedIndexSelection,
    CheckedLiteral, CheckedMemberSelection, CheckedOperatorSelection,
};

/// Whether checking established a valid expression result or retained recovery data.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedExpressionStatus {
    /// The result and all required category facts are valid.
    Valid,
    /// An earlier error prevented a complete semantic proof.
    Recovered,
}

/// The checked type and validity state of one expression occurrence.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedExpressionResult {
    ty: TypeId,
    status: CheckedExpressionStatus,
}

impl CheckedExpressionResult {
    /// Creates a checked expression result.
    pub const fn new(ty: TypeId, status: CheckedExpressionStatus) -> Self {
        Self { ty, status }
    }

    /// Returns the exact checked or recovery type.
    pub const fn ty(self) -> TypeId {
        self.ty
    }

    /// Returns whether this result is valid or retained for recovery.
    pub const fn status(self) -> CheckedExpressionStatus {
        self.status
    }

    /// Returns whether semantic recovery affected this result.
    pub const fn is_recovered(self) -> bool {
        matches!(self.status, CheckedExpressionStatus::Recovered)
    }
}

/// One expression ID and its category-specific durable fact.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedExpressionFactEntry<T> {
    expression: BoundExpressionId,
    value: T,
}

impl<T> CheckedExpressionFactEntry<T> {
    /// Creates one expression-associated fact.
    pub const fn new(expression: BoundExpressionId, value: T) -> Self {
        Self { expression, value }
    }

    /// Returns the exact source-correlated expression occurrence.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the category-specific checked fact.
    pub const fn value(&self) -> &T {
        &self.value
    }
}

/// Complete immutable value-producing expression facts for one exact bound unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedExpressionFacts {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    pub(super) results: Arc<[CheckedExpressionFactEntry<CheckedExpressionResult>]>,
    pub(super) literals: Arc<[CheckedExpressionFactEntry<CheckedLiteral>]>,
    pub(super) operators: Arc<[CheckedExpressionFactEntry<CheckedOperatorSelection>]>,
    pub(super) members: Arc<[CheckedExpressionFactEntry<CheckedMemberSelection>]>,
    pub(super) indexes: Arc<[CheckedExpressionFactEntry<CheckedIndexSelection>]>,
    pub(super) constructions: Arc<[CheckedExpressionFactEntry<CheckedConstruction>]>,
    pub(super) conversions: Arc<[CheckedExpressionFactEntry<CheckedConversion>]>,
}

impl CheckedExpressionFacts {
    pub(super) fn from_canonical(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        input: CheckedExpressionFactInput,
    ) -> Self {
        Self {
            unit,
            kind,
            results: input.results.into(),
            literals: input.literals.into(),
            operators: input.operators.into(),
            members: input.members.into(),
            indexes: input.indexes.into(),
            constructions: input.constructions.into(),
            conversions: input.conversions.into(),
        }
    }

    /// Returns the exact bound unit these facts describe.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the semantic category of the checked bound unit.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns expression results in canonical bound-expression ID order.
    pub fn results(&self) -> &[CheckedExpressionFactEntry<CheckedExpressionResult>] {
        &self.results
    }

    /// Returns the checked type and recovery state for an expression occurrence.
    pub fn result(&self, expression: BoundExpressionId) -> Option<CheckedExpressionResult> {
        entry(&self.results, expression).copied()
    }

    /// Returns the canonical literal value for an expression occurrence.
    pub fn literal(&self, expression: BoundExpressionId) -> Option<CheckedLiteral> {
        entry(&self.literals, expression).copied()
    }

    /// Returns the selected operator operation for an expression occurrence.
    pub fn operator(&self, expression: BoundExpressionId) -> Option<CheckedOperatorSelection> {
        entry(&self.operators, expression).copied()
    }

    /// Returns the resolved member target for an expression occurrence.
    pub fn member(&self, expression: BoundExpressionId) -> Option<&CheckedMemberSelection> {
        entry(&self.members, expression)
    }

    /// Returns the selected indexing contract for an expression occurrence.
    pub fn index(&self, expression: BoundExpressionId) -> Option<&CheckedIndexSelection> {
        entry(&self.indexes, expression)
    }

    /// Returns the resolved construction target and inputs for an expression occurrence.
    pub fn construction(&self, expression: BoundExpressionId) -> Option<&CheckedConstruction> {
        entry(&self.constructions, expression)
    }

    /// Returns the checked conversion plan for an expression occurrence.
    pub fn conversion(&self, expression: BoundExpressionId) -> Option<&CheckedConversion> {
        entry(&self.conversions, expression)
    }

    /// Returns whether any expression result was retained through recovery.
    pub fn is_recovered(&self) -> bool {
        self.results
            .iter()
            .any(|entry| entry.value().is_recovered())
    }
}

fn entry<T>(
    entries: &[CheckedExpressionFactEntry<T>],
    expression: BoundExpressionId,
) -> Option<&T> {
    entries
        .binary_search_by_key(&expression, CheckedExpressionFactEntry::expression)
        .ok()
        .map(|index| entries[index].value())
}
