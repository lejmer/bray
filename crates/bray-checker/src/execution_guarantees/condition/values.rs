use super::ExecutionCondition;
use bray_bound_tree::BoundOperator;
use std::sync::Arc;

impl ExecutionCondition {
    /// Identifies the complete nominal carrier within tuple, array and nullable wrappers.
    /// The supplied type is the checked type of the root input or completion result.
    pub fn witness_carrier(
        &self,
        values: &bray_symbols::SemanticValueStore,
        mut ty: bray_symbols::TypeId,
    ) -> Option<(Self, bray_symbols::TypeId)> {
        let mut path = Vec::new();
        let mut subject = self;

        let mut carrier = loop {
            match subject {
                Self::Projection(projection, parent) => {
                    path.push(*projection);
                    subject = parent;
                }
                Self::Borrowed(parent) => subject = parent,
                Self::Result => break Self::Result,
                Self::Input(place) => {
                    path.extend(place.projections.iter().rev().copied());

                    let mut root = place.clone();

                    root.projections = Arc::new([]);
                    break Self::Input(root);
                }
                _ => return None,
            }
        };

        for projection in path.into_iter().rev() {
            while let bray_symbols::TypeData::Borrow { target, .. } = values.type_data(ty).as_ref()
            {
                ty = *target;
            }

            let data = values.type_data(ty);

            ty = match (data.as_ref(), projection) {
                (
                    bray_symbols::TypeData::Tuple(elements),
                    bray_symbols::ConstantProjectionKind::TupleElement(index),
                ) => *elements
                    .get(usize::try_from(index.raw()).expect("checked tuple ordinal fits usize"))
                    .expect("checked carrier projection retains a tuple element"),
                (
                    bray_symbols::TypeData::Nullable(inner),
                    bray_symbols::ConstantProjectionKind::NullableValue,
                )
                | (
                    bray_symbols::TypeData::Array { element: inner, .. },
                    bray_symbols::ConstantProjectionKind::ArrayElementOrdinal(_),
                ) => *inner,
                _ => break,
            };

            carrier = carrier.component(projection);
        }

        Some((carrier, ty))
    }

    pub(crate) fn call(callable: bray_symbols::CallableInstanceData, arguments: Vec<Self>) -> Self {
        if arguments.iter().any(Self::contains_unknown) {
            Self::Unknown
        } else {
            Self::Call(callable, arguments.into())
        }
    }

    pub(crate) fn coerce(
        self,
        values: &bray_symbols::SemanticValueStore,
        source: bray_symbols::TypeId,
        target: bray_symbols::TypeId,
    ) -> Self {
        if source != target
            && matches!(values.type_data(target).as_ref(), bray_symbols::TypeData::Nullable(element) if *element == source)
        {
            self.present()
        } else {
            self
        }
    }

    pub(crate) fn pattern(subject: Self, operator: BoundOperator, expected: Self) -> Self {
        if let Self::Variant(variant) = expected {
            subject.matches_variant(variant)
        } else {
            Self::operation(operator, vec![subject, expected])
        }
    }

    pub(crate) fn matches_variant(self, variant: bray_symbols::UnionVariantSymbolId) -> Self {
        match self {
            Self::Unknown => Self::Unknown,
            Self::Borrowed(value) => value.as_ref().clone().matches_variant(variant),
            Self::Literal(ref value)
                if let bray_symbols::ConstantValueKind::Union {
                    variant: active, ..
                } = value.kind() =>
            {
                Self::Boolean(*active == variant)
            }
            Self::Tagged(active, _) => Self::Boolean(active == variant),
            Self::Constructed(base, fields) => Self::component_base(&base, &fields)
                .clone()
                .matches_variant(variant),
            value => Self::VariantTest(variant, Arc::new(value)),
        }
    }

    pub(crate) fn present(self) -> Self {
        match self {
            Self::Unknown => Self::Unknown,
            Self::Projection(bray_symbols::ConstantProjectionKind::NullableValue, value) => {
                value.as_ref().clone()
            }
            Self::Input(mut place)
                if place.projections.last()
                    == Some(&bray_symbols::ConstantProjectionKind::NullableValue) =>
            {
                place.projections = place.projections[..place.projections.len() - 1].into();

                Self::Input(place)
            }
            value => Self::Present(Arc::new(value)),
        }
    }

