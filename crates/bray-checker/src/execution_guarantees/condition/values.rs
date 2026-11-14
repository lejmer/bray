use crate::execution_guarantees::ExecutionObservationKind as ObservationKind;
use super::{ExecutionCondition, ExecutionConditionKind as K};
use super::model::PreparedCondition;
use bray_bound_tree::BoundOperator;
use std::sync::Arc;

impl ExecutionCondition {
    /// Preserve existing shared structure after a rewrite that changes no meaning.
    pub(crate) fn reuse_unchanged(&self, rewritten: Self) -> Self {
        if self == &rewritten { self.clone() } else { rewritten }
    }

    /// Name new contents by the existing value, independently of an alias slot.
    /// Replaying a mutation retains the defining identity instead of nesting epochs.
    pub(crate) fn post_state(mutation: bray_bound_tree::StorageMutation, identity: Self) -> Self {
        match identity.owner_identity().kind() {
            K::Unknown => ExecutionCondition::unknown(),
            K::PostState(_, identity) => ExecutionCondition::new(K::PostState(mutation, Arc::clone(identity))),
            _ => ExecutionCondition::new(K::PostState(mutation, Arc::new(identity.owner_identity().clone()))),
        }
    }

    /// Retain this descriptor observation through later input substitution.
    pub(crate) fn descriptor(&self, kind: super::ExecutionDescriptorKind) -> Self {
        let source = match kind {
            super::ExecutionDescriptorKind::Reference => self.clone(),
            super::ExecutionDescriptorKind::Value => self.clone().into_referent(),
        };

        let identity = source.descriptor_identity();

        if identity.contains_unknown() { return ExecutionCondition::unknown(); }

        let identity = identity.clone().into_referent();

        match identity.kind() {
            K::Descriptor(..) | K::Unknown => identity,
            _ => ExecutionCondition::new(K::Descriptor(kind, Arc::new(identity))),
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
            bray_symbols::ConstantValueKind::Boolean(value) => ExecutionCondition::boolean(*value),
            _ => ExecutionCondition::new(K::Literal(value)),
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

        match subject.kind() {
            K::Input(place) if place.projections.last().is_some_and(|projection| matches!(
                projection,
                bray_symbols::ConstantProjectionKind::ProductField(_)
                    | bray_symbols::ConstantProjectionKind::UnionPayloadField(_)
            )) => {
                let mut owner = place.clone();

                owner.projections = owner.projections[..owner.projections.len() - 1].into();

                ExecutionCondition::new(K::Input(owner))
            }
            K::Projection(
                bray_symbols::ConstantProjectionKind::ProductField(_)
                    | bray_symbols::ConstantProjectionKind::UnionPayloadField(_),
                parent,
            ) => parent.as_ref().clone(),
            _ => subject,
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
                return ExecutionCondition::unknown();
            }
        }

        ExecutionCondition::new(K::Call(
            callable,
            implementation,
            bray_base::shared_slice(witnesses),
            arguments.into(),
            reference_type,
        ))
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
            ) && !matches!(self.kind(), K::Borrowed(_))
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
        if let K::Variant(variant) = expected.kind() {
            subject.matches_variant(*variant)
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
        let K::Operation(operator, operands) = self.kind() else {
            return None;
        };

        if !matches!(
            (operator, holds),
            (BoundOperator::Equal, true) | (BoundOperator::NotEqual, false)
        ) {
            return None;
        }

        match operands.as_ref() {
            [left, right] => {
                let (subject, absent) = match (left.kind(), right.kind()) {
                    (_, K::Literal(absent)) => (left, absent),
                    (K::Literal(absent), _) => (right, absent),
                    _ => return None,
                };

                matches!(absent.kind(), bray_symbols::ConstantValueKind::NullableAbsent)
                    .then_some((subject, absent))
            }
            _ => None,
        }
    }

    /// Reads nullable payload contents while retaining the extraction's owning observation.
    pub(crate) fn nullable_payload_snapshot(&self) -> Option<&Self> {
        let K::Projection(bray_symbols::ConstantProjectionKind::NullableValue, owner) = self.kind()
        else {
            return None;
        };

        let K::Constructed(_, fields) = owner.kind() else {
            return None;
        };

        fields
            .iter()
            .find(|(field, _)| *field == bray_bound_tree::StorageProjection::NullableValue)
            .map(|(_, value)| value)
    }

