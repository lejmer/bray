use std::sync::Arc;

use bray_bound_tree::{BoundExpressionId, BoundUnitId, BoundUnitKind};
use bray_symbols::TypeId;

/// Whether expression typing completed normally or retained a recovery type.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExpressionTypeStatus {
    /// The expression has a valid canonical semantic type.
    Valid,
    /// An earlier or local type error required conservative recovery.
    Recovered,
}

/// The canonical type and validity state of one expression occurrence.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExpressionTypeResult {
    ty: TypeId,
    status: ExpressionTypeStatus,
}

impl ExpressionTypeResult {
    pub(crate) const fn new(ty: TypeId, status: ExpressionTypeStatus) -> Self {
        Self { ty, status }
    }

    /// Returns the canonical checked or recovery type.
    pub const fn ty(self) -> TypeId {
        self.ty
    }

    /// Returns whether checking established a valid type or recovered.
    pub const fn status(self) -> ExpressionTypeStatus {
        self.status
    }

    /// Returns whether this result was retained through recovery.
    pub const fn is_recovered(self) -> bool {
        matches!(self.status, ExpressionTypeStatus::Recovered)
    }
}

/// One source-correlated expression and its durable type result.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExpressionTypeEntry {
    expression: BoundExpressionId,
    result: ExpressionTypeResult,
}

impl ExpressionTypeEntry {
    pub(crate) const fn new(expression: BoundExpressionId, result: ExpressionTypeResult) -> Self {
        Self { expression, result }
    }

    /// Returns the exact expression occurrence.
    pub const fn expression(self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the expression's canonical type result.
    pub const fn result(self) -> ExpressionTypeResult {
        self.result
    }
}

/// Complete immutable expression-type results for one bound semantic unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpressionTypeCheckResult {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    entries: Arc<[ExpressionTypeEntry]>,
}

impl ExpressionTypeCheckResult {
    pub(crate) fn new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        entries: impl IntoIterator<Item = ExpressionTypeEntry>,
    ) -> Self {
        Self {
            unit,
            kind,
            entries: entries.into_iter().collect(),
        }
    }

    /// Returns the exact bound unit described by these results.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the semantic category of the checked bound unit.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns results in canonical bound-expression ID order.
    pub fn entries(&self) -> &[ExpressionTypeEntry] {
        &self.entries
    }

    /// Returns the type result for one expression occurrence.
    pub fn expression(&self, expression: BoundExpressionId) -> Option<ExpressionTypeResult> {
        self.entries
            .binary_search_by_key(&expression, |entry| entry.expression())
            .ok()
            .map(|index| self.entries[index].result())
    }

    /// Returns whether any expression required type recovery.
    pub fn is_recovered(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.result().is_recovered())
    }
}

#[cfg(test)]
mod tests {
    use super::{ExpressionTypeCheckResult, ExpressionTypeEntry, ExpressionTypeResult};

    #[test]
    fn expression_type_results_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ExpressionTypeResult>();
        assert_send_sync::<ExpressionTypeEntry>();
        assert_send_sync::<ExpressionTypeCheckResult>();
    }
}
