use super::ExecutionCondition;
use bray_bound_tree::BoundOperator;
use std::sync::Arc;

impl ExecutionCondition {
    /// Observes a checked constant value without introducing storage identity or authority.
    pub fn from_constant(
        values: &bray_symbols::SemanticValueStore,
        value: bray_symbols::ConstantValueId,
    ) -> Self {
        let borrowed = matches!(
            values.constant_value_data(value).kind(),
            bray_symbols::ConstantValueKind::Borrowed(_)
        );

        let value = values.constant_value_data(values.observed_constant_value(value));

        let observed = match value.kind() {
            bray_symbols::ConstantValueKind::Boolean(value) => Self::Boolean(*value),
            _ => Self::Literal(value),
        };

        if borrowed {
            observed.borrowed()
        } else {
            observed
        }
    }

    /// Retains the predicate's checked subject, or the value containing its marker or address field.
    /// Enclosing aggregates do not replace an independently typed child owner.
    pub fn witness_carrier(&self, containing_value: bool) -> Self {
        let subject = self.clone().into_referent();

        if !containing_value {
            return subject;
        }

        match &subject {
            Self::Input(place) if place.projections.last().is_some_and(|projection| matches!(
                projection,
                bray_symbols::ConstantProjectionKind::ProductField(_)
                    | bray_symbols::ConstantProjectionKind::UnionPayloadField(_)
            )) => {
                let mut owner = place.clone();

                owner.projections = owner.projections[..owner.projections.len() - 1].into();

                Self::Input(owner)
            }
            Self::Projection(
                bray_symbols::ConstantProjectionKind::ProductField(_)
                    | bray_symbols::ConstantProjectionKind::UnionPayloadField(_),
                parent,
            ) => parent.as_ref().clone(),
            value => value.clone(),
        }
    }

    pub(crate) fn call(
        callable: bray_symbols::CallableInstanceData,
        implementation: Option<bray_symbols::ImplementationInstanceData>,
        witnesses: impl IntoIterator<Item = bray_symbols::ImplementationInstanceData>,
        mut arguments: Vec<Self>,
    ) -> Self {
        for argument in &mut arguments {
            // Const calls compute from current values, independently of the
            // loan that supplies them. Flow values retain that loan separately.
            let value = std::mem::replace(argument, Self::Unknown).into_referent();

            let value = value.current_contents().clone();

            *argument = value.storage_contents().map(|(_, contents)| contents.clone()).unwrap_or(value);

            if argument.contains_unknown() {
                return Self::Unknown;
            }
        }

        Self::Call(
            callable,
            implementation,
            bray_base::shared_slice(witnesses),
            arguments.into(),
        )
    }

    pub(crate) fn coerce(
        self,
        values: &bray_symbols::SemanticValueStore,
        source: bray_symbols::TypeId,
        target: bray_symbols::TypeId,
    ) -> Self {
        if matches!(values.type_data(target).as_ref(), bray_symbols::TypeData::Borrow { .. }) {
            return self.borrowed();
        }

        if source != target
            && matches!(values.type_data(target).as_ref(), bray_symbols::TypeData::Nullable(element) if *element == source)
        {
            // The nullable owns a borrow value, not the reached witness carrier.
            // Retain that distinction when the same view is stored or forwarded.
            let value = if matches!(
                values.type_data(source).as_ref(),
                bray_symbols::TypeData::Borrow { .. }
            ) && !matches!(self, Self::Borrowed(_))
            {
                self.borrowed()
            } else {
                self
            };

            value.present()
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
            Self::Borrowed(value) | Self::CurrentValue(_, value) => value.as_ref().clone().matches_variant(variant),
            Self::Literal(ref value)
                if let bray_symbols::ConstantValueKind::Union {
                    variant: active, ..
                } = value.kind() =>
            {
                Self::Boolean(*active == variant)
            }
            Self::Tagged(active, _) => Self::Boolean(active == variant),
            Self::Constructed(base, _) => base.as_ref().clone().matches_variant(variant),
            value => {
                if let Some(snapshot) = value.nullable_payload_snapshot() {
                    snapshot.clone().matches_variant(variant)
                } else {
                    Self::VariantTest(variant, Arc::new(value))
                }
            }
        }
    }