    pub(crate) fn present(self) -> Self {
        match self.kind() {
            K::Unknown => ExecutionCondition::unknown(),
            K::Projection(bray_symbols::ConstantProjectionKind::NullableValue, value) => {
                if let Some(snapshot) = self.nullable_payload_snapshot()
                    && (matches!(snapshot.kind(), K::Projection(bray_symbols::ConstantProjectionKind::NullableValue, _))
                        || matches!(snapshot.kind(), K::Input(place) if place.projections.last() == Some(&bray_symbols::ConstantProjectionKind::NullableValue)))
                {
                    return snapshot.clone().present();
                }

                value.as_ref().clone()
            }
            K::Input(place) if place.projections.last() == Some(&bray_symbols::ConstantProjectionKind::NullableValue) => {
                let mut place = place.clone();

                place.projections = place.projections[..place.projections.len() - 1].into();

                ExecutionCondition::new(K::Input(place))
            }
            _ => ExecutionCondition::new(K::Present(Arc::new(self))),
        }
    }

    pub(crate) fn borrowed(self) -> Self {
        if let Some(payload) = self.nullable_payload_snapshot() {
            return payload.clone().borrowed();
        }

        if self == ExecutionCondition::unknown() || self.copied_value().is_reference() {
            // Copying a loan does not create a loan to its private copy context.
            return self.copied_value().clone();
        }

        if self.is_reference_computation() {
            return ExecutionCondition::new(K::Borrowed(Arc::new(self)));
        }

        if let K::Projection(projection, parent) = self.kind() {
            let selected = parent.as_ref().clone().component(*projection);

            if selected != self {
                return selected.borrowed();
            }
        }

        let owner = self.owner_identity();

        if owner.ownership_identity().is_none() {
            return Self::reference_value(ExecutionCondition::unknown(), self);
        }

        if owner == &self {
            ExecutionCondition::new(K::Borrowed(Arc::new(self)))
        } else {
            Self::reference_value(ExecutionCondition::new(K::Borrowed(Arc::new(owner.clone()))), self)
        }
    }

