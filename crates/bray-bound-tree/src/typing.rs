use std::sync::Arc;

use bray_symbols::TypeId;

use crate::{BoundExpressionId, BoundUnitId, BoundUnitKind};

/// Whether expression typing completed normally or retained a recovery type.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExpressionTypeStatus {
    /// The expression has a valid canonical semantic type.
    Valid,
    /// An earlier or local type error required conservative recovery.
    Recovered,
}

impl ExpressionTypeStatus {
    /// Returns this expression-type status's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Recovered => "recovered",
        }
    }
}

/// The canonical type and validity state of one expression occurrence.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExpressionTypeResult {
    ty: TypeId,
    status: ExpressionTypeStatus,
}

impl ExpressionTypeResult {
    /// Creates one durable expression type result.
    pub const fn new(ty: TypeId, status: ExpressionTypeStatus) -> Self {
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
    /// Creates one source-correlated expression type entry.
    pub const fn new(expression: BoundExpressionId, result: ExpressionTypeResult) -> Self {
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

/// Complete immutable expression types for one bound semantic unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedExpressionTypes {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    entries: Arc<[ExpressionTypeEntry]>,
}

impl CheckedExpressionTypes {
    /// Creates a complete expression type table in expression ID order.
    pub fn new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        entries: impl IntoIterator<Item = ExpressionTypeEntry>,
    ) -> Self {
        let mut entries = entries.into_iter().collect::<Vec<_>>();

        entries.sort_unstable_by_key(|entry| entry.expression());

        Self {
            unit,
            kind,
            entries: entries.into(),
        }
    }

    /// Returns the exact bound unit described by these types.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the semantic category of the checked bound unit.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns entries in canonical bound-expression ID order.
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
    use super::{
        CheckedExpressionTypes, ExpressionTypeEntry, ExpressionTypeResult, ExpressionTypeStatus,
    };
    use crate::test_support::error_type;
    use crate::{BoundExpressionId, BoundUnitId, BoundUnitKind};

    #[test]
    fn expression_type_facts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ExpressionTypeResult>();
        assert_send_sync::<ExpressionTypeEntry>();
        assert_send_sync::<CheckedExpressionTypes>();
    }

    #[test]
    fn expression_type_tables_canonicalize_entry_order() {
        let unit = BoundUnitId::new(1);
        let first = BoundExpressionId::from_slot(unit, 0);
        let second = BoundExpressionId::from_slot(unit, 1);
        let result = ExpressionTypeResult::new(error_type(), ExpressionTypeStatus::Recovered);

        let types = CheckedExpressionTypes::new(
            unit,
            BoundUnitKind::CallableBody,
            [
                ExpressionTypeEntry::new(second, result),
                ExpressionTypeEntry::new(first, result),
            ],
        );

        assert_eq!(
            types.entries(),
            [
                ExpressionTypeEntry::new(first, result),
                ExpressionTypeEntry::new(second, result),
            ]
        );
    }
}
