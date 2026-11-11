use crate::execution_guarantees::ExecutionObservationKind as ObservationKind;
use super::ExecutionCondition;

impl ExecutionCondition {
    pub(crate) fn selector_place(&self) -> Option<&crate::ExecutionPlace> {
        match self {
            Self::ScalarValue(origin) => origin.selector_place(),
            Self::Joined(_, _, place) => Some(place),
            Self::PostState(_, origin) => match origin.as_ref() {
                Self::Input(place) => Some(place),
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
            return Self::Unknown;
        }

        *remaining -= 1;

        if !self.depends_on(mutation) {
            return self.clone();
        }

        match self {
            Self::Constructed(base, fields) => Self::constructed(
                base.without_evaluation(mutation, remaining),
                fields
                    .iter()
                    .map(|(projection, value)| {
                        (*projection, value.without_evaluation(mutation, remaining))
                    })
                    .collect(),
            ),
            Self::Observation { owner, contents, kind }
                if *kind == ObservationKind::Owned
                    || *kind == ObservationKind::Reference && !owner.depends_on(mutation) =>
            {
                Self::observation(
                    owner.without_evaluation(mutation, remaining),
                    contents.without_evaluation(mutation, remaining),
                    *kind,
                )
            }
            Self::Present(value) => value.without_evaluation(mutation, remaining).present(),
            Self::Tagged(variant, value) => Self::Tagged(
                *variant,
                std::sync::Arc::new(value.without_evaluation(mutation, remaining)),
            ),
            // Scalar and proof terms cannot preserve their meaning after any
            // contributing evaluation expires. A retired reference also loses
            // its reached view, which never independently owned those contents.
            _ => Self::Unknown,
        }
    }

    /// Closed memory predicates observe pointer descriptors and current scalar extents.
    pub(crate) fn pointer_observations(
        &self,
        predicates: &std::collections::BTreeMap<bray_symbols::PredicateDefinitionSymbolId, &'static [usize]>,
    ) -> Self {
        match self {
            Self::Trusted(value) => {
                Self::Trusted(std::sync::Arc::new(value.pointer_observations(predicates)))
            }
            Self::Operation(operator, operands) => Self::Operation(
                *operator,
                operands
                    .iter()
                    .map(|value| value.pointer_observations(predicates))
                    .collect(),
            ),
            Self::Predicate(predicate, substitution, arguments)
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

                            argument.pointer_identity().clone()
                        })
                        .collect(),
                )
            }
            _ => self.clone(),
        }
    }

    /// The copied address descriptor, independently of the scalar slot holding it.
    pub(crate) fn pointer_identity(&self) -> &Self {
        let mut pointer = self.copied_value();

        while let Self::Borrowed(reached) = pointer {
            pointer = reached.copied_value();
        }

        pointer.owner_identity()
    }

    /// The carrier's identity survives changes to the component snapshot.
    pub(crate) fn owner_identity(&self) -> &Self {
        let mut value = self;

        loop {
            match value {
                Self::Constructed(base, _) => value = base,
                Self::Observation { owner: base, .. } if **base != Self::Unknown => value = base,
                Self::Tagged(_, base) | Self::Present(base) => value = base,
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
            match value {
                Self::Observation { kind: ObservationKind::Scalar, .. } => return None,
                Self::Observation { owner, kind: ObservationKind::Reference, .. } => {
                    role = ObservationKind::Reference;
                    value = owner;
                }
                Self::Borrowed(referent) => {
                    role = ObservationKind::Reference;
                    value = referent;
                }
                Self::Observation { owner, kind: ObservationKind::Owned, .. }
                | Self::Constructed(owner, _)
                | Self::Tagged(_, owner)
                | Self::Present(owner) => value = owner,
                Self::Projection(_, parent) => {
                    value = value.nullable_payload_snapshot().unwrap_or(parent);
                }
                Self::Input(_) | Self::Expression(_) | Self::ScopedCapability(_)
                | Self::PostState(_, _) | Self::Joined(_, _, _) | Self::Result
                | Self::CapturedInput { .. } => return Some((role, identity)),
                _ => return None,
            }
        }

        None
    }

    /// Whether current observations belong to the same owned value or loan.
    pub(crate) fn has_same_owner(&self, other: &Self) -> bool {
        match (self.owner_identity(), other.owner_identity()) {
            (Self::Borrowed(left), Self::Borrowed(right)) => {
                left.owner_identity() == right.owner_identity()
            }
            (left, right) => left == right,
        }
    }

    pub(crate) fn observation_path(
        &self,
    ) -> Option<(Self, Vec<bray_symbols::ConstantProjectionKind>)> {
        let mut value = self;
        let mut path = Vec::new();

        loop {
            match value {
                Self::Projection(component, parent) => {
                    path.push(*component);
                    value = parent;
                }
                Self::Input(place) => {
                    path.reverse();

                    let mut full_path = place.projections.to_vec();

                    full_path.extend(path);

                    let mut root = place.clone();

                    root.projections = std::sync::Arc::new([]);

                    return Some((Self::Input(root), full_path));
                }
                Self::Constructed(base, _) | Self::Observation { owner: base, .. } if **base != Self::Unknown => value = base,
                Self::Tagged(_, base) | Self::Present(base) => value = base,
                Self::Expression(_)
                | Self::ScopedCapability(_)
                | Self::PostState(_, _)
                | Self::Joined(_, _, _)
                | Self::CapturedInput { .. }
                | Self::Result => {
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
            value == owner || matches!((value, owner), (Self::Input(value), Self::Input(owner)) if owner.contains(value))
        })
    }

    /// Traverses containing value observations and captured frame identities.
    pub(crate) fn ownership_ancestors(&self) -> impl Iterator<Item = &Self> {
        std::iter::successors(Some(self.owner_identity()), |value| match value {
            Self::Projection(_, parent)
            | Self::Tagged(_, parent)
            | Self::Present(parent)
            | Self::CapturedInput { owner: parent, .. } => Some(parent.owner_identity()),
            _ => None,
        })
    }

    /// Returns the exact input places observed by this checked condition.
    pub fn inputs(&self) -> Vec<&crate::execution_guarantees::ExecutionPlace> {
        let mut pending = vec![self];
        let mut inputs = Vec::new();

        while let Some(condition) = pending.pop() {
            match condition {
                Self::Input(place) => inputs.push(place),
                Self::Operation(_, operands)
                | Self::Predicate(_, _, operands)
                | Self::Call(_, _, _, operands, _) => pending.extend(operands.iter()),
                Self::Tagged(_, value)
                | Self::VariantTest(_, value)
                | Self::Projection(_, value)
                | Self::Trusted(value)
                | Self::Borrowed(value)
                | Self::Present(value)
                | Self::Entry {
                    condition: value, ..
                }
                | Self::CapturedInput { owner: value, .. } => pending.push(value),
                Self::Observation { owner, contents: value, .. } => pending.extend([owner.as_ref(), value.as_ref()]),
                Self::Constructed(base, fields) => {
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
        let mut value = self;
        while let Some(snapshot) = value.nullable_payload_snapshot() {
            value = snapshot;
        }

        match value {
            Self::Constructed(_, fields) => fields.iter().map(|(field, _)| *field).collect(),
            Self::Present(_) => [bray_bound_tree::StorageProjection::NullableValue].into(),
            Self::Literal(value) if matches!(value.kind(), bray_symbols::ConstantValueKind::NullableAbsent) =>
                [bray_bound_tree::StorageProjection::NullableValue].into(),
            _ => Default::default(),
        }
    }

    pub(crate) fn owned_components(&self) -> impl Iterator<Item = &Self> {
        let present = match self {
            Self::Observation { contents: value, kind: ObservationKind::Owned, .. }
                if !self.ownership_identity().is_some_and(|(role, _)| role == ObservationKind::Reference) => Some(value.as_ref()),
            Self::Present(value)
            | Self::Tagged(_, value) => Some(value.as_ref()),
            Self::Projection(bray_symbols::ConstantProjectionKind::NullableValue, value)
                if self.ownership_identity().is_some_and(|(role, _)| role == ObservationKind::Owned) => Some(value.as_ref()),
            _ => None,
        };

        let fields = match self {
            Self::Constructed(_, fields) => fields.as_ref(),
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

                if let Self::Operation(bray_bound_tree::BoundOperator::LogicalAnd, operands) =
                    condition
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

        while let Some(value) = pending.pop() {
            if stable(value) {
                continue;
            }

            match value {
                Self::Boolean(_)
                | Self::Literal(_)
                | Self::Variant(_)
                | Self::InitializedSnapshot(_, _, _)
                | Self::Entry { captured: true, .. } => {}
                Self::Entry {
                    condition,
                    captured: false,
                } => pending.push(condition),
                Self::CapturedInput { owner, .. } => pending.push(owner),
                Self::Borrowed(_) | Self::Observation { kind: ObservationKind::Reference, .. } => return false,
                Self::Observation { owner, contents: value, .. } => pending.extend([owner.as_ref(), value.as_ref()]),
                Self::Tagged(_, value)
                | Self::VariantTest(_, value)
                | Self::Projection(_, value)
                | Self::Trusted(value)
                | Self::Present(value) => pending.push(value),
                Self::Predicate(_, _, values)
                | Self::Call(_, _, _, values, _)
                | Self::Operation(_, values) => pending.extend(values.iter()),
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

        std::iter::from_fn(move || {
            loop {
                let condition = pending.pop()?;

                if exclude(condition) {
                    continue;
                }

                match condition {
                    Self::Operation(_, operands)
                    | Self::Predicate(_, _, operands)
                    | Self::Call(_, _, _, operands, _) => {
                        pending.extend(operands.iter());
                    }
                    Self::Tagged(_, subject)
                    | Self::VariantTest(_, subject)
                    | Self::Trusted(subject)
                    | Self::Borrowed(subject)
                    | Self::Present(subject) => {
                        pending.push(subject);
                    }
                    Self::Entry {
                        condition,
                        captured: false,
                    } => pending.push(condition),
                    Self::Observation { owner, contents: value, .. } => pending.extend([owner.as_ref(), value.as_ref()]),
                    Self::Constructed(base, fields) => {
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
            .any(|condition| matches!(condition, Self::Trusted(_)) && condition.observes(value))
    }

    /// Reports whether a completion clause observes its result. Unknown meaning
    /// conservatively retains result authority until the owning query resolves it.
    pub fn observes_completion_result(&self) -> bool {
        self.observations().any(|condition| {
            condition.is_unknown() || condition.is_owned_projection_of(&Self::Result)
        })
    }

    pub(crate) fn observes_live(&self, changed: Option<&Self>) -> bool {
        self.observes_live_where(|value| changed.is_none_or(|changed| value.observes(changed)), |_| false)
    }

    /// Inspect live borrowed observations and uncaptured entry guards, retaining excluded contexts.
    pub(crate) fn observes_live_where(&self, observes: impl Fn(&Self) -> bool, exclude: impl Fn(&Self) -> bool) -> bool {
        let mut pending = vec![(self, false)];

        while let Some((condition, live)) = pending.pop() {
            if exclude(condition) { continue; }

            if live && condition.observation_path().is_some() {
                if observes(condition) {
                    return true;
                }

                continue;
            }

            match condition {
                Self::Borrowed(value) => pending.push((value, true)),
                Self::Entry {
                    condition,
                    captured: false,
                } => pending.push((condition, true)),
                Self::Operation(_, operands)
                | Self::Predicate(_, _, operands)
                | Self::Call(_, _, _, operands, _) => {
                    pending.extend(operands.iter().map(|value| (value, live)))
                }
                Self::Tagged(_, value)
                | Self::VariantTest(_, value)
                | Self::Trusted(value)
                | Self::Projection(_, value)
                | Self::Present(value) => pending.push((value, live)),
                Self::Observation { owner, contents, kind } => {
                    pending.push((owner, live));
                    pending.push((contents, live || *kind == ObservationKind::Reference));
                }
                Self::Constructed(base, fields) => {
                    pending.push((base, live));

                    pending.extend(fields.iter().map(|(_, value)| (value, live)));
                }
                _ => {}
            }
        }

        false
    }
}

/// Indexed changed values and their observed component paths.
#[derive(Default)]
pub(crate) struct Set {
    discriminants: std::collections::BTreeSet<ExecutionCondition>,
    copies: std::collections::BTreeSet<ExecutionCondition>,
    values: std::collections::BTreeSet<ExecutionCondition>,
    paths: std::collections::BTreeMap<
        ExecutionCondition,
        std::collections::BTreeSet<Vec<bray_symbols::ConstantProjectionKind>>,
    >,
}

impl Set {
    /// Tests ownership against actual retained snapshots, without a prefix grant.
    pub(crate) fn retains_owner(
        snapshots: &std::collections::BTreeSet<ExecutionCondition>,
        value: &ExecutionCondition,
    ) -> bool {
        Self::retained_snapshot(snapshots, value, false, |selected| {
            !selected.is_unknown() && matches!((selected.ownership_identity(), value.ownership_identity()),
                (Some((role, owner)), Some((other_role, other))) if role == other_role && owner == other)
        })
    }

    /// Tests actual selected data independently of the containing owner's identity.
    pub(crate) fn retains_contents(
        snapshots: &std::collections::BTreeSet<ExecutionCondition>,
        value: &ExecutionCondition,
    ) -> bool {
        Self::retained_snapshot(snapshots, value, true, |selected| {
            Self::same_snapshot_contents(selected, value)
        })
    }

    /// Compares actual predecessor and terminal data at the same observed path.
    pub(crate) fn retains_contents_between(
        before: &std::collections::BTreeSet<ExecutionCondition>,
        after: &std::collections::BTreeSet<ExecutionCondition>,
        observed: &ExecutionCondition,
    ) -> bool {
        Self::retained_snapshot(before, observed, true, |previous| {
            Self::retained_snapshot(after, observed, true, |current| Self::same_snapshot_contents(previous, current))
        })
    }

    fn same_snapshot_contents(left: &ExecutionCondition, right: &ExecutionCondition) -> bool {
        let left = Self::snapshot_contents(left);
        let right = Self::snapshot_contents(right);

        !left.contains_unknown() && !right.contains_unknown() && left == right
    }

    fn snapshot_contents(mut value: &ExecutionCondition) -> &ExecutionCondition {
        while let ExecutionCondition::Observation { contents, kind: ObservationKind::Owned | ObservationKind::Scalar, .. } = value {
            value = contents;
        }

        value
    }

    fn retained_snapshot(
        snapshots: &std::collections::BTreeSet<ExecutionCondition>,
        value: &ExecutionCondition,
        observe_referents: bool,
        matches: impl Fn(&ExecutionCondition) -> bool,
    ) -> bool {
        let path = Self::indexed_path(value);
        let mut pending = snapshots.iter().collect::<Vec<_>>();
        // Actual snapshots form a finite shared graph. Visit each value once so
        // unrelated roots cannot spend another value's structural work budget.
        let mut visited = std::collections::BTreeSet::new();

        while let Some(snapshot) = pending.pop() {
            if !visited.insert(snapshot) {
                continue;
            }

            if snapshot.has_same_owner(value) && matches(snapshot) {
                return true;
            }

            if let (Some((root, path)), Some((base, prefix))) = (&path, Self::indexed_path(snapshot))
                && *root == base && path.starts_with(&prefix)
                && path.len() - prefix.len() <= ExecutionCondition::WORK_LIMIT
            {
                let mut selected = snapshot.clone();

                for projection in &path[prefix.len()..] {
                    selected = selected.component(*projection);
                }

                if matches!(snapshot.owner_identity(), ExecutionCondition::Borrowed(_))
                    && !path[prefix.len()..].is_empty()
                {
                    selected = selected.borrowed();
                }

                if matches(&selected) {
                    return true;
                }
            }

            // The base of a constructed value names its lineage, not another
            // independently retained copy of its former contents.
            pending.extend(snapshot.owned_components());

            if observe_referents {
                // A live loan retains its actual reached data without owning it.
                // Ownership queries never traverse this reference boundary.
                match snapshot {
                    ExecutionCondition::Borrowed(referent)
                    | ExecutionCondition::Observation { contents: referent, kind: ObservationKind::Reference, .. } => pending.push(referent),
                    _ => {}
                }
            }
        }

        false
    }

    pub(crate) fn insert_classified(&mut self, value: ExecutionCondition, kind: bray_bound_tree::StorageObservationKind) {
        if kind == bray_bound_tree::StorageObservationKind::Discriminant {
            self.discriminants.insert(value);
        } else if kind.is_immutable_inline() {
            self.insert(value);
        }
    }

    /// Retained scalar data preserves value facts independently of its former storage.
    /// A retained tag supports only presence and variant observations.
    pub(crate) fn preserves(&self, value: &ExecutionCondition) -> bool {
        if self.contains(value)
            || matches!(value, ExecutionCondition::Borrowed(referent) if self.contains(referent))
            || self.copies.contains(value)
            || Self::retained_snapshot(&self.copies, value, false,
                |selected| Self::same_snapshot_contents(selected, value))
        {
            return true;
        }

        let subject = match value {
            ExecutionCondition::VariantTest(_, subject) => Some(subject.as_ref()),
            value => value.nullable_absence(true).or_else(|| value.nullable_absence(false)).map(|(subject, _)| subject),
        };

        subject.is_some_and(|subject| self.discriminants.contains(subject))
    }

    pub(crate) fn insert(&mut self, value: ExecutionCondition) {
        self.preserve_scalar_data(&value);

        if let Some((root, path)) = Self::indexed_path(&value) {
            self.paths.entry(root).or_default().insert(path);
        }

        self.values.insert(value);
    }

    /// A surviving scalar snapshot retains data without keeping its source slot alive.
    pub(crate) fn preserve_scalar_data(&mut self, value: &ExecutionCondition) {
        if let Some((_, contents)) = value.storage_contents() {
            let copied = contents.clone().argument_value(false);

            if !copied.is_unknown() { self.copies.insert(copied); }
        }
    }

    pub(crate) fn contains(&self, value: &ExecutionCondition) -> bool {
        if self.values.contains(value) {
            return true;
        }

        let Some((root, path)) = Self::indexed_path(value) else {
            return false;
        };

        let Some(paths) = self.paths.get(&root) else {
            return false;
        };

        (0..=path.len()).any(|length| paths.contains(&path[..length]))
    }

    pub(crate) fn overlaps(&self, value: &ExecutionCondition) -> bool {
        if self.contains(value) {
            return true;
        }

        let Some((root, path)) = Self::indexed_path(value) else {
            return false;
        };

        self.paths.get(&root).is_some_and(|paths| {
            paths
                .range(path.clone()..)
                .next()
                .is_some_and(|candidate| candidate.starts_with(&path))
        })
    }

    fn indexed_path(
        value: &ExecutionCondition,
    ) -> Option<(
        ExecutionCondition,
        Vec<bray_symbols::ConstantProjectionKind>,
    )> {
        match value {
            // Retaining a borrow preserves that view and its projections. It
            // cannot retain or retire the referent's ownership on its own.
            ExecutionCondition::Borrowed(referent) => {
                let (root, path) = referent.observation_path()?;

                Some((root.borrowed(), path))
            }
            value if matches!(value.owner_identity(), ExecutionCondition::Borrowed(_)) => {
                Self::indexed_path(value.owner_identity())
            }
            value => value.observation_path(),
        }
    }
}

impl FromIterator<ExecutionCondition> for Set {
    fn from_iter<T: IntoIterator<Item = ExecutionCondition>>(values: T) -> Self {
        let mut set = Self::default();

        for value in values { set.insert(value); }

        set
    }
}
