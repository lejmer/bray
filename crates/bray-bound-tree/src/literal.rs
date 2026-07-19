use std::num::NonZeroU16;
use std::sync::Arc;

use bray_symbols::{ConstantValueId, SemanticValueStore};

use crate::{
    BoundExpression, BoundExpressionId, BoundUnit, BoundUnitId, BoundUnitKind,
    CheckedExpressionTypes,
};

/// One source literal and its canonical adapted value.
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

    /// Returns the canonical value adapted to the expression's checked type.
    pub const fn value(self) -> ConstantValueId {
        self.value
    }
}

/// Complete canonical literal values for one checked bound unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedLiteralValues {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    target_integer_width_bits: Option<NonZeroU16>,
    entries: Arc<[CheckedLiteralValueEntry]>,
}

impl CheckedLiteralValues {
    /// Validates and creates the literal-value table for one bound unit.
    pub fn try_new(
        unit: &BoundUnit,
        types: &CheckedExpressionTypes,
        values: &SemanticValueStore,
        target_integer_width_bits: Option<NonZeroU16>,
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

        for expression in types.entries().iter().map(|entry| entry.expression()) {
            if matches!(
                unit.view().expression(expression),
                Some(BoundExpression::Literal(_))
            ) && entries
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

    /// Returns the target width used to validate machine-sized integer literals.
    ///
    /// `None` identifies a portable checking fact that cannot be supplied to lowering.
    pub const fn target_integer_width_bits(&self) -> Option<NonZeroU16> {
        self.target_integer_width_bits
    }

    /// Returns entries in canonical expression-ID order.
    pub fn entries(&self) -> &[CheckedLiteralValueEntry] {
        &self.entries
    }

    /// Returns the canonical value for one literal expression occurrence.
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
    /// An expression does not have a checked type result.
    MissingExpressionType(BoundExpressionId),
    /// A literal expression has no adapted canonical value.
    MissingLiteralValue(BoundExpressionId),
    /// A constant value does not belong to the supplied semantic value store.
    InvalidValue(BoundExpressionId),
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
        .map_err(|_| CheckedLiteralValueTableBuildError::InvalidValue(expression))?;

    if !checked_type.is_recovered() && value.ty() != checked_type.ty() {
        return Err(CheckedLiteralValueTableBuildError::ValueTypeMismatch(
            expression,
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use bray_symbols::{ConstantValueData, ConstantValueKind, TypeData};

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
    fn literal_value_tables_validate_types_and_canonicalize_expression_order() {
        let values = semantic_values();
        let ty = intern_type(&values, TypeData::tuple([]));
        let other = intern_type(&values, TypeData::tuple([ty]));
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
        let first = intern_value(&values, ty, true);
        let second = intern_value(&values, ty, false);

        let table = match CheckedLiteralValues::try_new(
            &unit,
            &types,
            &values,
            None,
            [
                CheckedLiteralValueEntry::new(expressions[1], second),
                CheckedLiteralValueEntry::new(expressions[0], first),
            ],
        ) {
            Ok(table) => table,
            Err(error) => panic!("valid literal values must publish: {error:?}"),
        };

        assert_eq!(table.expression(expressions[0]), Some(first));
        assert_eq!(table.entries()[0].expression(), expressions[0]);

        assert_eq!(
            CheckedLiteralValues::try_new(
                &unit,
                &types,
                &values,
                None,
                [CheckedLiteralValueEntry::new(expressions[0], first)],
            ),
            Err(CheckedLiteralValueTableBuildError::MissingLiteralValue(
                expressions[1]
            ))
        );

        let mismatched = intern_value(&values, other, true);

        assert_eq!(
            CheckedLiteralValues::try_new(
                &unit,
                &types,
                &values,
                None,
                [CheckedLiteralValueEntry::new(expressions[0], mismatched)],
            ),
            Err(CheckedLiteralValueTableBuildError::ValueTypeMismatch(
                expressions[0]
            ))
        );
    }

    fn intern_type(
        values: &bray_symbols::SemanticValueStore,
        data: TypeData,
    ) -> bray_symbols::TypeId {
        match values.intern_type(data) {
            Ok(ty) => ty,
            Err(error) => panic!("test type must be valid: {error:?}"),
        }
    }

    fn intern_value(
        values: &bray_symbols::SemanticValueStore,
        ty: bray_symbols::TypeId,
        value: bool,
    ) -> bray_symbols::ConstantValueId {
        match values.intern_constant_value(ConstantValueData::new(
            ty,
            ConstantValueKind::Boolean(value),
        )) {
            Ok(value) => value,
            Err(error) => panic!("test value must be valid: {error:?}"),
        }
    }
}