    /// Recognizes an ordinary comparison that establishes nullable absence.
    pub(crate) fn nullable_absence(
        &self,
        holds: bool,
    ) -> Option<(&Self, &Arc<bray_symbols::ConstantValueData>)> {
        let Self::Operation(operator, operands) = self else {
            return None;
        };

        if !matches!(
            (operator, holds),
            (BoundOperator::Equal, true) | (BoundOperator::NotEqual, false)
        ) {
            return None;
        }

        match operands.as_ref() {
            [subject, Self::Literal(absent)] | [Self::Literal(absent), subject]
                if matches!(
                    absent.kind(),
                    bray_symbols::ConstantValueKind::NullableAbsent
                ) =>
            {
                Some((subject, absent))
            }
            _ => None,
        }
    }

    /// Reads nullable payload contents while retaining the extraction's owning observation.
    pub(crate) fn nullable_payload_snapshot(&self) -> Option<&Self> {
        let Self::Projection(bray_symbols::ConstantProjectionKind::NullableValue, owner) = self
        else {
            return None;
        };

        let Self::Constructed(_, fields) = owner.as_ref() else {
            return None;
        };

        fields
            .iter()
            .find(|(field, _)| *field == bray_bound_tree::StorageProjection::NullableValue)
            .map(|(_, value)| value)
    }