    pub(crate) fn borrowed(self) -> Self {
        if self == Self::Unknown {
            self
        } else {
            Self::Borrowed(Arc::new(self))
        }
    }

    pub(crate) fn requiring_trusted_predicates(self) -> Self {
        match self {
            Self::Predicate(_, _, _) => Self::Trusted(Arc::new(self)),
            Self::Operation(operator, operands) => Self::operation(
                operator,
                operands
                    .iter()
                    .map(|operand| operand.clone().requiring_trusted_predicates())
                    .collect(),
            ),
            value => value,
        }
    }

    pub(crate) fn predicate(
        predicate: bray_symbols::PredicateDefinitionSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
        arguments: Vec<Self>,
    ) -> Self {
        if arguments.iter().any(Self::contains_unknown) {
            Self::Unknown
        } else {
            Self::Predicate(predicate, substitution, arguments.into())
        }
    }

    pub(crate) fn field(field: bray_symbols::AnySymbolId, value: Self) -> Self {
        let projection = match field {
            bray_symbols::AnySymbolId::StructField(field) => {
                bray_symbols::ConstantProjectionKind::ProductField(field)
            }
            bray_symbols::AnySymbolId::UnionPayloadField(field) => {
                bray_symbols::ConstantProjectionKind::UnionPayloadField(field)
            }
            _ => panic!(
                "checked execution field identifies a product or union payload field: {field:?}"
            ),
        };

        value.component(projection)
    }

    pub(crate) fn project(self, projection: bray_bound_tree::StorageProjection) -> Self {
        use bray_bound_tree::StorageProjection;

        if let (Self::Constructed(_, fields), StorageProjection::ElementFromEnd(index)) =
            (&self, projection)
        {
            let Some(index) = u32::try_from(fields.len())
                .ok()
                .and_then(|length| length.checked_sub(index.raw() + 1))
            else {
                return Self::Unknown;
            };

            return self.project(StorageProjection::ElementFromStart(
                bray_symbols::SymbolOrdinal::new(index),
            ));
        }

        let projection = match projection {
            StorageProjection::NullableValue => bray_symbols::ConstantProjectionKind::NullableValue,
            StorageProjection::ProductField(field) => {
                bray_symbols::ConstantProjectionKind::ProductField(field)
            }
            StorageProjection::ActiveUnionPayloadField { field, .. } => {
                bray_symbols::ConstantProjectionKind::UnionPayloadField(field)
            }
            StorageProjection::TupleElement(index) => {
                bray_symbols::ConstantProjectionKind::TupleElement(index)
            }
            StorageProjection::ElementFromStart(index) => {
                bray_symbols::ConstantProjectionKind::ArrayElementOrdinal(index)
            }
            _ => {
                return match self {
                    Self::Constructed(_, fields) => fields
                        .iter()
                        .find(|(candidate, _)| *candidate == projection)
                        .map(|(_, value)| value.clone())
                        .unwrap_or(Self::Unknown),
                    _ => Self::Unknown,
                };
            }
        };

        self.component(projection)
    }

