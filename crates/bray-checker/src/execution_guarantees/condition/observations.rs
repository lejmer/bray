use super::ExecutionCondition;

impl ExecutionCondition {
    /// The carrier's identity survives changes to the component snapshot.
    pub(crate) fn owner_identity(&self) -> &Self {
        let mut value = self;

        while let Self::Constructed(base, _) = value {
            if **base == Self::Unknown {
                break;
            }

            value = base;
        }

        value
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
                Self::Constructed(base, _) if **base != Self::Unknown => value = base,
                Self::Expression(_)
                | Self::ScopedCapability(_)
                | Self::PostState(_, _)
                | Self::Joined(_, _, _)
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
        let owner = owner.owner_identity();
        let mut value = self.owner_identity();

        loop {
            if value == owner {
                return true;
            }

            match (value, owner) {
                (Self::Input(value), Self::Input(owner)) => return owner.contains(value),
                (Self::Projection(_, parent), _) => value = parent.owner_identity(),
                _ => return false,
            }
        }
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
                | Self::Call(_, operands) => pending.extend(operands.iter()),
                Self::Projection(_, value)
                | Self::Trusted(value)
                | Self::Borrowed(value)
                | Self::Present(value)
                | Self::Entry {
                    condition: value, ..
                } => pending.push(value),
                Self::Constructed(base, fields) => {
                    pending.push(base);

                    pending.extend(fields.iter().map(|(_, value)| value))
                }
                _ => {}
            }
        }

        inputs
    }

    pub(crate) fn depends_on(&self, expression: bray_bound_tree::BoundExpressionId) -> bool {
        let mut pending = vec![self];

        while let Some(condition) = pending.pop() {
            match condition {
                Self::Expression(candidate) | Self::ScopedCapability(candidate)
                    if *candidate == expression =>
                {
                    return true;
                }
                Self::PostState(site, _) if site.occurrence().expression() == Some(expression) => {
                    return true;
                }
                Self::Constructed(base, fields) => {
                    pending.push(base);

                    pending.extend(fields.iter().map(|(_, value)| value))
                }
                Self::Operation(_, operands)
                | Self::Predicate(_, _, operands)
                | Self::Call(_, operands) => pending.extend(operands.iter()),
                Self::Projection(_, value)
                | Self::Trusted(value)
                | Self::Borrowed(value)
                | Self::Present(value)
                | Self::Entry {
                    condition: value, ..
                } => pending.push(value),
                _ => {}
            }
        }

        false
    }

    pub(crate) fn owned_components(&self) -> impl Iterator<Item = &Self> {
        let present = match self {
            Self::Present(value)
            | Self::Projection(bray_symbols::ConstantProjectionKind::NullableValue, value) => {
                Some(value.as_ref())
            }
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

    pub(crate) fn owned_values(&self) -> impl Iterator<Item = &Self> {
        let mut pending = vec![self];

        std::iter::from_fn(move || {
            let condition = pending.pop()?;

            pending.extend(condition.owned_components());

            Some(condition)
        })
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
                    | Self::Call(_, operands) => {
                        pending.extend(operands.iter());
                    }
                    Self::Trusted(subject) | Self::Borrowed(subject) | Self::Present(subject) => {
                        pending.push(subject);
                    }
                    Self::Entry {
                        condition,
                        captured: false,
                    } => pending.push(condition),
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

    /// Reports whether a completion clause observes its result. Unknown meaning
    /// conservatively retains result authority until the owning query resolves it.
    pub fn observes_completion_result(&self) -> bool {
        self.observations().any(|condition| {
            condition.is_unknown() || condition.is_owned_projection_of(&Self::Result)
        })
    }

    pub(crate) fn observes_borrowed(&self, changed: Option<&Self>) -> bool {
        self.observes_borrowed_where(|value| changed.is_none_or(|changed| value.observes(changed)))
    }

    pub(crate) fn observes_borrowed_where(&self, observes: impl Fn(&Self) -> bool) -> bool {
        let mut pending = vec![self];

        while let Some(condition) = pending.pop() {
            match condition {
                Self::Borrowed(value) if observes(value) => {
                    return true;
                }
                Self::Operation(_, operands)
                | Self::Predicate(_, _, operands)
                | Self::Call(_, operands) => pending.extend(operands.iter()),
                Self::Trusted(value) | Self::Projection(_, value) | Self::Present(value) => {
                    pending.push(value)
                }
                Self::Entry {
                    condition,
                    captured: false,
                } => pending.push(condition),
                _ => {}
            }
        }

        false
    }
}

/// Indexed changed values and their observed component paths.
#[derive(Default)]
pub(crate) struct Set {
    values: std::collections::BTreeSet<ExecutionCondition>,
    paths: std::collections::BTreeMap<
        ExecutionCondition,
        std::collections::BTreeSet<Vec<bray_symbols::ConstantProjectionKind>>,
    >,
}

impl Set {
    pub(crate) fn insert(&mut self, value: ExecutionCondition) {
        if let Some((root, path)) = value.observation_path() {
            self.paths.entry(root).or_default().insert(path);
        }

        self.values.insert(value);
    }

    pub(crate) fn contains(&self, value: &ExecutionCondition) -> bool {
        if self.values.contains(value) {
            return true;
        }

        let Some((root, path)) = value.observation_path() else {
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

        let Some((root, path)) = value.observation_path() else {
            return false;
        };

        self.paths.get(&root).is_some_and(|paths| {
            paths
                .range(path.clone()..)
                .next()
                .is_some_and(|candidate| candidate.starts_with(&path))
        })
    }
}
