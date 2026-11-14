use crate::execution_guarantees::ExecutionObservationKind as ObservationKind;
use super::ExecutionCondition;
use super::traversal::{NodeIdentity, visit_once};
use super::retention::Set;
use std::collections::HashSet;

impl ExecutionCondition {
    pub(crate) fn selector_place(&self) -> Option<&crate::ExecutionPlace> {
        match self.kind() {
            crate::ExecutionConditionKind::ScalarValue(origin) => origin.selector_place(),
            crate::ExecutionConditionKind::Joined(_, _, place) => Some(place),
            crate::ExecutionConditionKind::PostState(_, origin) => match (origin.as_ref()).kind() {
                crate::ExecutionConditionKind::Input(place) => Some(place),
                _ => None,
            },
            _ => None,
        }
    }

    /// Reading a stored reference preserves its own referent instead of rebasing it on the container.
    pub(super) fn projected_observation(
        kind: ObservationKind,
        selected: Self,
        owner: impl FnOnce() -> Self,
    ) -> Self {
        if kind != ObservationKind::Owned
            && selected.ownership_identity().is_some_and(|(role, _)| role == ObservationKind::Reference)
        {
            return selected;
        }

        match kind {
            ObservationKind::Owned => selected,
            ObservationKind::Scalar => Self::scalar_observation(owner(), selected),
            ObservationKind::Reference if selected.observation_kind() == Some(ObservationKind::Scalar) => {
                Self::scalar_observation(owner(), selected)
            }
            ObservationKind::Reference => Self::reference_value(owner().borrowed(), selected),
        }
    }

    /// Expires one finite evaluation while retaining independent owned snapshots.
    pub(crate) fn without_evaluation(
        &self,
        mutation: bray_bound_tree::StorageMutation,
        remaining: &mut usize,
    ) -> Self {
        if *remaining == 0 {
            return ExecutionCondition::unknown();
        }

        *remaining -= 1;

        if !self.depends_on(mutation) {
            return self.clone();
        }

        let rewritten = match self.kind() {
            crate::ExecutionConditionKind::Constructed(base, fields) => Self::constructed(
                base.without_evaluation(mutation, remaining),
                fields
                    .iter()
                    .map(|(projection, value)| {
                        (*projection, value.without_evaluation(mutation, remaining))
                    })
                    .collect(),
            ),
            crate::ExecutionConditionKind::Observation { owner, contents, kind }
                if *kind == ObservationKind::Owned
                    || *kind == ObservationKind::Reference && !owner.depends_on(mutation) =>
            {
                Self::observation(
                    owner.without_evaluation(mutation, remaining),
                    contents.without_evaluation(mutation, remaining),
                    *kind,
                )
            }
            crate::ExecutionConditionKind::Discriminant(value) => value.without_evaluation(mutation, remaining).discriminant(),
            crate::ExecutionConditionKind::Descriptor(kind, value) => value.without_evaluation(mutation, remaining).descriptor(*kind),
            crate::ExecutionConditionKind::Present(value) => ExecutionCondition::new(crate::ExecutionConditionKind::Present(std::sync::Arc::new(value.without_evaluation(mutation, remaining)))),
            crate::ExecutionConditionKind::Tagged(variant, value) => ExecutionCondition::new(crate::ExecutionConditionKind::Tagged(
                *variant,
                std::sync::Arc::new(value.without_evaluation(mutation, remaining)),
            )),
            // Scalar and proof terms cannot preserve their meaning after any
            // contributing evaluation expires. A retired reference also loses
            // its reached view, which never independently owned those contents.
            _ => ExecutionCondition::unknown(),
        };

        self.reuse_unchanged(rewritten)
    }

    /// Closed memory predicates observe pointer descriptors and current scalar extents.
    pub(crate) fn pointer_observations(
        &self,
        predicates: &std::collections::BTreeMap<bray_symbols::PredicateDefinitionSymbolId, &'static [usize]>,
    ) -> Self {
        let rewritten = match self.kind() {
            crate::ExecutionConditionKind::Trusted(value) => {
                ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(std::sync::Arc::new(value.pointer_observations(predicates))))
            }
            crate::ExecutionConditionKind::Operation(operator, operands) => ExecutionCondition::new(crate::ExecutionConditionKind::Operation(
                *operator,
                operands
                    .iter()
                    .map(|value| value.pointer_observations(predicates))
                    .collect(),
            )),
            crate::ExecutionConditionKind::Predicate(predicate, substitution, arguments)
                if let Some(pointers) = predicates.get(predicate) =>
            {
                Self::predicate(
                    *predicate,
                    *substitution,
                    arguments
                        .iter()
                        .enumerate()
                        .map(|(index, argument)| {
                            if !pointers.contains(&index) {
                                return argument.computed_value().clone();
                            }

                            argument.descriptor_identity().clone()
                        })
                        .collect(),
                )
            }
            _ => self.clone(),
        };