    pub(crate) fn present(self) -> Self {
        match self {
            Self::Unknown => Self::Unknown,
            payload @ Self::Projection(bray_symbols::ConstantProjectionKind::NullableValue, _) => {
                if let Some(snapshot) = payload.nullable_payload_snapshot()
                    && (matches!(
                        snapshot,
                        Self::Projection(bray_symbols::ConstantProjectionKind::NullableValue, _)
                    ) || matches!(snapshot, Self::Input(place) if place.projections.last() == Some(&bray_symbols::ConstantProjectionKind::NullableValue)))
                {
                    return snapshot.clone().present();
                }

                let Self::Projection(_, value) = payload else {
                    unreachable!()
                };

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
        if let Some(payload) = self.nullable_payload_snapshot() {
            return payload.clone().borrowed();
        }

        if let Self::CurrentValue(owner, contents) = &self
            && !matches!(owner.as_ref(), Self::Borrowed(_))
        {
            return Self::current_value(owner.as_ref().clone().borrowed(), contents.as_ref().clone());
        }

        if !matches!(self.owner_identity(), Self::Borrowed(_)) && self.owner_identity() != &self {
            return Self::current_value(self.owner_identity().clone().borrowed(), self);
        }

        // This marks a live observation, not an additional dereference. Formal
        // predicate substitution and runtime forwarding can reach the same view.
        if self == Self::Unknown || matches!(self.owner_identity(), Self::Borrowed(_)) {
            self
        } else {
            Self::Borrowed(Arc::new(self))
        }
    }

    /// Refresh reached contents while retaining this existing loan's identity and lifetime.
    pub(crate) fn with_referent(&self, contents: Self) -> Self {
        if self == &contents { return self.clone(); }

        let identity = self.owner_identity();

        if *identity == Self::Unknown { return Self::Unknown; }

        // A scalar view retains its physical storage even when its copied data changes.
        let previous = self.clone().into_referent();

        let contents = if let Some((owner, _)) = previous.storage_contents() {
            Self::storage_value(owner.clone(), contents)
        } else { contents };

        if let Self::Borrowed(previous) = self && previous.as_ref() == &contents {
            return self.clone();
        }

        Self::current_value(identity.clone().borrowed(), contents)
    }

    /// Structural overlays belong inside their existing owner or loan context.
    pub(crate) fn constructed(
        base: Self,
        fields: Arc<[(bray_bound_tree::StorageProjection, Self)]>,
    ) -> Self {
        match base {
            Self::CurrentValue(owner, contents) => Self::current_value(
                owner.as_ref().clone(), Self::Constructed(contents, fields),
            ),
            Self::Borrowed(contents) => Self::current_value(
                Self::Borrowed(Arc::clone(&contents)), Self::Constructed(contents, fields),
            ),
            base => Self::Constructed(Arc::new(base), fields),
        }
    }

    pub(crate) fn current_value(owner: Self, contents: Self) -> Self {
        let owner = owner.owner_identity();

        if owner == &contents || matches!(&contents, Self::CurrentValue(current, _) if current.as_ref() == owner) {
            return contents;
        }

        Self::CurrentValue(Arc::new(owner.clone()), Arc::new(contents))
    }

    /// Current-value contexts retain storage or loan identity outside value terms.
    fn current_contents(&self) -> &Self {
        let mut current = self;

        while let Self::CurrentValue(_, contents) = current { current = contents; }

        current
    }

    pub(crate) fn storage_value(owner: Self, value: Self) -> Self {
        let owner = owner.owner_identity().clone().into_referent().owner_identity().clone();

        if value.storage_contents().is_some_and(|(current, _)| current == &owner) {
            return value;
        }

        Self::StorageValue(Arc::new(owner), Arc::new(value))
    }

    /// An existing storage context and its current scalar contents are separate observations.
    pub(crate) fn storage_contents(&self) -> Option<(&Self, &Self)> {
        match self {
            Self::StorageValue(owner, value) => Some((owner, value)),
            _ => None,
        }
    }

    /// Terms observe current contents. The enclosing flow value retains the loan context.
    fn observe_contents(&mut self) {
        if matches!(self, Self::CurrentValue(owner, _) if matches!(owner.as_ref(), Self::Borrowed(_))) {
            *self = self.clone().into_referent().current_contents().clone().borrowed();
        }
    }

    /// Observe the current reached value. Only checked ownership transitions may move it.
    pub(crate) fn into_referent(self) -> Self {
        match self {
            Self::Borrowed(value) => value.as_ref().clone().into_referent(),
            Self::CurrentValue(owner, contents) if matches!(owner.as_ref(), Self::Borrowed(_)) => {
                contents.as_ref().clone().into_referent()
            }
            value => value,
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
        mut arguments: Vec<Self>,
    ) -> Self {
        for argument in &mut arguments {
            argument.observe_contents();

            if argument.contains_unknown() {
                return Self::Unknown;
            }
        }

        Self::Predicate(predicate, substitution, arguments.into())
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

        if let Self::CurrentValue(_, contents) = self {
            return contents.as_ref().clone().project(projection);
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
            Self::StorageValue(_, value) | Self::CurrentValue(_, value) => value.as_ref().clone().component(projection),
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
                        Self::component_base(base, fields)
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
                    base.as_ref().clone().component(projection)
                }),
            Self::Unknown => Self::Unknown,
            value => Self::Projection(projection, Arc::new(value)),
        }
    }

    pub(crate) fn with_component(
        &self,
        path: &[bray_symbols::ConstantProjectionKind],
        value: Self,
        storage: Option<&[bray_bound_tree::StorageProjection]>,
    ) -> Self {
        use bray_bound_tree::StorageProjection;
        use bray_symbols::ConstantProjectionKind;

        let Some((first, rest)) = path.split_first() else {
            return value;
        };

        if let Self::CurrentValue(owner, contents) = self {
            return Self::current_value(owner.as_ref().clone(), contents.with_component(path, value, storage));
        }

        if let Some(snapshot) = self.nullable_payload_snapshot() {
            let updated = snapshot.with_component(path, value, storage);

            if updated == *snapshot {
                return self.clone();
            }

            let Self::Projection(_, owner) = self else {
                unreachable!("a nullable payload snapshot retains its owner projection");
            };

            return owner
                .with_component(&[ConstantProjectionKind::NullableValue], updated, storage)
                .component(ConstantProjectionKind::NullableValue);
        }

        let projection = match first {
            ConstantProjectionKind::ProductField(field) => StorageProjection::ProductField(*field),
            ConstantProjectionKind::TupleElement(index) => StorageProjection::TupleElement(*index),
            ConstantProjectionKind::ArrayElementOrdinal(index) => {
                StorageProjection::ElementFromStart(*index)
            }
            ConstantProjectionKind::NullableValue => StorageProjection::NullableValue,
            ConstantProjectionKind::UnionPayloadField(field) => {
                // The checked storage path retains the variant even when this
                // observation has not materialized an active payload yet.
                let projection = storage.into_iter().flatten().find(|projection|
                    matches!(projection, StorageProjection::ActiveUnionPayloadField { field: candidate, .. } if candidate == field)
                ).or_else(|| {
                    let Self::Constructed(_, fields) = self else { return None; };

                    fields.iter().find_map(|(projection, _)|
                        matches!(projection, StorageProjection::ActiveUnionPayloadField { field: candidate, .. } if candidate == field).then_some(projection)
                    )
                });

                let Some(projection) = projection else {
                    return Self::Unknown;
                };

                *projection
            }
            _ => return Self::Unknown,
        };

        let previous = self.clone().component(*first);

        // Stored components contain payload data. The extraction view retains
        // its owner separately and must not become its own payload's backing.
        let previous = previous.nullable_payload_snapshot().unwrap_or(&previous);
        let child = previous.with_component(rest, value, storage);

        if child == *previous {
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

        Self::constructed(base.as_ref().clone(), fields.into())
    }

    pub(crate) fn operation(operator: BoundOperator, mut operands: Vec<Self>) -> Self {
        for operand in &mut operands {
            operand.observe_contents();

            if matches!(operand, Self::CurrentValue(_, _)) {
                *operand = operand.current_contents().clone();
            }

            if let Self::StorageValue(_, value) | Self::CurrentValue(_, value) = operand
                && matches!(value.as_ref(), Self::Literal(_) | Self::Boolean(_)) {
                *operand = value.as_ref().clone();
            }

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

        if let [Self::Literal(value)] = operands.as_slice()
            && let Some(kind) = crate::constant::fold_numeric_sign(operator, value.kind())
        {
            return Self::Literal(Arc::new(bray_symbols::ConstantValueData::new(
                value.ty(),
                kind,
            )));
        }

        if let [Self::Literal(left), Self::Literal(right)] = operands.as_slice()
            && matches!(
                operator,
                BoundOperator::Equal
                    | BoundOperator::NotEqual
                    | BoundOperator::Less
                    | BoundOperator::LessEqual
                    | BoundOperator::Greater
                    | BoundOperator::GreaterEqual
            )
            && let Ok(bray_symbols::ConstantValueKind::Boolean(value)) =
                crate::constant::fold_binary(
                    operator,
                    left.kind(),
                    right.kind(),
                    crate::ConstantEvaluationLimits::default().integer_bits(),
                )
        {
            return Self::Boolean(value);
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
            // Strict self-ordering is false even for NaN. Equality and weak ordering
            // require value-domain evidence that this untyped normalization does not carry.
            (BoundOperator::Less | BoundOperator::Greater, [left, right]) if left == right => {
                Self::Boolean(false)
            }
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
            (BoundOperator::LogicalAnd, [Self::Boolean(false), _] | [_, Self::Boolean(false)]) => {
                Self::Boolean(false)
            }
            (BoundOperator::LogicalOr, [Self::Boolean(true), _] | [_, Self::Boolean(true)]) => {
                Self::Boolean(true)
            }
            (
                BoundOperator::LogicalAnd,
                [Self::Boolean(true), value] | [value, Self::Boolean(true)],
            )
            | (
                BoundOperator::LogicalOr,
                [Self::Boolean(false), value] | [value, Self::Boolean(false)],
            ) => value.clone(),
            _ => Self::Operation(operator, operands.into()),
        }
    }
}
