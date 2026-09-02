use std::num::NonZeroU16;
use std::sync::Arc;

use bray_symbols::{ConstantValueId, SemanticValueStore};

use crate::{
    BoundExpression, BoundExpressionId, BoundUnit, BoundUnitId, BoundUnitKind,
    CheckedExpressionTypes,
};

/// One source literal and its value adapted to the final expression type.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedLiteralValueEntry {
    expression: BoundExpressionId,
    value: ConstantValueId,
}

impl CheckedLiteralValueEntry {
    /// Creates one literal-value association.
    pub const fn new(expression: BoundExpressionId, value: ConstantValueId) -> Self {
        Self { expression, value }
    }

    /// Returns the exact literal expression occurrence.
    pub const fn expression(self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the value adapted to the expression's final type.
    pub const fn value(self) -> ConstantValueId {
        self.value
    }
}

/// Complete immutable literal values for one checked bound unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedLiteralValues {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    target_integer_width_bits: NonZeroU16,
    entries: Arc<[CheckedLiteralValueEntry]>,
}

impl CheckedLiteralValues {
    /// Validates and creates the literal-value table for one bound unit.
    pub fn try_new(
        unit: &BoundUnit,
        types: &CheckedExpressionTypes,
        values: &SemanticValueStore,
        target_integer_width_bits: NonZeroU16,
        entries: impl IntoIterator<Item = CheckedLiteralValueEntry>,
    ) -> Result<Self, CheckedLiteralValueTableBuildError> {
        if types.unit() != unit.unit() || types.kind() != unit.key().kind() {
            return Err(CheckedLiteralValueTableBuildError::ForeignExpressionTypes);
        }

        let mut entries = entries.into_iter().collect::<Vec<_>>();

        for entry in &entries {
            validate_entry(unit, types, values, *entry)?;
        }

        entries.sort_unstable_by_key(|entry| entry.expression());

        if let Some(pair) = entries
            .windows(2)
            .find(|pair| pair[0].expression() == pair[1].expression())
        {
            return Err(CheckedLiteralValueTableBuildError::DuplicateExpression(
                pair[0].expression(),
            ));
        }

        for (expression, node) in unit.tree().expressions() {
            if !matches!(node, BoundExpression::Literal(_)) {
                continue;
            }

            if types.expression(expression).is_none() {
                return Err(CheckedLiteralValueTableBuildError::MissingExpressionType(
                    expression,
                ));
            }

            if entries
                .binary_search_by_key(&expression, |entry| entry.expression())
                .is_err()
            {
                return Err(CheckedLiteralValueTableBuildError::MissingLiteralValue(
                    expression,
                ));
            }
        }

        Ok(Self {
            unit: unit.unit(),
            kind: unit.key().kind(),
            target_integer_width_bits,
            entries: entries.into(),
        })
    }

    /// Returns the exact bound unit described by these values.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the semantic category of the checked bound unit.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns the target width used for machine-sized integer literals.
    pub const fn target_integer_width_bits(&self) -> NonZeroU16 {
        self.target_integer_width_bits
    }

    /// Returns entries in canonical expression-ID order.
    pub fn entries(&self) -> &[CheckedLiteralValueEntry] {
        &self.entries
    }

    /// Returns the adapted value for one literal expression occurrence.
    pub fn expression(&self, expression: BoundExpressionId) -> Option<ConstantValueId> {
        self.entries
            .binary_search_by_key(&expression, |entry| entry.expression())
            .ok()
            .map(|index| self.entries[index].value())
    }
}

/// A contract violation in a complete checked literal-value table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckedLiteralValueTableBuildError {
    /// The expression-type table belongs to another bound unit.
    ForeignExpressionTypes,
    /// An entry does not identify a literal in the owning unit.
    InvalidLiteral(BoundExpressionId),
    /// A literal expression does not have a final type.
    MissingExpressionType(BoundExpressionId),
    /// A literal expression has no adapted value.
    MissingLiteralValue(BoundExpressionId),
    /// The semantic value store rejected the literal's constant value identity.
    SemanticValue {
        /// The literal expression whose value could not be read.
        expression: BoundExpressionId,
        /// The exact semantic value store failure.
        error: bray_symbols::SemanticValueStoreError,
    },
    /// The adapted value and expression have different semantic types.
    ValueTypeMismatch(BoundExpressionId),
    /// More than one value was supplied for the same expression occurrence.
    DuplicateExpression(BoundExpressionId),
}