    pub(crate) fn component(self, projection: bray_symbols::ConstantProjectionKind) -> Self {
        use bray_bound_tree::StorageProjection;
        use bray_symbols::ConstantProjectionKind;

        if !matches!(
            projection,
            ConstantProjectionKind::NullableValue
                | ConstantProjectionKind::ProductField(_)
                | ConstantProjectionKind::UnionPayloadField(_)
                | ConstantProjectionKind::TupleElement(_)
                | ConstantProjectionKind::ArrayElementOrdinal(_)
        ) {
            return Self::Unknown;
        }

        match self {
            Self::Present(value) if projection == ConstantProjectionKind::NullableValue => {
                value.as_ref().clone()
            }
            Self::Input(place) => Self::Input(place.component(projection)),
            // A field access through a borrow observes the referent's field,
            // rather than projecting a field from the borrow value itself.
            Self::Borrowed(value) => value.as_ref().clone().component(projection),
            // Retain the nullable owner's identity when extracting its payload. Rewrapping
            // that payload can then name the acquisition witness without asserting presence.
            value @ Self::Constructed(_, _)
                if projection == ConstantProjectionKind::NullableValue =>
            {
                Self::Projection(projection, Arc::new(value))
            }
            Self::Projection(ConstantProjectionKind::NullableValue, ref owner)
                if let Self::Constructed(base, fields) = owner.as_ref() =>
            {
                fields
                    .iter()
                    .find(|(field, _)| *field == StorageProjection::NullableValue)
                    .map(|(_, value)| value.clone().component(projection))
                    .unwrap_or_else(|| {
                        Self::component_base(&base, &fields)
                            .clone()
                            .component(ConstantProjectionKind::NullableValue)
                            .component(projection)
                    })
            }
            Self::Constructed(base, fields) => fields
                .iter()
                .find(|(candidate, _)| match (candidate, projection) {
                    (
                        StorageProjection::ProductField(candidate),
                        ConstantProjectionKind::ProductField(field),
                    ) => *candidate == field,
                    (
                        StorageProjection::ActiveUnionPayloadField {
                            field: candidate, ..
                        },
                        ConstantProjectionKind::UnionPayloadField(field),
                    ) => *candidate == field,
                    (
                        StorageProjection::TupleElement(candidate),
                        ConstantProjectionKind::TupleElement(index),
                    )
                    | (
                        StorageProjection::ElementFromStart(candidate),
                        ConstantProjectionKind::ArrayElementOrdinal(index),
                    ) => *candidate == index,
                    _ => false,
                })
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| {
                    Self::component_base(&base, &fields)
                        .clone()
                        .component(projection)
                }),
            Self::Unknown => Self::Unknown,
            value => Self::Projection(projection, Arc::new(value)),
        }
    }

    fn component_base<'a>(
        base: &'a Self,
        fields: &'a [(bray_bound_tree::StorageProjection, Self)],
    ) -> &'a Self {
        fields
            .iter()
            .find(|(projection, _)| *projection == bray_bound_tree::StorageProjection::OwnedTarget)
            .map(|(_, contents)| contents)
            .unwrap_or(base)
    }

    pub(crate) fn with_component(
        &self,
        path: &[bray_symbols::ConstantProjectionKind],
        value: Self,
    ) -> Self {
        use bray_bound_tree::StorageProjection;
        use bray_symbols::ConstantProjectionKind;

        let Some((first, rest)) = path.split_first() else {
            return value;
        };

        let projection = match first {
            ConstantProjectionKind::ProductField(field) => StorageProjection::ProductField(*field),
            ConstantProjectionKind::TupleElement(index) => StorageProjection::TupleElement(*index),
            ConstantProjectionKind::ArrayElementOrdinal(index) => {
                StorageProjection::ElementFromStart(*index)
            }
            ConstantProjectionKind::NullableValue => StorageProjection::NullableValue,
            ConstantProjectionKind::UnionPayloadField(field) => {
                let Self::Constructed(_, fields) = self else {
                    return Self::Unknown;
                };

                let Some((projection, _)) = fields.iter().find(|(projection, _)|
                    matches!(projection, StorageProjection::ActiveUnionPayloadField { field: candidate, .. } if candidate == field)
                ) else { return Self::Unknown; };

                *projection
            }
            _ => return Self::Unknown,
        };

        let previous = self.clone().component(*first);
        let child = previous.with_component(rest, value);

        if child == previous {
            return self.clone();
        }

        let (base, mut fields) = match self {
            Self::Constructed(base, fields) => (Arc::clone(base), fields.to_vec()),
            value => (Arc::new(value.clone()), Vec::new()),
        };

        if let Some((_, current)) = fields.iter_mut().find(|(field, _)| *field == projection) {
            *current = child;
        } else {
            fields.push((projection, child));
            fields.sort_by_key(|(projection, _)| *projection);
        }

        Self::Constructed(base, fields.into())
    }

    pub(crate) fn operation(operator: BoundOperator, mut operands: Vec<Self>) -> Self {
        for operand in &mut operands {
            if let Self::Constructed(base, fields) = operand {
                let contents = Self::component_base(base, fields);

                if matches!(contents, Self::Literal(_) | Self::Boolean(_)) {
                    *operand = contents.clone();
                }
            }
        }

        if !matches!(
            operator,
            BoundOperator::LogicalNot | BoundOperator::LogicalAnd | BoundOperator::LogicalOr
        ) && operands.iter().any(Self::contains_unknown)
        {
            return Self::Unknown;
        }

        if let [Self::Literal(left), Self::Literal(right)] = operands.as_slice() {
            if matches!(
                operator,
                BoundOperator::Equal
                    | BoundOperator::NotEqual
                    | BoundOperator::Less
                    | BoundOperator::LessEqual
                    | BoundOperator::Greater
                    | BoundOperator::GreaterEqual
            ) {
                if let Ok(bray_symbols::ConstantValueKind::Boolean(value)) =
                    crate::constant::fold_binary(
                        operator,
                        left.kind(),
                        right.kind(),
                        crate::ConstantEvaluationLimits::default().integer_bits(),
                    )
                {
                    return Self::Boolean(value);
                }
            }
        }

        if matches!(operator, BoundOperator::Equal | BoundOperator::NotEqual) {
            if let [Self::Borrowed(subject), Self::Literal(absent)]
            | [Self::Literal(absent), Self::Borrowed(subject)] = operands.as_slice()
                && matches!(
                    absent.kind(),
                    bray_symbols::ConstantValueKind::NullableAbsent
                )
            {
                // Nullable tests observe the reached value, as union tag tests do.
                return Self::operation(
                    operator,
                    vec![subject.as_ref().clone(), Self::Literal(Arc::clone(absent))],
                );
            }

            if let [left, Self::Literal(absent)] | [Self::Literal(absent), left] =
                operands.as_slice()
                && matches!(
                    absent.kind(),
                    bray_symbols::ConstantValueKind::NullableAbsent
                )
                && let Self::Constructed(base, fields) = left
                && fields.iter().all(|(projection, _)| {
                    *projection == bray_bound_tree::StorageProjection::NullableValue
                })
            {
                // Updating a present payload snapshot does not replace the nullable tag.
                return Self::operation(
                    operator,
                    vec![(**base).clone(), Self::Literal(Arc::clone(absent))],
                );
            }
        }

        match (operator, operands.as_slice()) {
            (
                BoundOperator::Equal | BoundOperator::NotEqual,
                [Self::Present(_), Self::Literal(absent)]
                | [Self::Literal(absent), Self::Present(_)],
            ) if matches!(
                absent.kind(),
                bray_symbols::ConstantValueKind::NullableAbsent
            ) =>
            {
                Self::Boolean(operator == BoundOperator::NotEqual)
            }
            (
                BoundOperator::Equal | BoundOperator::LessEqual | BoundOperator::GreaterEqual,
                [left, right],
            ) if left == right => Self::Boolean(true),
            (
                BoundOperator::NotEqual | BoundOperator::Less | BoundOperator::Greater,
                [left, right],
            ) if left == right => Self::Boolean(false),
            (
                BoundOperator::Equal | BoundOperator::NotEqual,
                [condition, Self::Boolean(expected)] | [Self::Boolean(expected), condition],
            ) => {
                if *expected == (operator == BoundOperator::Equal) {
                    condition.clone()
                } else {
                    Self::operation(BoundOperator::LogicalNot, vec![condition.clone()])
                }
            }
            (BoundOperator::LogicalNot, [Self::Boolean(value)]) => Self::Boolean(!value),
            (BoundOperator::LogicalNot, [Self::Operation(BoundOperator::LogicalNot, inner)]) => {
                inner.first().cloned().unwrap_or(Self::Unknown)
            }
            (BoundOperator::LogicalAnd, [Self::Boolean(left), Self::Boolean(right)]) => {
                Self::Boolean(*left && *right)
            }
            (BoundOperator::LogicalOr, [Self::Boolean(left), Self::Boolean(right)]) => {
                Self::Boolean(*left || *right)
            }
            _ => Self::Operation(operator, operands.into()),
        }
    }
}
