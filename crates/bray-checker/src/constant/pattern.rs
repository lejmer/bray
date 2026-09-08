use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundPatternId, BoundPatternKind, BoundUnitView, CheckedPatterns, PatternPredicate,
    PatternProjection,
};
use bray_symbols::{
    ConstantBinaryOperation, ConstantProjection, ConstantProjectionKind, ConstantTermData,
    ConstantTermId, ConstantTest, ConstantUnaryOperation, ConstantValueData, ConstantValueKind,
    SemanticValueStore, SemanticValueStoreError, TypeData, TypeId,
};

pub(crate) enum PatternTermError<E> {
    Step(E),
    Semantic(SemanticValueStoreError),
    MissingPattern(BoundPatternId),
    MissingCheckedPattern(BoundPatternId),
    Unsupported,
}

impl<E> From<SemanticValueStoreError> for PatternTermError<E> {
    fn from(error: SemanticValueStoreError) -> Self {
        Self::Semantic(error)
    }
}

/// Builds structural pattern conditions with the same meaning in constant and runtime proofs.
pub(crate) struct PatternTerms<'a> {
    pub(crate) view: BoundUnitView<'a>,
    pub(crate) patterns: &'a CheckedPatterns,
    pub(crate) values: &'a SemanticValueStore,
    pub(crate) boolean: TypeId,
    pub(crate) retain_types: bool,
}