    /// Refresh reached data while retaining the reference value and its existing access.
    pub(crate) fn with_referent(&self, contents: Self) -> Self {
        if self == &contents { return self.clone(); }

        let identity = self.owner_identity();

        if *identity == ExecutionCondition::unknown() { return ExecutionCondition::unknown(); }

        // The observed physical context survives a contents update. The enclosing
        // reference retains access to that context without acquiring ownership.
        let previous = self.clone().into_referent();

        let contents = match previous.kind() {
            K::Observation { owner, kind: kind @ (ObservationKind::Owned | ObservationKind::Scalar), .. } => {
                Self::observation(owner.as_ref().clone(), contents, *kind)
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
        if let K::Present(payload) = base.kind()
            && matches!(fields.as_ref(), [(bray_bound_tree::StorageProjection::NullableValue, value)] if value == payload.as_ref())
        {
            return base;
        }

        if !fields.windows(2).all(|pair| pair[0].0 < pair[1].0) {
            Arc::make_mut(&mut fields).sort_by_key(|(projection, _)| *projection);
        }

        match base.kind() {
            K::Constructed(base, previous) => {
                // Compose existing immutable snapshots once. Later fields replace
                // earlier contents while preserving untouched components.
                let fields = previous.iter().chain(fields.iter()).cloned()
                    .collect::<std::collections::BTreeMap<_, _>>();

                Self::constructed(base.as_ref().clone(), fields.into_iter().collect())
            }
            K::Observation { owner, contents, kind } => Self::observation(
                owner.as_ref().clone(), Self::constructed(contents.as_ref().clone(), fields), *kind,
            ),
            K::Borrowed(contents) => Self::reference_value(
                ExecutionCondition::new(K::Borrowed(Arc::clone(contents))),
                Self::constructed(contents.as_ref().clone(), fields),
            ),
            _ => ExecutionCondition::new(K::Constructed(Arc::new(base), fields)),
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
        Self::scalar_observation(identity.clone(), ExecutionCondition::new(K::ScalarValue(Arc::new(identity))))
    }

    pub(crate) fn value_alias(left: Self, right: Self) -> Self {
        let left = left.copied_value().clone();
        let right = right.copied_value().clone();

        if left.contains_unknown() || right.contains_unknown() { return ExecutionCondition::unknown(); }

        assert!(!left.contains_trusted() && !right.contains_trusted()
            && !left.is_reference() && !right.is_reference(),
            "data aliases require ordinary copied observations: left={left:?}, right={right:?}");

        if left == right { return ExecutionCondition::boolean(true); }

        let (left, right) = if left < right { (left, right) } else { (right, left) };

        ExecutionCondition::new(K::ValueAlias(Arc::new(left), Arc::new(right)))
    }

    pub(crate) fn scalar_observation(owner: Self, contents: Self) -> Self {
        let owner = owner.owner_identity().clone().into_referent().owner_identity().clone();

        Self::observation(owner, contents, ObservationKind::Scalar)
    }

    pub(crate) fn observation(owner: Self, contents: Self, kind: ObservationKind) -> Self {
        let owner = owner.owner_identity();

        if kind == ObservationKind::Owned && owner == &contents
            || kind == ObservationKind::Reference && matches!(owner.kind(), K::Borrowed(referent) if referent.as_ref() == &contents)
        {
            return owner.clone();
        }

        if matches!(contents.kind(), K::Observation { owner: current, kind: current_kind, .. } if current.as_ref() == owner && *current_kind == kind) {
            return contents;
        }

        ExecutionCondition::new(K::Observation { owner: Arc::new(owner.clone()), contents: Arc::new(contents), kind })
    }

    pub(crate) fn observation_kind(&self) -> Option<ObservationKind> {
        match self.kind() {
            K::Observation { kind, .. } => Some(*kind),
            _ => None,
        }
    }

    /// Reached data does not own storage. The reference value retains its access capability.
    pub(crate) fn current_referent(&self) -> Option<&Self> {
        match self.kind() {
            K::Observation { contents, kind: ObservationKind::Reference, .. } => Some(contents),
            _ => None,
        }
    }

    pub(crate) fn is_reference(&self) -> bool {
        matches!(self.kind(), K::Borrowed(_)) || self.current_referent().is_some()
    }

    /// Built-in computations read values rather than their storage or loan identities.
    pub(crate) fn computed_value(&self) -> &Self {
        let mut value = self;

        loop {
            match value.kind() {
                K::Borrowed(contents) | K::Observation { contents, .. } => value = contents,
                _ => return value,
            }
        }
    }

    /// A scalar storage context and its current copied contents are distinct observations.
    pub(crate) fn storage_contents(&self) -> Option<(&Self, &Self)> {
        match self.kind() {
            K::Observation { owner, contents, kind: ObservationKind::Scalar } => Some((owner, contents)),
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
        match self.kind() {
            K::Borrowed(value) | K::Observation { contents: value, kind: ObservationKind::Reference, .. } => {
                value.as_ref().clone().into_referent()
            }
            _ => self,
        }
    }

    /// A typed argument either retains its loan or reads the reached value.
    pub(crate) fn argument_value(self, borrowed: bool) -> Self {
        if borrowed { return self.borrowed(); }

        let mut value = self;

        loop {
            match value.kind() {
                K::Borrowed(contents) | K::Observation {
                    contents, kind: ObservationKind::Reference | ObservationKind::Scalar, ..
                } => value = contents.as_ref().clone(),
                _ => return value,
            }
        }
    }

    pub(crate) fn requiring_trusted_predicates(self) -> Self {
        match self.kind() {
            K::Predicate(_, _, _) => ExecutionCondition::new(K::Trusted(Arc::new(self))),
            K::Operation(operator, operands) => Self::operation(
                *operator,
                operands
                    .iter()
                    .map(|operand| operand.clone().requiring_trusted_predicates())
                    .collect(),
            ),
            _ => self,
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

            if let K::Present(payload) = argument.kind() {
                // Predicate arguments name the nullable value. Structural visitors
                // separately retain its explicit initialized payload snapshot.
                *argument = payload.as_ref().clone().present();
            }

            if let K::Observation { contents, kind: ObservationKind::Reference, .. } = argument.kind()
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
                return ExecutionCondition::unknown();
            }
        }

        ExecutionCondition::new(K::Predicate(predicate, substitution, arguments.into()))
    }

    fn is_reference_computation(&self) -> bool {
        let mut value = self;

        while let K::Projection(_, parent) = value.kind() {
            value = parent;
        }

        matches!(value.kind(), K::Call(_, _, _, _, Some(_)))
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

        if let (K::Constructed(_, fields), StorageProjection::ElementFromEnd(index)) =
            (self.kind(), projection)
        {
            let Some(index) = u32::try_from(fields.len())
                .ok()
                .and_then(|length| length.checked_sub(index.raw() + 1))
            else {
                return ExecutionCondition::unknown();
            };

            return self.project(StorageProjection::ElementFromStart(
                bray_symbols::SymbolOrdinal::new(index),
            ));
        }

        if let K::Observation { owner, contents, kind } = self.kind() {
            let selected = contents.as_ref().clone().project(projection);

            return Self::projected_observation(*kind, selected, || owner.as_ref().clone().project(projection));
        }

        if let K::Borrowed(value) = self.kind() {
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
                return match self.kind() {
                    K::Constructed(_, fields) => fields
                        .iter()
                        .find(|(candidate, _)| *candidate == projection)
                        .map(|(_, value)| value.clone())
                        .unwrap_or_else(ExecutionCondition::unknown),
                    _ => ExecutionCondition::unknown(),
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
            return ExecutionCondition::unknown();
        }

        match self.kind() {
            K::Present(value) if projection == ConstantProjectionKind::NullableValue => {
                value.as_ref().clone()
            }
            K::Input(place) => ExecutionCondition::new(K::Input(place.clone().component(projection))),
            // A field access through a borrow observes the referent's field,
            // rather than projecting a field from the borrow value itself.
            K::Observation { owner, contents, kind } => {
                let selected = contents.as_ref().clone().component(projection);

                Self::projected_observation(*kind, selected, || owner.as_ref().clone().component(projection))
            }
            K::Borrowed(value) => value.as_ref().clone().component(projection).borrowed(),
            K::Tagged(_, value) => value.as_ref().clone().component(projection),
            // Retain the nullable owner's identity when extracting its payload. Rewrapping
            // that payload can then name the acquisition witness without asserting presence.
            K::Constructed(_, _) if projection == ConstantProjectionKind::NullableValue =>
            {
                ExecutionCondition::new(K::Projection(projection, Arc::new(self)))
            }
            K::Projection(ConstantProjectionKind::NullableValue, owner)
                if let K::Constructed(base, fields) = owner.kind() =>
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
            K::Constructed(base, fields) => fields
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
            K::Unknown | K::Discriminant(_) | K::Descriptor(..) => ExecutionCondition::unknown(),
            _ => ExecutionCondition::new(K::Projection(projection, Arc::new(self))),
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
                            if let K::Observation { contents, .. } = observed.kind() {
                                observed = contents;
                            } else if let Some(payload) = observed.nullable_payload_snapshot() {
                                observed = payload;
                            } else {
                                break;
                            }
                        }

                        let K::Constructed(_, fields) = observed.kind() else { return None; };

                        fields.iter().find_map(|(projection, _)|
                            matches!(projection, StorageProjection::ActiveUnionPayloadField { field: candidate, .. } if candidate == field).then_some(*projection)
                        )
                    });

                    let Some(projection) = projection else { return ExecutionCondition::unknown(); };

                    projection
                }
                _ => return ExecutionCondition::unknown(),
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

        if let K::Observation { owner, contents, kind } = self.kind() {
            return Self::observation(owner.as_ref().clone(), contents.with_storage_component(path, value), *kind);
        }

        if let Some(snapshot) = self.nullable_payload_snapshot() {
            let updated = snapshot.with_storage_component(path, value);

            if updated == *snapshot {
                return self.clone();
            }

            let K::Projection(_, owner) = self.kind() else {
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

    fn has_ordinary_complement(&self, terms: &std::collections::BTreeSet<Self>) -> bool {
        let K::Operation(operator, operands) = self.kind() else { return false; };

        if !matches!(operator, BoundOperator::LogicalNot | BoundOperator::NotEqual)
            || operands.contains_trusted() || operands.contains_unknown()
        {
            return false;
        }

        match (operator, operands.as_ref()) {
            (BoundOperator::LogicalNot, [operand]) => terms.contains(operand),
            (BoundOperator::NotEqual, [_, _]) => terms.contains(&ExecutionCondition::new(K::Operation(
                operator.equality_complement().expect("inequality has an exact equality complement"),
                operands.clone(),
            ))),
            _ => false,
        }
    }

    fn boolean_operation(operator: BoundOperator, operands: Vec<Self>) -> Self {
        let conjunction = operator == BoundOperator::LogicalAnd;
        let mut remaining = Self::WORK_LIMIT;
        let mut pending = operands;
        let mut terms = std::collections::BTreeSet::new();

        while let Some(operand) = pending.pop() {
            let Some(next) = remaining.checked_sub(1) else {
                return ExecutionCondition::unknown();
            };

            remaining = next;

            match operand.kind() {
                K::Boolean(value) if *value != conjunction => return ExecutionCondition::boolean(*value),
                K::Boolean(_) => {}
                K::Operation(inner, children) if *inner == operator => {
                    if children.len() > remaining {
                        return ExecutionCondition::unknown();
                    }

                    pending.extend(children.iter().cloned());
                }
                _ => { terms.insert(operand); }
            }
        }

        // Folding must not bypass the work limit of the original binary rebuild.
        if terms.len().saturating_sub(1) > remaining {
            return ExecutionCondition::unknown();
        }

        if terms.iter().any(|term| term.has_ordinary_complement(&terms)) {
            return ExecutionCondition::boolean(!conjunction);
        }

        let mut level = terms.into_iter().collect::<Vec<_>>();

        if level.is_empty() {
            return ExecutionCondition::boolean(conjunction);
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
                            return ExecutionCondition::unknown();
                        };

                        remaining = next_remaining;

                        ExecutionCondition::new(K::Operation(operator, vec![left, right].into()))
                    }
                    None => left,
                });
            }

            level = next;
        }