fn validate_entry(
    unit: &BoundUnit,
    types: &CheckedExpressionTypes,
    values: &SemanticValueStore,
    entry: CheckedLiteralValueEntry,
) -> Result<(), CheckedLiteralValueTableBuildError> {
    let expression = entry.expression();

    if !matches!(
        unit.view().expression(expression),
        Some(BoundExpression::Literal(_))
    ) {
        return Err(CheckedLiteralValueTableBuildError::InvalidLiteral(
            expression,
        ));
    }

    let Some(checked_type) = types.expression(expression) else {
        return Err(CheckedLiteralValueTableBuildError::MissingExpressionType(
            expression,
        ));
    };

    let value = values
        .constant_value_data(entry.value())
        .map_err(|error| CheckedLiteralValueTableBuildError::SemanticValue {
            expression,
            error,
        })?;

    if !checked_type.is_recovered() && value.ty() != checked_type.ty() {
        return Err(CheckedLiteralValueTableBuildError::ValueTypeMismatch(
            expression,
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use bray_symbols::{
        ConstantValueData, ConstantValueKind, SemanticValueStore, SemanticValueStoreError, TypeData,
    };

    use super::{
        CheckedLiteralValueEntry, CheckedLiteralValueTableBuildError, CheckedLiteralValues,
    };
    use crate::test_support::{expression_unit, push_expression, semantic_values};
    use crate::testing::checked_expression_types;
    use crate::{
        BoundExpression, BoundLiteralExpression, BoundLiteralKind, BoundUnitId,
        ExpressionTypeResult, ExpressionTypeStatus,
    };

    #[test]
    fn literal_value_tables_validate_completeness_and_canonicalize_order() {
        let values = semantic_values();

        let ty = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test type must intern: {error:?}"));

        let (unit, expressions) = expression_unit(BoundUnitId::new(80), |tree, origin| {
            (0..2)
                .map(|_| {
                    push_expression(
                        tree,
                        BoundExpression::Literal(BoundLiteralExpression::new(
                            origin,
                            origin.source_anchor().syntax().full_range(),
                            BoundLiteralKind::Boolean,
                            Some(ty),
                            false,
                        )),
                    )
                })
                .collect::<Vec<_>>()
        });

        let result = ExpressionTypeResult::new(ty, ExpressionTypeStatus::Valid);
        let types = checked_expression_types(&unit, expressions.iter().copied(), result);

        let first = values
            .intern_constant_value(ConstantValueData::new(ty, ConstantValueKind::Boolean(true)))
            .unwrap_or_else(|error| panic!("first value must intern: {error:?}"));

        let second = values
            .intern_constant_value(ConstantValueData::new(
                ty,
                ConstantValueKind::Boolean(false),
            ))
            .unwrap_or_else(|error| panic!("second value must intern: {error:?}"));

        let width = std::num::NonZeroU16::new(64).unwrap_or(std::num::NonZeroU16::MIN);

        let checked = CheckedLiteralValues::try_new(
            &unit,
            &types,
            &values,
            width,
            [
                CheckedLiteralValueEntry::new(expressions[1], second),
                CheckedLiteralValueEntry::new(expressions[0], first),
            ],
        )
        .unwrap_or_else(|error| panic!("complete literal values must validate: {error:?}"));

        assert_eq!(
            checked.entries(),
            [
                CheckedLiteralValueEntry::new(expressions[0], first),
                CheckedLiteralValueEntry::new(expressions[1], second),
            ]
        );

        assert_eq!(checked.target_integer_width_bits(), width);

        assert_eq!(
            CheckedLiteralValues::try_new(
                &unit,
                &types,
                &values,
                width,
                [CheckedLiteralValueEntry::new(expressions[0], first)],
            ),
            Err(CheckedLiteralValueTableBuildError::MissingLiteralValue(
                expressions[1]
            ))
        );
    }

    #[test]
    fn literal_values_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CheckedLiteralValueEntry>();
        assert_send_sync::<CheckedLiteralValues>();
    }

    #[test]
    fn literal_value_tables_preserve_foreign_store_identity() {
        let values = semantic_values();

        let ty = values
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("test type must intern: {error:?}"));

        let (unit, expressions) = expression_unit(BoundUnitId::new(81), |tree, origin| {
            vec![push_expression(
                tree,
                BoundExpression::Literal(BoundLiteralExpression::new(
                    origin,
                    origin.source_anchor().syntax().full_range(),
                    BoundLiteralKind::Boolean,
                    Some(ty),
                    false,
                )),
            )]
        });

        let expression = expressions[0];
        let result = ExpressionTypeResult::new(ty, ExpressionTypeStatus::Valid);
        let types = checked_expression_types(&unit, expressions, result);

        let foreign = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("foreign store must initialize: {error:?}"));

        let foreign_type = foreign
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("foreign type must intern: {error:?}"));

        let foreign_value = foreign
            .intern_constant_value(ConstantValueData::new(
                foreign_type,
                ConstantValueKind::Error,
            ))
            .unwrap_or_else(|error| panic!("foreign value must intern: {error:?}"));

        let width = std::num::NonZeroU16::new(64).unwrap_or(std::num::NonZeroU16::MIN);

        assert_eq!(
            CheckedLiteralValues::try_new(
                &unit,
                &types,
                &values,
                width,
                [CheckedLiteralValueEntry::new(expression, foreign_value)],
            ),
            Err(CheckedLiteralValueTableBuildError::SemanticValue {
                expression,
                error: SemanticValueStoreError::ForeignId {
                    expected: values.id(),
                    actual: foreign.id(),
                },
            })
        );
    }
}
