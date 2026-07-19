use std::sync::Arc;

use bray_bound_tree::{BoundExpressionId, BoundUnitId, BoundUnitKind};

use super::{SelectedCall, SelectedOperation};

/// The exact checked semantic choice attached to one expression occurrence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SemanticSelection {
    /// An exact callable, ABI, witness set, and argument mapping.
    Call(SelectedCall),
    /// An exact member, operator, index, construction, conversion, or implementation operation.
    Operation(SelectedOperation),
}

/// One source-correlated expression and its exact semantic selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticSelectionEntry {
    expression: BoundExpressionId,
    selection: SemanticSelection,
}

impl SemanticSelectionEntry {
    /// Creates one expression selection entry.
    pub const fn new(expression: BoundExpressionId, selection: SemanticSelection) -> Self {
        Self {
            expression,
            selection,
        }
    }

    /// Returns the exact expression occurrence.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the selected semantic operation.
    pub const fn selection(&self) -> &SemanticSelection {
        &self.selection
    }
}

/// A malformed immutable semantic-selection table.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SemanticSelectionTableBuildError {
    /// A selection belongs to another bound unit.
    ForeignExpression(BoundExpressionId),
    /// More than one selection was supplied for one expression occurrence.
    DuplicateExpression(BoundExpressionId),
}

/// Complete immutable semantic selections for one bound unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedSemanticSelections {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    entries: Arc<[SemanticSelectionEntry]>,
}

impl CheckedSemanticSelections {
    /// Creates one table in canonical expression-ID order.
    pub fn try_new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        entries: impl IntoIterator<Item = SemanticSelectionEntry>,
    ) -> Result<Self, SemanticSelectionTableBuildError> {
        let mut entries = entries.into_iter().collect::<Vec<_>>();

        if let Some(entry) = entries
            .iter()
            .find(|entry| entry.expression().unit() != unit)
        {
            return Err(SemanticSelectionTableBuildError::ForeignExpression(
                entry.expression(),
            ));
        }

        entries.sort_unstable_by_key(SemanticSelectionEntry::expression);

        if let Some(pair) = entries
            .windows(2)
            .find(|pair| pair[0].expression() == pair[1].expression())
        {
            return Err(SemanticSelectionTableBuildError::DuplicateExpression(
                pair[0].expression(),
            ));
        }

        Ok(Self {
            unit,
            kind,
            entries: entries.into(),
        })
    }

    /// Returns the exact bound unit described by these selections.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the semantic category of the selected bound unit.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns selections in canonical expression-ID order.
    pub fn entries(&self) -> &[SemanticSelectionEntry] {
        &self.entries
    }

    /// Returns the exact semantic selection for one expression occurrence.
    pub fn expression(&self, expression: BoundExpressionId) -> Option<&SemanticSelection> {
        self.entries
            .binary_search_by_key(&expression, SemanticSelectionEntry::expression)
            .ok()
            .and_then(|index| self.entries.get(index))
            .map(SemanticSelectionEntry::selection)
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundOperator, BoundUnitId};

    use super::{
        CheckedSemanticSelections, SemanticSelection, SemanticSelectionEntry,
        SemanticSelectionTableBuildError,
    };
    use crate::test_support::{
        expression_unit, integer_literal_expression, push_expression, tuple_type,
    };
    use crate::{OperatorTarget, SelectedOperation};

    #[test]
    fn selection_tables_canonicalize_order_and_reject_duplicate_expressions() {
        let unit = BoundUnitId::new(90);
        let ty = tuple_type([]);

        let (bound_unit, expressions) = expression_unit(unit, |tree, origin| {
            vec![
                push_expression(tree, integer_literal_expression(origin, Some(ty))),
                push_expression(tree, integer_literal_expression(origin, Some(ty))),
            ]
        });

        let [first, second] = expressions.as_slice() else {
            panic!("selection table fixture must contain two expressions");
        };

        let (first, second) = (*first, *second);

        let selection = || {
            SemanticSelection::Operation(SelectedOperation::Operator {
                target: OperatorTarget::BuiltIn(BoundOperator::Add),
                result_type: ty,
            })
        };

        let table = match CheckedSemanticSelections::try_new(
            unit,
            bound_unit.key().kind(),
            [
                SemanticSelectionEntry::new(second, selection()),
                SemanticSelectionEntry::new(first, selection()),
            ],
        ) {
            Ok(table) => table,
            Err(error) => panic!("distinct selection entries must be valid: {error:?}"),
        };

        assert_eq!(table.entries()[0].expression(), first);
        assert_eq!(table.entries()[1].expression(), second);
        assert_eq!(table.expression(first), Some(&selection()));

        assert!(matches!(
            CheckedSemanticSelections::try_new(
                unit,
                bound_unit.key().kind(),
                [
                    SemanticSelectionEntry::new(first, selection()),
                    SemanticSelectionEntry::new(first, selection()),
                ],
            ),
            Err(SemanticSelectionTableBuildError::DuplicateExpression(expression))
                if expression == first
        ));

        let (_, foreign_expressions) = expression_unit(BoundUnitId::new(91), |tree, origin| {
            vec![push_expression(
                tree,
                integer_literal_expression(origin, Some(ty)),
            )]
        });

        let foreign = foreign_expressions[0];

        assert_eq!(
            CheckedSemanticSelections::try_new(
                unit,
                bound_unit.key().kind(),
                [SemanticSelectionEntry::new(foreign, selection())],
            ),
            Err(SemanticSelectionTableBuildError::ForeignExpression(foreign))
        );
    }
}
