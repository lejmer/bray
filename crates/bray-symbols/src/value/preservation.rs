use std::collections::BTreeMap;

/// Contents preserved by an identity transfer, with overrides for child subtrees.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyValuePreservation<Projection> {
    preserved: bool,
    children: BTreeMap<Projection, Self>,
}

#[cfg(test)]
mod tests {
    use super::DependencyValuePreservation as Mask;

    #[test]
    fn child_defaults_intersect_and_redundant_overrides_disappear() {
        let partial = Mask::new(true, [(0_u8, Mask::none()), (2, Mask::all())].into());
        let sparse = Mask::new(false, [(1, Mask::all())].into());

        assert_eq!(partial.children().len(), 1);
        assert_eq!(partial.projected(&1), Mask::all());
        assert_eq!(partial.projected(&0), Mask::none());
        assert_eq!(partial.intersection(&sparse), sparse);
        assert_eq!(partial.intersection(&Mask::none()), Mask::none());
    }

    #[test]
    fn selector_mapping_intersects_colliding_overrides() {
        let mask = Mask::new(true, [(0_u8, Mask::none()), (1, Mask::new(false, [(2, Mask::all())].into()))].into());

        let mapped = mask.try_map_projections(&mut |_| Ok::<_, ()>(0_u8)).unwrap();

        assert_eq!(mapped, Mask::new(true, [(0, Mask::none())].into()));
    }
}

impl<Projection> Default for DependencyValuePreservation<Projection> {
    fn default() -> Self {
        Self::none()
    }
}

impl<Projection> DependencyValuePreservation<Projection> {
    /// Records preservation of every value in the subtree.
    pub fn all() -> Self {
        Self { preserved: true, children: BTreeMap::new() }
    }

    /// Records identity alone, with no preserved contents.
    pub fn none() -> Self {
        Self { preserved: false, children: BTreeMap::new() }
    }

    /// Returns the default preservation of this subtree.
    pub fn preserves_value(&self) -> bool {
        self.preserved
    }

    /// Returns child overrides relative to the transferred input.
    pub fn children(&self) -> &BTreeMap<Projection, Self> {
        &self.children
    }
}

impl<Projection: Clone + Ord> DependencyValuePreservation<Projection> {
    /// Selects a child subtree, inheriting this node's default when absent.
    pub fn projected(&self, projection: &Projection) -> Self {
        self.children.get(projection).cloned().unwrap_or(Self {
            preserved: self.preserved,
            children: BTreeMap::new(),
        })
    }

    /// Creates a canonical subtree mask, omitting redundant default leaves.
    pub fn new(preserved: bool, mut children: BTreeMap<Projection, Self>) -> Self {
        children.retain(|_, child| child.preserved != preserved || !child.children.is_empty());

        Self { preserved, children }
    }

    /// Retains only preservation established by both transfers.
    pub fn intersection(&self, other: &Self) -> Self {
        let mut children = BTreeMap::new();
        let left_default = Self { preserved: self.preserved, children: BTreeMap::new() };
        let right_default = Self { preserved: other.preserved, children: BTreeMap::new() };

        for projection in self.children.keys().chain(other.children.keys()) {
            if children.contains_key(projection) {
                continue;
            }

            let left = self.children.get(projection).unwrap_or(&left_default);
            let right = other.children.get(projection).unwrap_or(&right_default);

            children.insert(projection.clone(), left.intersection(right));
        }

        Self::new(self.preserved && other.preserved, children)
    }

    /// Translates selectors while conservatively combining coincident children.
    pub fn try_map_projections<Other: Clone + Ord, Error>(
        &self,
        map: &mut impl FnMut(&Projection) -> Result<Other, Error>,
    ) -> Result<DependencyValuePreservation<Other>, Error> {
        let mut children: BTreeMap<Other, DependencyValuePreservation<Other>> = BTreeMap::new();

        for (projection, child) in &self.children {
            let projection = map(projection)?;
            let child = child.try_map_projections(map)?;

            children.entry(projection)
                .and_modify(|previous| *previous = previous.intersection(&child))
                .or_insert(child);
        }

        Ok(DependencyValuePreservation::new(self.preserved, children))
    }
}
