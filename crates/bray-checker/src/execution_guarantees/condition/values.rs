use crate::execution_guarantees::ExecutionObservationKind as ObservationKind;
use super::ExecutionCondition;
use bray_bound_tree::BoundOperator;
use std::sync::Arc;

impl ExecutionCondition {
    /// Name new contents by the existing value, independently of an alias slot.
    /// Replaying a mutation retains the defining identity instead of nesting epochs.
    pub(crate) fn post_state(mutation: bray_bound_tree::StorageMutation, identity: Self) -> Self {
        match identity.owner_identity() {
            Self::Unknown => Self::Unknown,
            Self::PostState(_, identity) => Self::PostState(mutation, Arc::clone(identity)),
            identity => Self::PostState(mutation, Arc::new(identity.clone())),
        }
    }

    /// Retain this descriptor observation through later input substitution.
    pub(crate) fn descriptor(&self) -> Self {
        let identity = self.descriptor_identity();

        if identity.contains_unknown() { return Self::Unknown; }

        let identity = identity.clone().into_referent();

        match identity {
            Self::Descriptor(_) | Self::Unknown => identity,
            identity => Self::Descriptor(Arc::new(identity)),
        }
    }

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
        reference_type: Option<bray_symbols::TypeId>,
    ) -> Self {
        for argument in &mut arguments {
            // Const calls compute from current values, independently of the
            // loan that supplies them. Flow values retain that loan separately.
            *argument = argument.computed_value().clone();

            if argument.contains_unknown() {
                return Self::Unknown;
            }
        }

        Self::Call(
            callable,
            implementation,
            bray_base::shared_slice(witnesses),
            arguments.into(),
            reference_type,
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

    /// The active tag established by this produced value, through inline data wrappers.
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

        if self == Self::Unknown || self.copied_value().is_reference() {
            // Copying a loan does not create a loan to its private copy context.
            return self.copied_value().clone();
        }

        if self.is_reference_computation() {
            return Self::Borrowed(Arc::new(self));
        }

        if let Self::Projection(projection, parent) = &self {
            let selected = parent.as_ref().clone().component(*projection);

            if selected != self {
                return selected.borrowed();
            }
        }

        let owner = self.owner_identity();

        if owner.ownership_identity().is_none() {
            return Self::reference_value(Self::Unknown, self);
        }

        if owner == &self {
            Self::Borrowed(Arc::new(self))
        } else {
            Self::reference_value(Self::Borrowed(Arc::new(owner.clone())), self)
        }
    }

    /// Refresh reached data while retaining the reference value and its existing access.
    pub(crate) fn with_referent(&self, contents: Self) -> Self {
        if self == &contents { return self.clone(); }

        let identity = self.owner_identity();

        if *identity == Self::Unknown { return Self::Unknown; }

        // The observed physical context survives a contents update. The enclosing
        // reference retains access to that context without acquiring ownership.
        let previous = self.clone().into_referent();

        let contents = match previous {
            Self::Observation { owner, kind: kind @ (ObservationKind::Owned | ObservationKind::Scalar), .. } => {
                Self::observation(owner.as_ref().clone(), contents, kind)
            }
            _ => contents,
        };

        Self::reference_value(identity.clone(), contents)
    }

    /// Replaces current contents without reclassifying the actual descriptor.
    pub(crate) fn with_current_contents(&self, contents: Self) -> Self {
        // A lifted nullable has its payload's identity, not a separate whole-input
        // context. Its retained children already carry the surviving authority.
        if self.observation_kind().is_none() && !self.is_reference() && self.known_presence().is_some() {
            return contents;
        }


        match self.ownership_identity() {
            Some((ObservationKind::Reference, _)) => self.with_referent(contents),
            Some((ObservationKind::Owned, identity)) => Self::owned_value(identity.clone(), contents),
            _ => Self::scalar_observation(self.owner_identity().clone(), contents),
        }
    }

    /// Structural overlays retain the existing value's identity and ownership role.
    pub(crate) fn constructed(
        base: Self,
        mut fields: Arc<[(bray_bound_tree::StorageProjection, Self)]>,
    ) -> Self {
        if let Self::Present(payload) = &base
            && matches!(fields.as_ref(), [(bray_bound_tree::StorageProjection::NullableValue, value)] if value == payload.as_ref())
        {
            return base;
        }

        if !fields.windows(2).all(|pair| pair[0].0 < pair[1].0) {
            Arc::make_mut(&mut fields).sort_by_key(|(projection, _)| *projection);
        }

        match base {
            Self::Constructed(base, previous) => {
                // Compose existing immutable snapshots once. Later fields replace
                // earlier contents while preserving untouched components.
                let fields = previous.iter().chain(fields.iter()).cloned()
                    .collect::<std::collections::BTreeMap<_, _>>();

                Self::constructed(base.as_ref().clone(), fields.into_iter().collect())
            }
            Self::Observation { owner, contents, kind } => Self::observation(
                owner.as_ref().clone(), Self::constructed(contents.as_ref().clone(), fields), kind,
            ),
            Self::Borrowed(contents) => Self::reference_value(
                Self::Borrowed(Arc::clone(&contents)),
                Self::constructed(contents.as_ref().clone(), fields),
            ),
            base => Self::Constructed(Arc::new(base), fields),
        }
    }

    pub(crate) fn owned_value(owner: Self, contents: Self) -> Self {
        Self::observation(owner, contents, ObservationKind::Owned)
    }

    pub(crate) fn reference_value(owner: Self, contents: Self) -> Self {
        Self::observation(owner, contents, ObservationKind::Reference)
    }

    /// Data produced without a known scalar payload still has its defining identity.
    pub(crate) fn opaque_scalar(identity: Self) -> Self {
        Self::scalar_observation(identity.clone(), Self::ScalarValue(Arc::new(identity)))
    }

    pub(crate) fn value_alias(left: Self, right: Self) -> Self {
        let left = left.copied_value().clone();
        let right = right.copied_value().clone();

        if left.contains_unknown() || right.contains_unknown() { return Self::Unknown; }

        assert!(!left.contains_trusted() && !right.contains_trusted()
            && !left.is_reference() && !right.is_reference(),
            "data aliases require ordinary copied observations: left={left:?}, right={right:?}");

        if left == right { return Self::Boolean(true); }

        let (left, right) = if left < right { (left, right) } else { (right, left) };

        Self::ValueAlias(Arc::new(left), Arc::new(right))
    }

    pub(crate) fn scalar_observation(owner: Self, contents: Self) -> Self {
        let owner = owner.owner_identity().clone().into_referent().owner_identity().clone();

        Self::observation(owner, contents, ObservationKind::Scalar)
    }

    pub(crate) fn observation(owner: Self, contents: Self, kind: ObservationKind) -> Self {
        let owner = owner.owner_identity();

        if kind == ObservationKind::Owned && owner == &contents
            || kind == ObservationKind::Reference && matches!(owner, Self::Borrowed(referent) if referent.as_ref() == &contents)
        {
            return owner.clone();
        }

        if matches!(&contents, Self::Observation { owner: current, kind: current_kind, .. } if current.as_ref() == owner && *current_kind == kind) {
            return contents;
        }

        Self::Observation { owner: Arc::new(owner.clone()), contents: Arc::new(contents), kind }
    }

    pub(crate) fn observation_kind(&self) -> Option<ObservationKind> {
        match self {
            Self::Observation { kind, .. } => Some(*kind),
            _ => None,
        }
    }

    /// Reached data does not own storage. The reference value retains its access capability.
    pub(crate) fn current_referent(&self) -> Option<&Self> {
        match self {
            Self::Observation { contents, kind: ObservationKind::Reference, .. } => Some(contents),
            _ => None,
        }
    }

    pub(crate) fn is_reference(&self) -> bool {
        matches!(self, Self::Borrowed(_)) || self.current_referent().is_some()
    }

    /// Built-in computations read values rather than their storage or loan identities.
    pub(crate) fn computed_value(&self) -> &Self {
        let mut value = self;

        loop {
            match value {
                Self::Borrowed(contents) | Self::Observation { contents, .. } => value = contents,
                _ => return value,
            }
        }
    }

    /// A scalar storage context and its current copied contents are distinct observations.
    pub(crate) fn storage_contents(&self) -> Option<(&Self, &Self)> {
        match self {
            Self::Observation { owner, contents, kind: ObservationKind::Scalar } => Some((owner, contents)),
            _ => None,
        }
    }

    /// The immutable value copied into a new context, without renaming its source owner.
    pub(crate) fn copied_value(&self) -> &Self {
        let mut value = self;

        while let Some((_, contents)) = value.storage_contents() {
            value = contents;
        }

        value
    }

    /// Observe the current reached value. Only checked ownership transitions may move it.
    pub(crate) fn into_referent(self) -> Self {
        match self {
            Self::Borrowed(value) | Self::Observation { contents: value, kind: ObservationKind::Reference, .. } => {
                value.as_ref().clone().into_referent()
            }
            value => value,
        }
    }

    /// A typed argument either retains its loan or reads the reached value.
    pub(crate) fn argument_value(self, borrowed: bool) -> Self {
        if borrowed { return self.borrowed(); }

        let mut value = self;

        loop {
            match value {
                Self::Borrowed(contents) | Self::Observation {
                    contents, kind: ObservationKind::Reference | ObservationKind::Scalar, ..
                } => value = contents.as_ref().clone(),
                _ => return value,
            }
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
            // A by-value predicate observes copied data. A borrowed argument
            // retains its storage context even when the contents are equal.
            *argument = argument.copied_value().clone();

            if let Self::Present(payload) = argument {
                // Predicate arguments name the nullable value. Structural visitors
                // separately retain its explicit initialized payload snapshot.
                *argument = payload.as_ref().clone().present();
            }

            if let Self::Observation { contents, kind: ObservationKind::Reference, .. } = argument
                && (contents.ownership_identity().is_some_and(|(role, _)| role == ObservationKind::Owned)
                    || contents.is_reference_computation())
            {
                // Retain the actual referent or checked reference computation,
                // independently of the loan handle. Scalar contexts remain distinct.
                *argument = contents.as_ref().clone().borrowed();
            }
        }

        for argument in &arguments {
            if argument.contains_unknown() {
                return Self::Unknown;
            }
        }

        Self::Predicate(predicate, substitution, arguments.into())
    }

    fn is_reference_computation(&self) -> bool {
        let mut value = self;

        while let Self::Projection(_, parent) = value {
            value = parent;
        }

        matches!(value, Self::Call(_, _, _, _, Some(_)))
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

        if projection == StorageProjection::AddressTarget { return self.into_referent(); }

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

        if let Self::Observation { owner, contents, kind } = self {
            let selected = contents.as_ref().clone().project(projection);

            return Self::projected_observation(kind, selected, || owner.as_ref().clone().project(projection));
        }

        if let Self::Borrowed(value) = self {
            return value.as_ref().clone().project(projection).borrowed();
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
            Self::Observation { owner, contents, kind } => {
                let selected = contents.as_ref().clone().component(projection);

                Self::projected_observation(kind, selected, || owner.as_ref().clone().component(projection))
            }
            Self::Borrowed(value) => value.as_ref().clone().component(projection).borrowed(),
            Self::Tagged(_, value) => value.as_ref().clone().component(projection),
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
                        base.as_ref().clone().component(ConstantProjectionKind::NullableValue)
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
            Self::Unknown | Self::Discriminant(_) | Self::Descriptor(_) => Self::Unknown,
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

        let mut current = self.clone();
        let mut projections = Vec::with_capacity(path.len());

        for component in path {
            let projection = match component {
                ConstantProjectionKind::ProductField(field) => StorageProjection::ProductField(*field),
                ConstantProjectionKind::TupleElement(index) => StorageProjection::TupleElement(*index),
                ConstantProjectionKind::ArrayElementOrdinal(index) => {
                    StorageProjection::ElementFromStart(*index)
                }
                ConstantProjectionKind::NullableValue => StorageProjection::NullableValue,
                ConstantProjectionKind::UnionPayloadField(field) => {
                    // A typed path retains the variant even before its payload is observed.
                    let projection = storage.into_iter().flatten().find(|projection|
                        matches!(projection, StorageProjection::ActiveUnionPayloadField { field: candidate, .. } if candidate == field)
                    ).copied().or_else(|| {
                        let mut observed = &current;

                        loop {
                            if let Self::Observation { contents, .. } = observed {
                                observed = contents;
                            } else if let Some(payload) = observed.nullable_payload_snapshot() {
                                observed = payload;
                            } else {
                                break;
                            }
                        }

                        let Self::Constructed(_, fields) = observed else { return None; };

                        fields.iter().find_map(|(projection, _)|
                            matches!(projection, StorageProjection::ActiveUnionPayloadField { field: candidate, .. } if candidate == field).then_some(*projection)
                        )
                    });

                    let Some(projection) = projection else { return Self::Unknown; };

                    projection
                }
                _ => return Self::Unknown,
            };

            projections.push(projection);
            current = current.component(*component);
        }

        self.with_storage_component(&projections, value)
    }

    /// Update retained data along a checked storage path without losing its selectors.
    pub(crate) fn with_storage_component(
        &self,
        path: &[bray_bound_tree::StorageProjection],
        value: Self,
    ) -> Self {
        use bray_bound_tree::StorageProjection;
        use bray_symbols::ConstantProjectionKind;

        let Some((projection, rest)) = path.split_first() else {
            return value;
        };

        if let Some(data) = self.current_referent() {
            return self.with_referent(data.with_storage_component(path, value));
        }

        if let Self::Observation { owner, contents, kind } = self {
            return Self::observation(owner.as_ref().clone(), contents.with_storage_component(path, value), *kind);
        }

        if let Some(snapshot) = self.nullable_payload_snapshot() {
            let updated = snapshot.with_storage_component(path, value);

            if updated == *snapshot {
                return self.clone();
            }

            let Self::Projection(_, owner) = self else {
                unreachable!("a nullable payload snapshot retains its owner projection");
            };

            return owner
                .with_storage_component(&[StorageProjection::NullableValue], updated)
                .component(ConstantProjectionKind::NullableValue);
        }

        let previous = self.clone().project(*projection);

        // Stored components contain payload data. The extraction view retains
        // its owner separately and must not become its own payload's backing.
        let previous = previous.nullable_payload_snapshot().unwrap_or(&previous);
        let value = value.nullable_payload_snapshot().cloned().unwrap_or(value);
        let child = previous.with_storage_component(rest, value);

        if child == *previous {
            return self.clone();
        }

        Self::constructed(self.clone(), Arc::from([(*projection, child)]))
    }

    fn boolean_operation(operator: BoundOperator, operands: Vec<Self>) -> Self {
        let conjunction = operator == BoundOperator::LogicalAnd;
        let mut remaining = Self::WORK_LIMIT;
        let mut pending = operands;
        let mut terms = std::collections::BTreeSet::new();

        while let Some(operand) = pending.pop() {
            let Some(next) = remaining.checked_sub(1) else {
                return Self::Unknown;
            };

            remaining = next;

            match operand {
                Self::Boolean(value) if value != conjunction => return Self::Boolean(value),
                Self::Boolean(_) => {}
                Self::Operation(inner, children) if inner == operator => {
                    if children.len() > remaining {
                        return Self::Unknown;
                    }

                    pending.extend(children.iter().cloned());
                }
                value => { terms.insert(value); }
            }
        }

        let mut level = terms.into_iter().collect::<Vec<_>>();

        if level.is_empty() {
            return Self::Boolean(conjunction);
        }

        // Build directly: recursively calling operation would repeatedly flatten the tree.
        // Sorted leaves and deterministic pairing give associative inputs one binary shape.
        while level.len() > 1 {
            let mut next = Vec::with_capacity(level.len().div_ceil(2));
            let mut values = level.into_iter();

            while let Some(left) = values.next() {
                next.push(match values.next() {
                    Some(right) => {
                        let Some(next_remaining) = remaining.checked_sub(1) else {
                            return Self::Unknown;
                        };

                        remaining = next_remaining;

                        Self::Operation(operator, vec![left, right].into())
                    }
                    None => left,
                });
            }

            level = next;
        }

        level.pop().expect("nonempty Boolean terms retain their result")
    }

    pub(crate) fn operation(operator: BoundOperator, mut operands: Vec<Self>) -> Self {
        for operand in &mut operands {
            *operand = operand.computed_value().clone();
        }

        if matches!(operator, BoundOperator::LogicalAnd | BoundOperator::LogicalOr) {
            return Self::boolean_operation(operator, operands);
        }

        if matches!(operator, BoundOperator::Equal | BoundOperator::NotEqual)
            && let [value, Self::Literal(absent)] | [Self::Literal(absent), value] = operands.as_slice()
            && matches!(absent.kind(), bray_symbols::ConstantValueKind::NullableAbsent)
        {
            if let Some(present) = value.known_presence() {
                return Self::Boolean(present == (operator == BoundOperator::NotEqual));
            }

            let tag = match value {
                Self::Discriminant(base) | Self::Constructed(base, _) => Some(base),
                _ => None,
            };

            if let Some(tag) = tag {
                return Self::operation(operator, vec![(**tag).clone(), Self::Literal(Arc::clone(absent))]);
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

        if operator == BoundOperator::LogicalNot
            && let [Self::Operation(comparison, inner)] = operands.as_slice()
            && let Some(complement) = comparison.equality_complement()
        {
            return Self::operation(complement, inner.to_vec());
        }

        match (operator, operands.as_slice()) {
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
            _ => Self::Operation(operator, operands.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ExecutionCondition, ObservationKind};
    use crate::ExecutionPlace;
    use bray_bound_tree::StorageProjection;
    use bray_symbols::{ConstantProjectionKind, SymbolId, SymbolOrdinal, UnionVariantSymbolId};
    use std::sync::Arc;

    #[test]
    fn descriptor_selection_survives_substitution_without_observing_reached_contents() {
        let input = |ordinal| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal)));
        let template = input(0).descriptor();
        let owner = input(1);

        for contents in [input(2), ExecutionCondition::Unknown] {
            let view = ExecutionCondition::reference_value(owner.clone().borrowed(), contents);
            let copied = ExecutionCondition::scalar_observation(input(3), view.clone());
            assert_eq!(copied.clone().borrowed(), view);

            for actual in [view, copied.clone(), copied.borrowed()] {
                let observed = template.substitute(&|_| actual.clone(), &ExecutionCondition::Unknown,
                    &mut { ExecutionCondition::WORK_LIMIT });

                assert_eq!(observed, owner.descriptor());
                assert_eq!(observed.descriptor(), observed);
                assert!(!observed.contains_unknown());
                assert!(!observed.observes_live(None));
                assert_eq!(observed.ownership_identity(), None);
            }
        }

        assert_ne!(owner.descriptor(), input(4).descriptor(), "replacement does not retain the old descriptor");
        assert_eq!(ExecutionCondition::Unknown.descriptor(), ExecutionCondition::Unknown);
        assert_eq!(ExecutionCondition::reference_value(ExecutionCondition::Unknown, input(2)).descriptor(),
            ExecutionCondition::Unknown, "known pointee data cannot supply a missing descriptor identity");

        let observed = owner.descriptor();
        let contents = [(owner.clone(), input(2))].into();
        let identities = [(owner.clone(), input(4))].into();

        assert_eq!(observed.with_equalities(&contents), observed,
            "equal data cannot equate descriptor identities");
        assert_eq!(observed.with_joined_values(&contents, &Default::default()), observed,
            "replacing reached contents cannot replace the observed descriptor");
        assert_eq!(observed.with_joined_values(&contents, &identities), input(4).descriptor(),
            "a real identity transfer must follow the identity map rather than new contents");
    }

    #[test]
    fn equality_polarity_keeps_guarded_trusted_evidence_composable() {
        use bray_bound_tree::BoundOperator;
        use std::collections::BTreeSet;

        let input = |ordinal| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal)));
        let predicate = ExecutionCondition::Trusted(Arc::new(input(2)));

        for (operator, opposite) in [(BoundOperator::Equal, BoundOperator::NotEqual),
            (BoundOperator::NotEqual, BoundOperator::Equal)]
        {
            let compare = |operator| ExecutionCondition::operation(operator, vec![input(0), input(1)]);
            let premise = ExecutionCondition::operation(BoundOperator::LogicalOr, vec![
                ExecutionCondition::operation(BoundOperator::LogicalNot, vec![compare(opposite)]), predicate.clone(),
            ]);
            let required = ExecutionCondition::operation(BoundOperator::LogicalOr,
                vec![compare(operator), predicate.clone()]);
            let facts = BTreeSet::from([(premise, true)]);

            assert_eq!(required.prove_trusted(&facts, &BTreeSet::new()), Some(true));
            assert_eq!(required.prove_trusted(&BTreeSet::new(), &facts), None,
                "ordinary occurrences cannot establish the guarded authority");
        }
    }

    #[test]
    fn repeated_guard_qualification_has_one_exact_boolean_shape() {
        use bray_bound_tree::BoundOperator;
        use std::collections::BTreeSet;

        let guard = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
        let predicate = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));

        let qualify = |clause| ExecutionCondition::operation(BoundOperator::LogicalOr, vec![
            ExecutionCondition::operation(BoundOperator::LogicalNot, vec![guard.clone()]), clause,
        ]);

        let expected = qualify(predicate.clone());
        let mut repeated = expected.clone();

        for _ in 0..4096 {
            repeated = qualify(repeated);
            assert_eq!(repeated, expected);
        }

        for guard_value in [false, true] {
            for predicate_value in [false, true] {
                let facts = BTreeSet::from([(guard.clone(), guard_value), (predicate.clone(), predicate_value)]);

                assert_eq!(repeated.prove(&facts, &mut { ExecutionCondition::WORK_LIMIT }),
                    Some(!guard_value || predicate_value));
            }
        }
    }

    #[test]
    fn boolean_grouping_is_canonical_without_unknown_complement_evidence() {
        use bray_bound_tree::BoundOperator;
        use std::collections::BTreeSet;

        let first = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
        let second = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
        let third = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(2)));

        for operator in [BoundOperator::LogicalAnd, BoundOperator::LogicalOr] {
            let combine = |left, right| ExecutionCondition::operation(operator, vec![left, right]);

            assert_eq!(combine(combine(first.clone(), second.clone()), third.clone()),
                combine(third.clone(), combine(second.clone(), first.clone())));

            let unknown_complement = combine(ExecutionCondition::Unknown,
                ExecutionCondition::operation(BoundOperator::LogicalNot, vec![ExecutionCondition::Unknown]));

            assert_eq!(unknown_complement.prove(&BTreeSet::new(), &mut { ExecutionCondition::WORK_LIMIT }), None);

            assert_eq!(ExecutionCondition::operation(operator,
                vec![first.clone(); ExecutionCondition::WORK_LIMIT + 1]), ExecutionCondition::Unknown);
        }
    }

    #[test]
    fn scalar_and_reference_projections_keep_their_roles() {
        let owner = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
        let child = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let data = ExecutionCondition::constructed(owner.clone(), [(field, child.clone())].into());
        let copied = ExecutionCondition::scalar_observation(owner.clone(), data.clone()).project(field);

        assert_eq!(copied.observation_kind(), Some(ObservationKind::Scalar));
        assert_eq!(copied.storage_contents().map(|(_, contents)| contents), Some(&child));
        assert_eq!(copied.ownership_identity(), None);
        assert!(!child.is_owned_projection_of(&copied));

        let loan = data.borrowed().component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));

        assert_eq!(loan.ownership_identity().map(|(role, _)| role), Some(ObservationKind::Reference));
        assert!(!child.is_owned_projection_of(&loan));
        assert_eq!(loan.into_referent(), child);

        let descriptor = owner.clone().borrowed();

        let copied_views = ExecutionCondition::scalar_observation(owner, ExecutionCondition::constructed(
            ExecutionCondition::Unknown, [(field, descriptor.clone())].into(),
        ));

        assert_eq!(copied_views.project(field), descriptor);
    }

    #[test]
    fn shape_decorations_keep_data_out_of_stable_loan_identity() {
        let owner = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let variant = UnionVariantSymbolId::from_symbol_id(SymbolId::new(900));

        let make = |contents| ExecutionCondition::Tagged(variant, Arc::new(ExecutionCondition::constructed(
            owner.clone(), [(field, contents)].into(),
        ))).present().borrowed();

        let before = make(ExecutionCondition::Boolean(true));
        let after = make(ExecutionCondition::Boolean(false));
        let loan = owner.borrowed();

        assert_eq!(before.owner_identity(), loan.owner_identity());
        assert_eq!(after.owner_identity(), loan.owner_identity());
        assert_ne!(before, after);
        assert_eq!(before.into_referent().component(ConstantProjectionKind::NullableValue).matches_variant(variant), ExecutionCondition::Boolean(true));
    }
}
