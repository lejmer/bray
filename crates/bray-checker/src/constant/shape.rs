use bray_symbols::{
    ConstantProjectionKind, ConstantTermData, ConstantTermId, ConstantTest, ConstantValueId,
    ConstantValueKind, SemanticValueStore, SemanticValueStoreError,
};

/// Builds the structural term shared by constant evaluation and runtime value observations.
pub(crate) fn constructed_term(
    target: bray_bound_tree::ConstructionTarget,
    inputs: impl IntoIterator<Item = (bray_bound_tree::ConstructionInputId, ConstantTermId)>,
) -> Option<ConstantTermData> {
    use bray_bound_tree::{ConstructionInputId, ConstructionTarget};
    use bray_symbols::ConstantField;

    match target {
        ConstructionTarget::Struct(_) => inputs
            .into_iter()
            .map(|(input, value)| match input {
                ConstructionInputId::StructField(field) => Some(ConstantField::new(field, value)),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .map(ConstantTermData::product),
        ConstructionTarget::UnionVariant(variant) => inputs
            .into_iter()
            .map(|(input, value)| match input {
                ConstructionInputId::UnionPayloadField(field) => {
                    Some(ConstantField::new(field, value))
                }
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .map(|fields| ConstantTermData::union(variant, fields)),
        ConstructionTarget::TypeForm { .. } => None,
    }
}

pub(super) fn project_value(
    subject: &ConstantValueKind,
    projection: ConstantProjectionKind,
    index: Option<usize>,
) -> Option<ConstantValueId> {
    match (subject, projection) {
        (ConstantValueKind::Tuple(elements), ConstantProjectionKind::TupleElement(ordinal)) => {
            ordinal
                .to_index()
                .and_then(|index| elements.get(index))
                .copied()
        }
        (ConstantValueKind::Array(elements), ConstantProjectionKind::ArrayElement(_)) => {
            index.and_then(|index| elements.get(index)).copied()
        }
        (ConstantValueKind::Product(fields), ConstantProjectionKind::ProductField(field)) => fields
            .iter()
            .find(|entry| *entry.field() == field)
            .map(|entry| *entry.value()),
        (
            ConstantValueKind::Union { fields, .. },
            ConstantProjectionKind::UnionPayloadField(field),
        ) => fields
            .iter()
            .find(|entry| *entry.field() == field)
            .map(|entry| *entry.value()),
        (ConstantValueKind::NullablePresent(value), ConstantProjectionKind::NullableValue) => {
            Some(*value)
        }
        _ => None,
    }
}

pub(crate) fn test_value_shape(kind: ConstantTest, value: &ConstantValueKind) -> Option<bool> {
    match (kind, value) {
        (ConstantTest::NullablePresent, ConstantValueKind::NullableAbsent) => Some(false),
        (ConstantTest::NullablePresent, ConstantValueKind::NullablePresent(_)) => Some(true),
        (ConstantTest::ActiveUnionVariant(expected), ConstantValueKind::Union { variant, .. }) => {
            Some(expected == *variant)
        }
        _ => None,
    }
}

pub(crate) fn test_term_shape(
    values: &SemanticValueStore,
    subject: ConstantTermId,
    kind: ConstantTest,
    remaining: &mut usize,
) -> Result<Option<bool>, SemanticValueStoreError> {
    let Some(subject) = observation_subject(values, subject, remaining)? else {
        return Ok(None);
    };

    let data = values.constant_term_data(subject)?;

    Ok(match data.as_ref() {
        ConstantTermData::Value(value) => {
            test_value_shape(kind, values.constant_value_data(*value)?.kind())
        }
        ConstantTermData::NullablePresent(_) if kind == ConstantTest::NullablePresent => Some(true),
        ConstantTermData::Union { variant, .. } => match kind {
            ConstantTest::ActiveUnionVariant(expected) => Some(expected == *variant),
            ConstantTest::NullablePresent => None,
        },
        _ => None,
    })
}

/// Returns the observed value beneath checked type annotations, within the caller's work budget.
pub(crate) fn observation_subject(
    values: &SemanticValueStore,
    mut subject: ConstantTermId,
    remaining: &mut usize,
) -> Result<Option<ConstantTermId>, SemanticValueStoreError> {
    let mut projections = Vec::new();

    loop {
        let Some(next) = remaining.checked_sub(1) else {
            return Ok(None);
        };

        *remaining = next;

        let data = values.constant_term_data(subject)?;

        match data.as_ref() {
            ConstantTermData::Typed { term, .. } => subject = *term,
            ConstantTermData::Projection(projection) => {
                projections.push(projection.kind());
                subject = projection.subject();
            }
            _ => break,
        }
    }

    for kind in projections.into_iter().rev() {
        let Some(next) = remaining.checked_sub(1) else {
            return Ok(None);
        };

        *remaining = next;

        subject = match project_observation(values, subject, kind, remaining)? {
            Some(projected) => projected,
            None => return Ok(None),
        };
    }

    while let ConstantTermData::Typed { term, .. } = values.constant_term_data(subject)?.as_ref() {
        let Some(next) = remaining.checked_sub(1) else {
            return Ok(None);
        };

        *remaining = next;
        subject = *term;
    }

    Ok(Some(subject))
}

/// Identifies a storage path rooted in an input or local, excluding computed values.
pub(crate) fn storage_observation_subject(
    values: &SemanticValueStore,
    term: ConstantTermId,
    remaining: &mut usize,
) -> Result<Option<ConstantTermId>, SemanticValueStoreError> {
    let Some(subject) = observation_subject(values, term, remaining)? else {
        return Ok(None);
    };

    let mut root = subject;

    loop {
        let Some(next) = remaining.checked_sub(1) else {
            return Ok(None);
        };

        *remaining = next;

        match values.constant_term_data(root)?.as_ref() {
            ConstantTermData::CallableArgument(_) => return Ok(Some(subject)),
            ConstantTermData::Projection(projection) => root = projection.subject(),
            _ => return Ok(None),
        }
    }
}

/// Names a fixed observed storage path without evaluating dynamic selectors.
pub(crate) fn storage_path_observation(
    values: &SemanticValueStore,
    mut subject: ConstantTermId,
    projections: &[bray_bound_tree::StorageProjection],
    owned_projections_verified: bool,
) -> Result<Option<ConstantTermId>, SemanticValueStoreError> {
    use bray_bound_tree::StorageProjection;

    for projection in projections {
        let kind = match *projection {
            StorageProjection::ProductField(field) => ConstantProjectionKind::ProductField(field),
            StorageProjection::TupleElement(ordinal) => {
                ConstantProjectionKind::TupleElement(ordinal)
            }
            StorageProjection::ActiveUnionPayloadField { field, .. } => {
                ConstantProjectionKind::UnionPayloadField(field)
            }
            StorageProjection::NullableValue => ConstantProjectionKind::NullableValue,
            StorageProjection::OwnedTarget if owned_projections_verified => {
                ConstantProjectionKind::OwnedTarget
            }
            StorageProjection::ElementFromStart(ordinal) => {
                let index = array_index_observation(values, u64::from(ordinal.raw()))?;

                ConstantProjectionKind::ArrayElement(index)
            }
            _ => return Ok(None),
        };

        subject = values.intern_constant_term(ConstantTermData::Projection(
            bray_symbols::ConstantProjection::new(subject, kind),
        ))?;
    }

    Ok(Some(subject))
}

pub(crate) fn array_index_observation(
    values: &SemanticValueStore,
    index: u64,
) -> Result<ConstantTermId, SemanticValueStoreError> {
    values.intern_constant_term(ConstantTermData::IntegerLiteral {
        ty: bray_symbols::TargetSizedIntegerType::Usize,
        value: bray_symbols::IntegerConstant::from_u64(index),
    })
}

fn project_observation(
    values: &SemanticValueStore,
    mut subject: ConstantTermId,
    kind: ConstantProjectionKind,
    remaining: &mut usize,
) -> Result<Option<ConstantTermId>, SemanticValueStoreError> {
    let index = match kind {
        ConstantProjectionKind::ArrayElement(index) => observation_index(values, index, remaining)?,
        _ => None,
    };

    loop {
        let Some(next) = remaining.checked_sub(1) else {
            return Ok(None);
        };

        *remaining = next;

        let data = values.constant_term_data(subject)?;

        let projected = match (data.as_ref(), kind) {
            (ConstantTermData::Typed { term, .. }, _) => {
                subject = *term;

                continue;
            }
            (ConstantTermData::Value(value), _) => {
                project_value(values.constant_value_data(*value)?.kind(), kind, index)
                    .map(|value| values.intern_constant_term(ConstantTermData::Value(value)))
                    .transpose()?
            }
            (ConstantTermData::Tuple(elements), ConstantProjectionKind::TupleElement(ordinal)) => {
                ordinal
                    .to_index()
                    .and_then(|index| elements.get(index))
                    .copied()
            }
            (ConstantTermData::Array(elements), ConstantProjectionKind::ArrayElement(_)) => {
                index.and_then(|index| elements.get(index)).copied()
            }
            (ConstantTermData::Product(fields), ConstantProjectionKind::ProductField(field)) => {
                fields
                    .iter()
                    .find(|entry| *entry.field() == field)
                    .map(|entry| *entry.value())
            }
            (
                ConstantTermData::Union { fields, .. },
                ConstantProjectionKind::UnionPayloadField(field),
            ) => fields
                .iter()
                .find(|entry| *entry.field() == field)
                .map(|entry| *entry.value()),
            (ConstantTermData::NullablePresent(value), ConstantProjectionKind::NullableValue) => {
                Some(*value)
            }
            _ => None,
        };

        return match projected {
            Some(term) => Ok(Some(term)),
            None => values
                .intern_constant_term(ConstantTermData::Projection(
                    bray_symbols::ConstantProjection::new(subject, kind),
                ))
                .map(Some),
        };
    }
}

pub(crate) fn observation_index(
    values: &SemanticValueStore,
    mut term: ConstantTermId,
    remaining: &mut usize,
) -> Result<Option<usize>, SemanticValueStoreError> {
    loop {
        let Some(next) = remaining.checked_sub(1) else {
            return Ok(None);
        };

        *remaining = next;

        match values.constant_term_data(term)?.as_ref() {
            ConstantTermData::Typed { term: inner, .. } => term = *inner,
            ConstantTermData::IntegerLiteral { value, .. } => {
                return Ok(super::integer_to_usize(value));
            }
            ConstantTermData::Value(value) => {
                return Ok(match values.constant_value_data(*value)?.kind() {
                    ConstantValueKind::Integer(value) => super::integer_to_usize(value),
                    _ => None,
                });
            }
            _ => return Ok(None),
        }
    }
}

pub(crate) fn observation_boolean(
    values: &SemanticValueStore,
    term: ConstantTermId,
    remaining: &mut usize,
) -> Result<Option<bool>, SemanticValueStoreError> {
    let Some(term) = observation_subject(values, term, remaining)? else {
        return Ok(None);
    };

    let data = values.constant_term_data(term)?;

    let ConstantTermData::Value(value) = data.as_ref() else {
        return Ok(None);
    };

    Ok(match values.constant_value_data(*value)?.kind() {
        ConstantValueKind::Boolean(value) => Some(*value),
        _ => None,
    })
}

/// Compares closed scalar observations using the language's constant-operation semantics.
pub(crate) fn literal_observations_equal(
    values: &SemanticValueStore,
    left: ConstantTermId,
    right: ConstantTermId,
    remaining: &mut usize,
) -> Result<Option<bool>, SemanticValueStoreError> {
    let (Some(left), Some(right)) = (
        observation_subject(values, left, remaining)?,
        observation_subject(values, right, remaining)?,
    ) else {
        return Ok(None);
    };

    let (left, right) = (
        values.constant_term_data(left)?,
        values.constant_term_data(right)?,
    );

    let (ConstantTermData::Value(left), ConstantTermData::Value(right)) =
        (left.as_ref(), right.as_ref())
    else {
        return Ok(None);
    };

    let (left, right) = (
        values.constant_value_data(*left)?,
        values.constant_value_data(*right)?,
    );

    let cost = scalar_comparison_cost(left.kind())
        .and_then(|left| left.checked_add(scalar_comparison_cost(right.kind())?));

    let Some(next) = cost.and_then(|cost| remaining.checked_sub(cost)) else {
        return Ok(None);
    };

    if left.ty() != right.ty() {
        return Ok(None);
    }

    *remaining = next;

    // Undefined comparisons provide no proof evidence.
    Ok(
        match super::operation::fold_binary(
            bray_bound_tree::BoundOperator::Equal,
            left.kind(),
            right.kind(),
            u32::MAX,
        ) {
            Ok(ConstantValueKind::Boolean(equal)) => Some(equal),
            _ => None,
        },
    )
}

fn scalar_comparison_cost(kind: &ConstantValueKind) -> Option<usize> {
    match kind {
        ConstantValueKind::Integer(value) => value.magnitude().len().checked_add(1),
        ConstantValueKind::String(value) => value.len().checked_add(1),
        ConstantValueKind::Boolean(_)
        | ConstantValueKind::Character(_)
        | ConstantValueKind::Real(_)
        | ConstantValueKind::Complex { .. } => Some(1),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::test_term_shape;
    use bray_symbols::{ConstantTermData, ConstantTest, SemanticValueStore, SymbolOrdinal};

    #[test]
    fn projection_observations_preserve_paths_and_ignore_receiver_type_annotations() {
        let values = SemanticValueStore::try_new().unwrap();

        let ty = values
            .intern_type(bray_symbols::TypeData::tuple([]))
            .unwrap();

        let root = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let typed = values
            .intern_constant_term(ConstantTermData::Typed { term: root, ty })
            .unwrap();

        let kind = bray_symbols::ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));

        let plain = values
            .intern_constant_term(ConstantTermData::Projection(
                bray_symbols::ConstantProjection::new(root, kind),
            ))
            .unwrap();

        let annotated = values
            .intern_constant_term(ConstantTermData::Projection(
                bray_symbols::ConstantProjection::new(typed, kind),
            ))
            .unwrap();

        let sibling = values
            .intern_constant_term(ConstantTermData::Projection(
                bray_symbols::ConstantProjection::new(
                    typed,
                    bray_symbols::ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)),
                ),
            ))
            .unwrap();

        assert_eq!(
            super::observation_subject(&values, annotated, &mut 8).unwrap(),
            Some(plain)
        );

        assert_ne!(
            super::observation_subject(&values, sibling, &mut 8).unwrap(),
            Some(plain)
        );

        assert_eq!(
            super::observation_subject(&values, annotated, &mut 0).unwrap(),
            None
        );
    }

    #[test]
    fn open_payload_does_not_hide_known_presence() {
        let values = SemanticValueStore::try_new().unwrap();

        let input = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let present = values
            .intern_constant_term(ConstantTermData::NullablePresent(input))
            .unwrap();

        assert_eq!(
            test_term_shape(&values, present, ConstantTest::NullablePresent, &mut 4),
            Ok(Some(true))
        );

        assert_eq!(
            test_term_shape(&values, input, ConstantTest::NullablePresent, &mut 4),
            Ok(None)
        );

        assert_eq!(
            test_term_shape(&values, present, ConstantTest::NullablePresent, &mut 0),
            Ok(None)
        );
    }

    #[test]
    fn computed_observations_are_not_mutable_storage_paths() {
        let values = SemanticValueStore::try_new().unwrap();

        let root = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let field = values
            .intern_constant_term(ConstantTermData::Projection(
                bray_symbols::ConstantProjection::new(
                    root,
                    bray_symbols::ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)),
                ),
            ))
            .unwrap();

        let computed = values
            .intern_constant_term(ConstantTermData::Unary {
                operation: bray_symbols::ConstantUnaryOperation::LogicalNot,
                operand: field,
            })
            .unwrap();

        let computed_field = values
            .intern_constant_term(ConstantTermData::Projection(
                bray_symbols::ConstantProjection::new(
                    computed,
                    bray_symbols::ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)),
                ),
            ))
            .unwrap();

        for term in [root, field] {
            assert_eq!(
                super::storage_observation_subject(&values, term, &mut 20).unwrap(),
                Some(term)
            );
        }

        for term in [computed, computed_field] {
            assert_eq!(
                super::storage_observation_subject(&values, term, &mut 20).unwrap(),
                None
            );
        }

        assert_eq!(
            super::storage_observation_subject(&values, field, &mut 0).unwrap(),
            None
        );
    }

    #[test]
    fn literal_equality_uses_language_float_semantics_and_a_work_budget() {
        use bray_symbols::{
            ConstantValueData, ConstantValueKind, IntegerConstant, RealConstantBits, TypeData,
        };

        let values = SemanticValueStore::try_new().unwrap();
        let ty = values.intern_type(TypeData::tuple([])).unwrap();

        let literal = |kind| {
            let value = values
                .intern_constant_value(ConstantValueData::new(ty, kind))
                .unwrap();

            values
                .intern_constant_term(ConstantTermData::Value(value))
                .unwrap()
        };

        for (left, right, equal) in [
            (0.0_f32, -0.0_f32, true),
            (f32::NAN, f32::NAN, false),
            (1.0_f32, 2.0_f32, false),
        ] {
            let left = literal(ConstantValueKind::Real(RealConstantBits::Binary32(
                left.to_bits(),
            )));

            let right = literal(ConstantValueKind::Real(RealConstantBits::Binary32(
                right.to_bits(),
            )));

            assert_eq!(
                super::literal_observations_equal(&values, left, right, &mut 20).unwrap(),
                Some(equal)
            );
        }

        let first = literal(ConstantValueKind::Integer(IntegerConstant::from_u64(0)));
        let second = literal(ConstantValueKind::Integer(IntegerConstant::from_u64(1)));

        assert_eq!(
            super::literal_observations_equal(&values, first, first, &mut 20).unwrap(),
            Some(true)
        );

        assert_eq!(
            super::literal_observations_equal(&values, first, second, &mut 20).unwrap(),
            Some(false)
        );

        assert_eq!(
            super::literal_observations_equal(&values, first, second, &mut 0).unwrap(),
            None
        );
    }

    #[test]
    fn aggregate_observations_reduce_selected_paths_and_preserve_unknown_paths() {
        use bray_symbols::{
            ConstantProjection, ConstantProjectionKind, ConstantValueData, ConstantValueKind,
            IntegerConstant, TypeData,
        };

        let values = SemanticValueStore::try_new().unwrap();
        let ty = values.intern_type(TypeData::tuple([])).unwrap();

        let first = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let second = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(1)))
            .unwrap();

        let tuple = values
            .intern_constant_term(ConstantTermData::tuple([first, second]))
            .unwrap();

        let present = values
            .intern_constant_term(ConstantTermData::NullablePresent(tuple))
            .unwrap();

        let unwrap = values
            .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
                present,
                ConstantProjectionKind::NullableValue,
            )))
            .unwrap();

        let selected = values
            .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
                unwrap,
                ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)),
            )))
            .unwrap();

        assert_eq!(
            super::observation_subject(&values, selected, &mut 30).unwrap(),
            Some(second)
        );

        assert_eq!(
            super::observation_subject(&values, selected, &mut 2).unwrap(),
            None
        );

        let array = values
            .intern_constant_term(ConstantTermData::array([first, second]))
            .unwrap();

        let index = values
            .intern_constant_value(ConstantValueData::new(
                ty,
                ConstantValueKind::Integer(IntegerConstant::from_u64(1)),
            ))
            .unwrap();

        let index = values
            .intern_constant_term(ConstantTermData::Value(index))
            .unwrap();

        let element = values
            .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
                array,
                ConstantProjectionKind::ArrayElement(index),
            )))
            .unwrap();

        assert_eq!(
            super::observation_subject(&values, element, &mut 30).unwrap(),
            Some(second)
        );

        let unknown = values
            .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
                array,
                ConstantProjectionKind::ArrayElement(first),
            )))
            .unwrap();

        assert_eq!(
            super::observation_subject(&values, unknown, &mut 30).unwrap(),
            Some(unknown)
        );

        let missing = values
            .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
                tuple,
                ConstantProjectionKind::TupleElement(SymbolOrdinal::new(9)),
            )))
            .unwrap();

        assert_eq!(
            super::observation_subject(&values, missing, &mut 30).unwrap(),
            Some(missing)
        );
    }
}