        level.pop().expect("nonempty Boolean terms retain their result")
    }

    pub(crate) fn operation(operator: BoundOperator, operands: Vec<Self>) -> Self {
        Self::prepare_operation(operator, operands).finish()
    }

    pub(in crate::execution_guarantees) fn prepare_operation(operator: BoundOperator, mut operands: Vec<Self>) -> PreparedCondition {
        for operand in &mut operands {
            *operand = operand.computed_value().clone();
        }

        if matches!(operator, BoundOperator::LogicalAnd | BoundOperator::LogicalOr) {
            return Self::boolean_operation(operator, operands).into();
        }

        if matches!(operator, BoundOperator::Equal | BoundOperator::NotEqual)
            && let [left, right] = operands.as_slice()
        {
            let nullable = match (left.kind(), right.kind()) {
                (_, K::Literal(absent)) => Some((left, absent)),
                (K::Literal(absent), _) => Some((right, absent)),
                _ => None,
            };

            if let Some((value, absent)) = nullable
                && matches!(absent.kind(), bray_symbols::ConstantValueKind::NullableAbsent)
            {
                if let Some(present) = value.known_presence() {
                    return ExecutionCondition::boolean(present == (operator == BoundOperator::NotEqual)).into();
                }

                let tag = match value.kind() {
                    K::Discriminant(base) | K::Constructed(base, _) => Some(base),
                    _ => None,
                };

                if let Some(tag) = tag {
                    return Self::prepare_operation(operator, vec![(**tag).clone(), ExecutionCondition::new(K::Literal(Arc::clone(absent)))]);
                }
            }
        }

        if !matches!(
            operator,
            BoundOperator::LogicalNot | BoundOperator::LogicalAnd | BoundOperator::LogicalOr
        ) && operands.iter().any(Self::contains_unknown)
        {
            return ExecutionCondition::unknown().into();
        }

        if let [value] = operands.as_slice()
            && let K::Literal(value) = value.kind()
            && let Some(kind) = crate::constant::fold_numeric_sign(operator, value.kind())
        {
            return PreparedCondition::new(K::Literal(Arc::new(bray_symbols::ConstantValueData::new(
                value.ty(), kind,
            ))));
        }

        if let [left, right] = operands.as_slice()
            && let (K::Literal(left), K::Literal(right)) = (left.kind(), right.kind())
            && matches!(operator, BoundOperator::Equal | BoundOperator::NotEqual | BoundOperator::Less
                | BoundOperator::LessEqual | BoundOperator::Greater | BoundOperator::GreaterEqual)
            && let Ok(bray_symbols::ConstantValueKind::Boolean(value)) = crate::constant::fold_binary(
                operator, left.kind(), right.kind(), crate::ConstantEvaluationLimits::default().integer_bits(),
            )
        {
            return ExecutionCondition::boolean(value).into();
        }

        if operator == BoundOperator::LogicalNot
            && let [value] = operands.as_slice()
            && let K::Operation(comparison, inner) = value.kind()
            && let Some(complement) = comparison.equality_complement()
        {
            return Self::prepare_operation(complement, inner.to_vec());
        }

        match (operator, operands.as_slice()) {
            // Strict self-ordering is false even for NaN. Equality and weak ordering
            // require value-domain evidence that this untyped normalization does not carry.
            (BoundOperator::Less | BoundOperator::Greater, [left, right]) if left == right => {
                ExecutionCondition::boolean(false).into()
            }
            (BoundOperator::Equal | BoundOperator::NotEqual, [left, right]) => {
                let boolean = match (left.kind(), right.kind()) {
                    (_, K::Boolean(expected)) => Some((left, *expected)),
                    (K::Boolean(expected), _) => Some((right, *expected)),
                    _ => None,
                };

                if let Some((condition, expected)) = boolean {
                    if expected == (operator == BoundOperator::Equal) {
                        return condition.clone().into();
                    }

                    return Self::prepare_operation(BoundOperator::LogicalNot, vec![condition.clone()]);
                }

                PreparedCondition::new(K::Operation(operator, operands.into()))
            }
            (BoundOperator::LogicalNot, [value]) => match value.kind() {
                K::Boolean(value) => ExecutionCondition::boolean(!*value).into(),
                K::Operation(BoundOperator::LogicalNot, inner) => {
                    inner.first().cloned().unwrap_or_else(ExecutionCondition::unknown).into()
                }
                _ => PreparedCondition::new(K::Operation(operator, operands.into())),
            },
            _ => PreparedCondition::new(K::Operation(operator, operands.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ExecutionCondition, ObservationKind, K};
    use crate::ExecutionPlace;
    use bray_bound_tree::StorageProjection;
    use bray_symbols::{ConstantProjectionKind, SymbolId, SymbolOrdinal, UnionVariantSymbolId};
    use std::sync::Arc;

    #[test]
    fn raw_descriptor_reads_follow_current_field_data_after_substitution() {
        use crate::execution_guarantees::ExecutionDescriptorKind as Kind;

        let root = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let owner = ExecutionCondition::new(K::Input(ExecutionPlace::argument(SymbolOrdinal::new(1))));
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let original = owner.clone().project(field);
        let replacement = ExecutionCondition::new(K::Input(ExecutionPlace::argument(SymbolOrdinal::new(2))));
        let template = ExecutionCondition::new(K::Input(root.clone())).project(field).descriptor(Kind::Value);

        for current in [original.clone(), replacement.clone()] {
            let contents = ExecutionCondition::constructed(owner.clone(), [(field, current.clone())].into());
            let actual = ExecutionCondition::reference_value(owner.clone().borrowed(), contents);
            let inputs: std::collections::BTreeMap<_, _> = [(root.clone(), actual)].into();

            let observed = template.substitute(&|place| place.value_in(&inputs).expect("selected input"),
                &ExecutionCondition::unknown(), &mut { ExecutionCondition::WORK_LIMIT });

            assert_eq!(observed, current.descriptor(Kind::Value));
            assert_eq!(observed == original.descriptor(Kind::Value), current == original);
        }
    }

    #[test]
    fn descriptor_selection_survives_substitution_without_observing_reached_contents() {
        let input = |ordinal| ExecutionCondition::new(K::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal))));
        let template = input(0).descriptor(crate::execution_guarantees::ExecutionDescriptorKind::Reference);
        let owner = input(1);

        for contents in [input(2), ExecutionCondition::unknown()] {
            let view = ExecutionCondition::reference_value(owner.clone().borrowed(), contents);
            let copied = ExecutionCondition::scalar_observation(input(3), view.clone());

            assert_eq!(copied.clone().borrowed(), view);

            for actual in [view, copied.clone(), copied.borrowed()] {
                let observed = template.substitute(&|_| actual.clone(), &ExecutionCondition::unknown(),
                    &mut { ExecutionCondition::WORK_LIMIT });

                assert_eq!(observed, owner.descriptor(crate::execution_guarantees::ExecutionDescriptorKind::Reference));
                assert_eq!(observed.descriptor(crate::execution_guarantees::ExecutionDescriptorKind::Reference), observed);
                assert!(!observed.contains_unknown());
                assert!(!observed.observes_live(None));
                assert_eq!(observed.ownership_identity(), None);
            }
        }

        assert_ne!(owner.descriptor(crate::execution_guarantees::ExecutionDescriptorKind::Reference), input(4).descriptor(crate::execution_guarantees::ExecutionDescriptorKind::Reference), "replacement does not retain the old descriptor");
        assert_eq!(ExecutionCondition::unknown().descriptor(crate::execution_guarantees::ExecutionDescriptorKind::Reference), ExecutionCondition::unknown());

        assert_eq!(ExecutionCondition::reference_value(ExecutionCondition::unknown(), input(2)).descriptor(crate::execution_guarantees::ExecutionDescriptorKind::Reference),
            ExecutionCondition::unknown(), "known pointee data cannot supply a missing descriptor identity");

        let observed = owner.descriptor(crate::execution_guarantees::ExecutionDescriptorKind::Reference);
        let contents = [(owner.clone(), input(2))].into();
        let identities = [(owner.clone(), input(4))].into();

        assert_eq!(observed.with_equalities(&contents), observed,
            "equal data cannot equate descriptor identities");

        assert_eq!(observed.with_joined_values(&contents, &Default::default()), observed,
            "replacing reached contents cannot replace the observed descriptor");

        assert_eq!(observed.with_joined_values(&contents, &identities), input(4).descriptor(crate::execution_guarantees::ExecutionDescriptorKind::Reference),
            "a real identity transfer must follow the identity map rather than new contents");
    }

    #[test]
    fn equality_polarity_keeps_guarded_trusted_evidence_composable() {
        use bray_bound_tree::BoundOperator;
        use std::collections::BTreeSet;

        let input = |ordinal| ExecutionCondition::new(K::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal))));
        let predicate = ExecutionCondition::new(K::Trusted(Arc::new(input(2))));

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

        let guard = ExecutionCondition::new(K::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));
        let predicate = ExecutionCondition::new(K::Input(ExecutionPlace::argument(SymbolOrdinal::new(1))));

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

        let first = ExecutionCondition::new(K::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));
        let second = ExecutionCondition::new(K::Input(ExecutionPlace::argument(SymbolOrdinal::new(1))));
        let third = ExecutionCondition::new(K::Input(ExecutionPlace::argument(SymbolOrdinal::new(2))));

        for operator in [BoundOperator::LogicalAnd, BoundOperator::LogicalOr] {
            let combine = |left, right| ExecutionCondition::operation(operator, vec![left, right]);

            assert_eq!(combine(combine(first.clone(), second.clone()), third.clone()),
                combine(third.clone(), combine(second.clone(), first.clone())));

            let unknown_complement = combine(ExecutionCondition::unknown(),
                ExecutionCondition::operation(BoundOperator::LogicalNot, vec![ExecutionCondition::unknown()]));

            assert_eq!(unknown_complement.prove(&BTreeSet::new(), &mut { ExecutionCondition::WORK_LIMIT }), None);

            assert_eq!(ExecutionCondition::operation(operator,
                vec![first.clone(); ExecutionCondition::WORK_LIMIT + 1]), ExecutionCondition::unknown());
        }
    }

    #[test]
    fn ordinary_complements_fold_without_manufacturing_qualified_truth() {
        use bray_bound_tree::BoundOperator;

        let input = |ordinal| ExecutionCondition::new(K::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal))));
        let negate = |value| ExecutionCondition::operation(BoundOperator::LogicalNot, vec![value]);
        let compare = |operator| ExecutionCondition::operation(operator, vec![input(0), input(1)]);
        let trusted = ExecutionCondition::new(K::Trusted(Arc::new(input(2))));

        for operator in [BoundOperator::LogicalAnd, BoundOperator::LogicalOr] {
            let absorbing = ExecutionCondition::boolean(operator == BoundOperator::LogicalOr);

            for (positive, negative) in [
                (input(0), negate(input(0))),
                (compare(BoundOperator::Equal), compare(BoundOperator::NotEqual)),
            ] {
                assert_eq!(ExecutionCondition::operation(operator, vec![
                    positive.clone(), trusted.clone(), negative.clone(),
                ]), absorbing);

                assert_eq!(ExecutionCondition::operation(operator, vec![
                    negative, ExecutionCondition::operation(operator, vec![trusted.clone(), positive]),
                ]), absorbing);
            }

            for (positive, negative) in [
                (trusted.clone(), negate(trusted.clone())),
                (input(2), negate(trusted.clone())),
                (ExecutionCondition::unknown(), negate(ExecutionCondition::unknown())),
                (compare(BoundOperator::Less), compare(BoundOperator::GreaterEqual)),
            ] {
                assert!(!matches!(ExecutionCondition::operation(operator, vec![positive, negative]).kind(),
                    K::Boolean(_)), "qualified, unknown and unordered pairs need evidence");
            }

            let mut terms = (0..ExecutionCondition::WORK_LIMIT / 2)
                .map(|ordinal| input(u32::try_from(ordinal).unwrap()))
                .collect::<Vec<_>>();

            terms.push(negate(input(0)));

            assert_eq!(ExecutionCondition::operation(operator, terms), ExecutionCondition::unknown(),
                "a complement cannot bypass the original binary rebuild budget");
        }
    }

    #[test]
    fn scalar_and_reference_projections_keep_their_roles() {
        let owner = ExecutionCondition::new(K::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));
        let child = ExecutionCondition::new(K::Input(ExecutionPlace::argument(SymbolOrdinal::new(1))));
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
            ExecutionCondition::unknown(), [(field, descriptor.clone())].into(),
        ));

        assert_eq!(copied_views.project(field), descriptor);
    }

    #[test]
    fn shape_decorations_keep_data_out_of_stable_loan_identity() {
        let owner = ExecutionCondition::new(K::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let variant = UnionVariantSymbolId::from_symbol_id(SymbolId::new(900));

        let make = |contents| ExecutionCondition::new(K::Tagged(variant, Arc::new(ExecutionCondition::constructed(
            owner.clone(), [(field, contents)].into(),
        )))).present().borrowed();

        let before = make(ExecutionCondition::boolean(true));
        let after = make(ExecutionCondition::boolean(false));
        let loan = owner.borrowed();

        assert_eq!(before.owner_identity(), loan.owner_identity());
        assert_eq!(after.owner_identity(), loan.owner_identity());
        assert_ne!(before, after);
        assert_eq!(before.into_referent().component(ConstantProjectionKind::NullableValue).matches_variant(variant), ExecutionCondition::boolean(true));
    }

    #[test]
    fn prepared_boolean_normalization_preserves_exhaustion() {
        use bray_bound_tree::BoundOperator::LogicalAnd;

        let terms = (0..ExecutionCondition::WORK_LIMIT / 2)
            .map(|ordinal| ExecutionCondition::new(K::Input(ExecutionPlace::argument(
                SymbolOrdinal::new(u32::try_from(ordinal).expect("test ordinal fits")),
            ))))
            .collect::<Vec<_>>();

        let balanced = ExecutionCondition::prepare_operation(LogicalAnd, terms).finish();

        let K::Operation(LogicalAnd, operands) = balanced.kind() else {
            panic!("the flat input fits the Boolean normalization budget");
        };

        assert_eq!(ExecutionCondition::prepare_operation(LogicalAnd, operands.to_vec()).finish(), ExecutionCondition::unknown());
    }

}