impl PatternTerms<'_> {
    pub(crate) fn test<E>(
        &self,
        pattern: BoundPatternId,
        subject: ConstantTermId,
        mut step: impl FnMut() -> Result<(), E>,
    ) -> Result<ConstantTermId, PatternTermError<E>> {
        let mut pending = vec![(pattern, subject, false)];
        let mut results = BTreeMap::new();

        while let Some((id, subject, expanded)) = pending.pop() {
            step().map_err(PatternTermError::Step)?;

            let node = self
                .view
                .pattern(id)
                .ok_or(PatternTermError::MissingPattern(id))?;

            let checked = self
                .patterns
                .pattern(id)
                .ok_or(PatternTermError::MissingCheckedPattern(id))?;

            if checked.is_recovered() {
                return Err(PatternTermError::Unsupported);
            }

            let alternative = node.kind() == BoundPatternKind::Alternative;

            if !expanded {
                pending.push((id, subject, true));

                for child in node.children().iter().rev() {
                    let child_checked = self
                        .patterns
                        .pattern(*child)
                        .ok_or(PatternTermError::MissingCheckedPattern(*child))?;

                    let projected = if alternative {
                        subject
                    } else {
                        self.project(subject, checked.input_type(), child_checked.projection())?
                    };

                    pending.push((*child, projected, false));
                }

                continue;
            }

            let mut result = if alternative {
                self.boolean(false)?
            } else {
                self.predicate(subject, checked.input_type(), checked.test())?
            };

            let operation = if alternative {
                ConstantBinaryOperation::LogicalOr
            } else {
                ConstantBinaryOperation::LogicalAnd
            };

            for child in node.children() {
                let right = results
                    .get(child)
                    .copied()
                    .ok_or(PatternTermError::MissingPattern(*child))?;

                result = self.typed(ConstantTermData::Binary {
                    operation,
                    left: result,
                    right,
                })?;
            }

            results.insert(id, result);
        }

        results
            .get(&pattern)
            .copied()
            .ok_or(PatternTermError::MissingPattern(pattern))
    }

    fn predicate<E>(
        &self,
        subject: ConstantTermId,
        subject_type: TypeId,
        predicate: Option<PatternPredicate>,
    ) -> Result<ConstantTermId, PatternTermError<E>> {
        let data = match predicate {
            None | Some(PatternPredicate::ProductShape(_) | PatternPredicate::TupleShape(_)) => {
                return self.boolean(true);
            }
            Some(PatternPredicate::ArrayShape(count)) => {
                let data = self.values.type_data(subject_type)?;

                let TypeData::Array { length, .. } = data.as_ref() else {
                    return Err(PatternTermError::Unsupported);
                };

                ConstantTermData::Binary {
                    operation: ConstantBinaryOperation::Equal,
                    left: *length,
                    right: self.index(u64::from(count))?,
                }
            }
            Some(PatternPredicate::NullablePresent | PatternPredicate::NullableAbsent) => {
                let test = ConstantTermData::Test {
                    subject,
                    kind: ConstantTest::NullablePresent,
                };

                if predicate == Some(PatternPredicate::NullableAbsent) {
                    ConstantTermData::Unary {
                        operation: ConstantUnaryOperation::LogicalNot,
                        operand: self.typed(test)?,
                    }
                } else {
                    test
                }
            }
            Some(PatternPredicate::ActiveUnionVariant(variant)) => ConstantTermData::Test {
                subject,
                kind: ConstantTest::ActiveUnionVariant(variant),
            },
            Some(PatternPredicate::Literal(literal)) => ConstantTermData::Binary {
                operation: ConstantBinaryOperation::Equal,
                left: subject,
                right: self.term(ConstantTermData::Value(literal.value()))?,
            },
            Some(PatternPredicate::Constant(right)) => ConstantTermData::Binary {
                operation: ConstantBinaryOperation::Equal,
                left: subject,
                right,
            },
            Some(PatternPredicate::OwnedTarget) => return self.boolean(true),
        };

        self.typed(data)
    }

    fn project<E>(
        &self,
        subject: ConstantTermId,
        subject_type: TypeId,
        projection: Option<PatternProjection>,
    ) -> Result<ConstantTermId, PatternTermError<E>> {
        let kind = match projection {
            None => return Ok(subject),
            Some(PatternProjection::ProductField(field)) => {
                ConstantProjectionKind::ProductField(field)
            }
            Some(PatternProjection::TupleElement(ordinal)) => {
                ConstantProjectionKind::TupleElement(ordinal)
            }
            Some(PatternProjection::ActiveUnionPayloadField { field, .. }) => {
                ConstantProjectionKind::UnionPayloadField(field)
            }
            Some(PatternProjection::NullableValue) => ConstantProjectionKind::NullableValue,
            Some(PatternProjection::ElementFromStart(ordinal)) => {
                ConstantProjectionKind::ArrayElement(self.index(u64::from(ordinal.raw()))?)
            }
            Some(PatternProjection::ElementFromEnd(ordinal)) => {
                let data = self.values.type_data(subject_type)?;

                let TypeData::Array { length, .. } = data.as_ref() else {
                    return Err(PatternTermError::Unsupported);
                };

                let right = self.index(u64::from(ordinal.raw()) + 1)?;

                let index = self.term(ConstantTermData::Binary {
                    operation: ConstantBinaryOperation::Subtract,
                    left: *length,
                    right,
                })?;

                ConstantProjectionKind::ArrayElement(index)
            }
            Some(PatternProjection::OwnedTarget) => ConstantProjectionKind::OwnedTarget,
        };

        self.term(ConstantTermData::Projection(ConstantProjection::new(
            subject, kind,
        )))
    }

    fn index<E>(&self, value: u64) -> Result<ConstantTermId, PatternTermError<E>> {
        super::shape::array_index_observation(self.values, value).map_err(Into::into)
    }

    fn boolean<E>(&self, value: bool) -> Result<ConstantTermId, PatternTermError<E>> {
        let value = self.values.intern_constant_value(ConstantValueData::new(
            self.boolean,
            ConstantValueKind::Boolean(value),
        ))?;

        self.term(ConstantTermData::Value(value))
    }

    fn typed<E>(&self, data: ConstantTermData) -> Result<ConstantTermId, PatternTermError<E>> {
        let term = self.term(data)?;

        if self.retain_types {
            self.term(ConstantTermData::typed(term, self.boolean))
        } else {
            Ok(term)
        }
    }

    fn term<E>(&self, data: ConstantTermData) -> Result<ConstantTermId, PatternTermError<E>> {
        self.values
            .intern_constant_term(data)
            .map_err(PatternTermError::Semantic)
    }
}