        self.reuse_unchanged(rewritten)
    }

    /// The copied descriptor, independently of its scalar slot and reached contents.
    pub(crate) fn descriptor_identity(&self) -> &Self {
        let mut pointer = self.copied_value();

        // An owning context can survive replacement of the address stored in it.
        // Read that descriptor's current data before selecting its identity.
        while let crate::ExecutionConditionKind::Observation { contents, kind: ObservationKind::Owned, .. } = pointer.kind() {
            pointer = contents.copied_value();
        }

        while let crate::ExecutionConditionKind::Borrowed(reached) = pointer.kind() {
            pointer = reached.copied_value();
        }

        pointer.owner_identity()
    }

    /// The carrier's identity survives changes to the component snapshot.
    pub(crate) fn owner_identity(&self) -> &Self {
        let mut value = self;

        loop {
            match value.kind() {
                crate::ExecutionConditionKind::Constructed(base, _) => value = base,
                crate::ExecutionConditionKind::Observation { owner: base, .. } if **base != ExecutionCondition::unknown() => value = base,
                crate::ExecutionConditionKind::Tagged(_, base) | crate::ExecutionConditionKind::Present(base) => value = base,
                _ => {
                    if let Some(payload) = value.nullable_payload_snapshot() {
                        value = payload;
                    } else {
                        break;
                    }
                }
            }
        }

        value
    }

    /// Selects carrier authority independently of general data and physical identity.
    pub(crate) fn ownership_identity(&self) -> Option<(ObservationKind, &Self)> {
        let mut value = self;
        let identity = self.owner_identity();
        let mut role = ObservationKind::Owned;

        for _ in 0..Self::WORK_LIMIT {
            match value.kind() {
                crate::ExecutionConditionKind::Observation { kind: ObservationKind::Scalar, .. } => return None,
                crate::ExecutionConditionKind::Observation { owner, kind: ObservationKind::Reference, .. } => {
                    role = ObservationKind::Reference;
                    value = owner;
                }
                crate::ExecutionConditionKind::Borrowed(referent) => {
                    role = ObservationKind::Reference;
                    value = referent;
                }
                crate::ExecutionConditionKind::Observation { owner, kind: ObservationKind::Owned, .. }
                | crate::ExecutionConditionKind::Constructed(owner, _)
                | crate::ExecutionConditionKind::Tagged(_, owner)
                | crate::ExecutionConditionKind::Present(owner) => value = owner,
                crate::ExecutionConditionKind::Projection(_, parent) => {
                    value = value.nullable_payload_snapshot().unwrap_or(parent);
                }
                crate::ExecutionConditionKind::Input(_) | crate::ExecutionConditionKind::Expression(_) | crate::ExecutionConditionKind::ScopedCapability(_)
                | crate::ExecutionConditionKind::PostState(_, _) | crate::ExecutionConditionKind::Joined(_, _, _) | crate::ExecutionConditionKind::Result
                | crate::ExecutionConditionKind::CapturedInput { .. } => return Some((role, identity)),
                _ => return None,
            }
        }

        None
    }

    /// Whether current observations belong to the same owned value or loan.
    pub(crate) fn has_same_owner(&self, other: &Self) -> bool {
        self.owner_match_key() == other.owner_match_key()
    }

    pub(super) fn owner_match_key(&self) -> (bool, &Self) {
        let owner = self.owner_identity();

        match owner.kind() {
            crate::ExecutionConditionKind::Borrowed(referent) => (true, referent.owner_identity()),
            _ => (false, owner),
        }
    }

    pub(crate) fn observation_path(
        &self,
    ) -> Option<(Self, Vec<bray_symbols::ConstantProjectionKind>)> {
        let mut value = self;
        let mut path = Vec::new();

        loop {
            match value.kind() {
                crate::ExecutionConditionKind::Projection(component, parent) => {
                    path.push(*component);
                    value = parent;
                }
                crate::ExecutionConditionKind::Input(place) => {
                    path.reverse();

                    let mut full_path = place.projections.to_vec();

                    full_path.extend(path);

                    let mut root = place.clone();

                    root.projections = std::sync::Arc::new([]);

                    return Some((ExecutionCondition::new(crate::ExecutionConditionKind::Input(root)), full_path));
                }
                crate::ExecutionConditionKind::Constructed(base, _) | crate::ExecutionConditionKind::Observation { owner: base, .. } if **base != ExecutionCondition::unknown() => value = base,
                crate::ExecutionConditionKind::Tagged(_, base) | crate::ExecutionConditionKind::Present(base) => value = base,
                crate::ExecutionConditionKind::Expression(_)
                | crate::ExecutionConditionKind::ScopedCapability(_)
                | crate::ExecutionConditionKind::PostState(_, _)
                | crate::ExecutionConditionKind::Joined(_, _, _)
                | crate::ExecutionConditionKind::CapturedInput { .. }
                | crate::ExecutionConditionKind::Result => {
                    path.reverse();

                    return Some((value.clone(), path));
                }
                _ => return None,
            }
        }
    }

    fn overlaps_observation(&self, other: &Self) -> bool {
        match (self.observation_path(), other.observation_path()) {
            (Some((root, path)), Some((other_root, other_path))) => {
                root == other_root
                    && (path.starts_with(&other_path) || other_path.starts_with(&path))
            }
            _ => false,
        }
    }

    pub(crate) fn is_owned_projection_of(&self, owner: &Self) -> bool {
        let Some((role, owner)) = owner.ownership_identity() else { return false; };

        let Some((own_role, _)) = self.ownership_identity() else { return false; };

        if role != own_role {
            return false;
        }

        if role == ObservationKind::Reference {
            return matches!((Set::indexed_path(self), Set::indexed_path(owner)),
                (Some((root, path)), Some((other, prefix))) if root == other && path.starts_with(&prefix));
        }

        self.ownership_ancestors().any(|value| {
            value == owner || matches!((value.kind(), owner.kind()), (crate::ExecutionConditionKind::Input(value), crate::ExecutionConditionKind::Input(owner)) if owner.contains(value))
        })
    }

    /// Traverses containing value observations and captured frame identities.
    pub(crate) fn ownership_ancestors(&self) -> impl Iterator<Item = &Self> {
        std::iter::successors(Some(self.owner_identity()), |value| match value.kind() {
            crate::ExecutionConditionKind::Projection(_, parent)
            | crate::ExecutionConditionKind::Tagged(_, parent)
            | crate::ExecutionConditionKind::Present(parent)
            | crate::ExecutionConditionKind::CapturedInput { owner: parent, .. } => Some(parent.owner_identity()),
            _ => None,
        })
    }

    /// Returns the exact input places observed by this checked condition.
    pub fn inputs(&self) -> Vec<&crate::execution_guarantees::ExecutionPlace> {
        let mut pending = vec![self];
        let mut inputs = Vec::new();

        while let Some(condition) = pending.pop() {
            match condition.kind() {
                crate::ExecutionConditionKind::Input(place) => inputs.push(place),
                crate::ExecutionConditionKind::Operation(_, operands)
                | crate::ExecutionConditionKind::Predicate(_, _, operands)
                | crate::ExecutionConditionKind::Call(_, _, _, operands, _) => pending.extend(operands.iter()),
                crate::ExecutionConditionKind::Tagged(_, value)
                | crate::ExecutionConditionKind::VariantTest(_, value)
                | crate::ExecutionConditionKind::Projection(_, value)
                | crate::ExecutionConditionKind::Trusted(value)
                | crate::ExecutionConditionKind::Borrowed(value)
                | crate::ExecutionConditionKind::Descriptor(_, value)
                | crate::ExecutionConditionKind::Present(value)
                | crate::ExecutionConditionKind::Entry {
                    condition: value, ..
                }
                | crate::ExecutionConditionKind::CapturedInput { owner: value, .. } => pending.push(value),
                crate::ExecutionConditionKind::Observation { owner, contents: value, .. } => pending.extend([owner.as_ref(), value.as_ref()]),
                crate::ExecutionConditionKind::Constructed(base, fields) => {
                    pending.push(base);

                    pending.extend(fields.iter().map(|(_, value)| value))
                }
                _ => {}
            }
        }

        inputs
    }

    /// Component paths explicitly retained by this snapshot.
    pub(crate) fn component_projections(&self) -> std::collections::BTreeSet<bray_bound_tree::StorageProjection> {
        let mut value = self.computed_value();

        while let Some(snapshot) = value.nullable_payload_snapshot() {
            value = snapshot.computed_value();
        }

        match value.kind() {
            crate::ExecutionConditionKind::Constructed(_, fields) => fields.iter().map(|(field, _)| *field).collect(),
            crate::ExecutionConditionKind::Present(_) => [bray_bound_tree::StorageProjection::NullableValue].into(),
            crate::ExecutionConditionKind::Literal(value) if matches!(value.kind(), bray_symbols::ConstantValueKind::NullableAbsent) =>
                [bray_bound_tree::StorageProjection::NullableValue].into(),
            _ => Default::default(),
        }
    }

    pub(crate) fn owned_components(&self) -> impl Iterator<Item = &Self> {
        let present = match self.kind() {
            crate::ExecutionConditionKind::Observation { contents: value, kind: ObservationKind::Owned, .. }
                if !self.ownership_identity().is_some_and(|(role, _)| role == ObservationKind::Reference) => Some(value.as_ref()),
            crate::ExecutionConditionKind::Present(value)
            | crate::ExecutionConditionKind::Tagged(_, value) => Some(value.as_ref()),
            crate::ExecutionConditionKind::Projection(bray_symbols::ConstantProjectionKind::NullableValue, value)
                if self.ownership_identity().is_some_and(|(role, _)| role == ObservationKind::Owned) => Some(value.as_ref()),
            _ => None,
        };

        let fields = match self.kind() {
            crate::ExecutionConditionKind::Constructed(_, fields) => fields.as_ref(),
            _ => &[],
        };

        present
            .into_iter()
            .chain(fields.iter().map(|(_, value)| value))
    }

    /// Returns independent clauses while preserving the guards within each clause.
    pub fn conjuncts(&self) -> impl Iterator<Item = &Self> {
        let mut pending = vec![self];

        std::iter::from_fn(move || {
            loop {
                let condition = pending.pop()?;

                if let crate::ExecutionConditionKind::Operation(bray_bound_tree::BoundOperator::LogicalAnd, operands) =
                    condition.kind()
                {
                    pending.extend(operands.iter());
                } else {
                    return Some(condition);
                }
            }
        })
    }

    /// Whether every observed value is in the established unchanged set.
    pub(crate) fn observes_only(&self, stable: impl Fn(&Self) -> bool) -> bool {
        let mut pending = vec![self];
        let mut visited = HashSet::new();

        while let Some(value) = pending.pop() {
            if !visit_once(&mut visited, NodeIdentity(value), !pending.is_empty()) { continue; }

            if stable(value) {
                continue;
            }

            match value.kind() {
                crate::ExecutionConditionKind::ValueAlias(left, right) => pending.extend([left.as_ref(), right.as_ref()]),
                crate::ExecutionConditionKind::Boolean(_)
                | crate::ExecutionConditionKind::Literal(_)
                | crate::ExecutionConditionKind::Variant(_)
                | crate::ExecutionConditionKind::InitializedSnapshot(_, _, _)
                | crate::ExecutionConditionKind::Entry { captured: true, .. }
                | crate::ExecutionConditionKind::Descriptor(..) => {}
                crate::ExecutionConditionKind::Entry {
                    condition,
                    captured: false,
                } => pending.push(condition),
                crate::ExecutionConditionKind::CapturedInput { owner, .. } => pending.push(owner),
                crate::ExecutionConditionKind::Borrowed(_) | crate::ExecutionConditionKind::Observation { kind: ObservationKind::Reference, .. } => return false,
                crate::ExecutionConditionKind::Observation { owner, contents: value, .. } => pending.extend([owner.as_ref(), value.as_ref()]),
                crate::ExecutionConditionKind::Tagged(_, value)
                | crate::ExecutionConditionKind::VariantTest(_, value)
                | crate::ExecutionConditionKind::Projection(_, value)
                | crate::ExecutionConditionKind::Trusted(value)
                | crate::ExecutionConditionKind::Present(value) => pending.push(value),
                crate::ExecutionConditionKind::Predicate(_, _, values)
                | crate::ExecutionConditionKind::Call(_, _, _, values, _)
                | crate::ExecutionConditionKind::Operation(_, values) => pending.extend(values.iter()),
                _ => return false,
            }
        }

        true
    }

    pub(crate) fn observations(&self) -> impl Iterator<Item = &Self> {
        self.observations_except(|_| false)
    }

    pub(crate) fn observations_except(
        &self,
        exclude: impl Fn(&Self) -> bool,
    ) -> impl Iterator<Item = &Self> {
        let mut pending = vec![self];
        let mut visited = HashSet::new();

        std::iter::from_fn(move || {
            loop {
                let condition = pending.pop()?;

                if !visit_once(&mut visited, NodeIdentity(condition), !pending.is_empty()) || exclude(condition) {
                    continue;
                }

                match condition.kind() {
                    crate::ExecutionConditionKind::ValueAlias(left, right) => pending.extend([left.as_ref(), right.as_ref()]),
                    crate::ExecutionConditionKind::Operation(_, operands)
                    | crate::ExecutionConditionKind::Predicate(_, _, operands)
                    | crate::ExecutionConditionKind::Call(_, _, _, operands, _) => {
                        pending.extend(operands.iter());
                    }
                    crate::ExecutionConditionKind::Tagged(_, subject)
                    | crate::ExecutionConditionKind::VariantTest(_, subject)
                    | crate::ExecutionConditionKind::Trusted(subject)
                    | crate::ExecutionConditionKind::Borrowed(subject)
                    | crate::ExecutionConditionKind::Present(subject) => {
                        pending.push(subject);
                    }
                    crate::ExecutionConditionKind::Entry {
                        condition,
                        captured: false,
                    } => pending.push(condition),
                    crate::ExecutionConditionKind::Observation { owner, contents: value, .. } => pending.extend([owner.as_ref(), value.as_ref()]),
                    crate::ExecutionConditionKind::Constructed(base, fields) => {
                        pending.push(base);
                        pending.extend(fields.iter().map(|(_, value)| value));
                    }
                    _ => {}
                }

                return Some(condition);
            }
        })
    }

    pub(crate) fn observes(&self, value: &Self) -> bool {
        self.observations()
            .any(|condition| condition == value || condition.overlaps_observation(value))
    }

    /// Observes an access subject through trusted claims, excluding ordinary guards.
    pub(crate) fn observes_trusted(&self, value: &Self) -> bool {
        self.observations()
            .any(|condition| matches!(condition.kind(), crate::ExecutionConditionKind::Trusted(_)) && condition.observes(value))
    }

    /// Reports whether a completion clause observes its result. Unknown meaning
    /// conservatively retains result authority until the owning query resolves it.
    pub fn observes_completion_result(&self) -> bool {
        self.observations().any(|condition| {
            condition.is_unknown() || condition.is_owned_projection_of(&ExecutionCondition::result())
        })
    }

    pub(crate) fn observes_live(&self, changed: Option<&Self>) -> bool {
        self.observes_live_where(|value| changed.is_none_or(|changed| value.observes(changed)), |_| false)
    }

    /// Inspect live borrowed observations and uncaptured entry guards, retaining excluded contexts.
    pub(crate) fn observes_live_where(&self, observes: impl Fn(&Self) -> bool, exclude: impl Fn(&Self) -> bool) -> bool {
        let mut pending = vec![(self, false)];
        let mut visited = HashSet::new();

        while let Some((condition, live)) = pending.pop() {
            // A shared node reached through a loan must still be inspected as live.
            if !visit_once(&mut visited, (NodeIdentity(condition), live), !pending.is_empty()) { continue; }

            if exclude(condition) { continue; }

            if live && condition.observation_path().is_some() {
                if observes(condition) {
                    return true;
                }

                continue;
            }

            match condition.kind() {
                crate::ExecutionConditionKind::Descriptor(..) => {},
                crate::ExecutionConditionKind::ValueAlias(left, right) => pending.extend([(left.as_ref(), live), (right.as_ref(), live)]),
                crate::ExecutionConditionKind::Borrowed(value) => pending.push((value, true)),
                crate::ExecutionConditionKind::Entry {
                    condition,
                    captured: false,
                } => pending.push((condition, true)),
                crate::ExecutionConditionKind::Operation(_, operands)
                | crate::ExecutionConditionKind::Predicate(_, _, operands)
                | crate::ExecutionConditionKind::Call(_, _, _, operands, _) => {
                    pending.extend(operands.iter().map(|value| (value, live)))
                }
                crate::ExecutionConditionKind::Tagged(_, value)
                | crate::ExecutionConditionKind::VariantTest(_, value)
                | crate::ExecutionConditionKind::Trusted(value)
                | crate::ExecutionConditionKind::Projection(_, value)
                | crate::ExecutionConditionKind::Present(value) => pending.push((value, live)),
                crate::ExecutionConditionKind::Observation { owner, contents, kind } => {
                    pending.push((owner, live));
                    pending.push((contents, live || *kind == ObservationKind::Reference));
                }
                crate::ExecutionConditionKind::Constructed(base, fields) => {
                    pending.push((base, live));

                    pending.extend(fields.iter().map(|(_, value)| (value, live)));
                }
                _ => {}
            }
        }

        false
    }
}
