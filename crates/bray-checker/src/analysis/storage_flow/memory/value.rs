use std::collections::BTreeSet;

use super::model::{Retirement, Target};

/// Precision of an evaluated address, independent of allocation identity or authority.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum AddressKnowledge {
    #[default]
    Values,
    Regions,
    DisplacedValues,
    DisplacedRegions,
    Unknown,
}

impl AddressKnowledge {
    fn join(self, incoming: Self) -> Self {
        match (self, incoming) {
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::DisplacedRegions, _) | (_, Self::DisplacedRegions)
            | (Self::Regions, Self::DisplacedValues) | (Self::DisplacedValues, Self::Regions) => Self::DisplacedRegions,
            (Self::DisplacedValues, _) | (_, Self::DisplacedValues) => Self::DisplacedValues,
            (Self::Regions, _) | (_, Self::Regions) => Self::Regions,
            (Self::Values, Self::Values) => Self::Values,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct AddressValue {
    pub(super) targets: BTreeSet<Target>,
    knowledge: AddressKnowledge,
    /// An actual empty alternative, independent of the nonempty targets.
    pub(super) nullable: bool,
}

impl AddressValue {
    pub(super) fn one(target: Target) -> Self {
        Self::from_targets([target])
    }

    pub(super) fn from_targets(targets: impl IntoIterator<Item = Target>) -> Self {
        Self { targets: targets.into_iter().collect(), ..Self::default() }
    }

    pub(super) fn empty() -> Self {
        Self { nullable: true, ..Self::default() }
    }

    pub(super) fn unknown() -> Self {
        Self { knowledge: AddressKnowledge::Unknown, ..Self::default() }
    }

    pub(super) fn is_unknown(&self) -> bool {
        self.knowledge == AddressKnowledge::Unknown
    }

    pub(super) fn is_displaced(&self) -> bool {
        matches!(self.knowledge, AddressKnowledge::DisplacedValues | AddressKnowledge::DisplacedRegions)
    }

    pub(super) fn is_bounded(&self) -> bool {
        matches!(self.knowledge, AddressKnowledge::Regions | AddressKnowledge::DisplacedRegions)
    }

    pub(super) fn has_values(&self) -> bool {
        self.is_value_set() && (!self.nullable || self.targets.is_empty())
    }

    /// These are evaluated address alternatives, including any known empty alternative.
    fn is_value_set(&self) -> bool {
        self.knowledge == AddressKnowledge::Values
    }

    pub(super) fn has_empty_alternative(&self) -> bool {
        !self.is_unknown() && self.nullable
    }

    pub(super) fn forget_value(&mut self) {
        self.knowledge = AddressKnowledge::Unknown;
    }

    pub(super) fn bound(&mut self) {
        self.knowledge = self.knowledge.join(AddressKnowledge::Regions);
    }

    pub(super) fn displace(&mut self) {
        if self.nullable {
            // Arithmetic from a null alternative has no established allocation bound.
            self.forget_value();
            self.nullable = false;
        }

        self.knowledge = self.knowledge.join(AddressKnowledge::DisplacedValues);
    }

    pub(super) fn merge(&mut self, incoming: &Self) -> bool {
        let count = self.targets.len();
        let knowledge = self.knowledge;
        let nullable = self.nullable;

        // Each alternative set owns descriptors and shares immutable projection paths.
        self.targets.extend(incoming.targets.iter().cloned());
        self.knowledge = self.knowledge.join(incoming.knowledge);
        self.nullable |= incoming.nullable;

        self.targets.len() != count || self.knowledge != knowledge || self.nullable != nullable
    }

    pub(super) fn retire(&mut self, current: &Target, retirement: Retirement) {
        let targets = self.targets.range(current.clone()..)
            .take_while(|target| current.contains(target))
            .filter(|target| retirement.contains(current, target))
            .cloned().collect::<Vec<_>>();

        for target in targets {
            self.targets.remove(&target);
            self.targets.insert(Target { previous: true, ..target });
        }
    }

    pub(super) fn is_exact(&self) -> bool {
        self.has_values() && self.targets.len() == 1
            && self.targets.first().expect("one address alternative").is_exact()
    }

    pub(super) fn regions(&self) -> Option<Vec<Target>> {
        if self.is_unknown() {
            return None;
        }

        self.targets.iter().map(|target| {
            if self.is_displaced() { target.allocation_region() } else { Some(target.clone()) }
        }).collect()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum DependencyCompleteness {
    #[default]
    Untracked,
    Partial,
    Complete,
}

impl DependencyCompleteness {
    pub(super) fn tracked(self) -> bool {
        self != Self::Untracked
    }

    pub(super) fn complete(self) -> bool {
        self == Self::Complete
    }

    pub(super) fn lose_completeness(&mut self) {
        if self.tracked() { *self = Self::Partial; }
    }

    pub(super) fn for_tracked(complete: bool) -> Self {
        if complete { Self::Complete } else { Self::Partial }
    }

    fn join(self, incoming: Self) -> Self {
        match (self, incoming) {
            (Self::Untracked, Self::Untracked) => Self::Untracked,
            (Self::Complete, Self::Complete) => Self::Complete,
            _ => Self::Partial,
        }
    }

    fn include(self, incoming: Self) -> Self {
        match (self, incoming) {
            (Self::Untracked, _) => incoming,
            (_, Self::Untracked) => self,
            _ => self.join(incoming),
        }
    }
}

/// One evaluated value's address and carried dependencies, captured and published together.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct ValueBinding {
    // An inactive payload contributes no value, independently of an actual null address.
    no_payload: bool,
    pub(super) address: Option<AddressValue>,
    /// Possible address origins carried by generic transport, never evaluated address identity.
    pub(super) address_bounds: Option<AddressValue>,
    pub(super) sources: BTreeSet<Target>,
    pub(super) completeness: DependencyCompleteness,
}

impl ValueBinding {
    pub(super) fn no_payload() -> Self {
        Self { no_payload: true, ..Self::default() }
    }

    pub(super) fn is_no_payload(&self) -> bool { self.no_payload }

    pub(super) fn whole_sources(&self) -> Option<(&BTreeSet<Target>, bool)> {
        (!self.no_payload && self.completeness.tracked()).then_some((&self.sources, self.completeness.complete()))
    }

    pub(super) fn whole_complete(&self) -> bool {
        self.no_payload || self.completeness.complete()
    }

    pub(super) fn unknown_dependencies(&self) -> bool {
        !self.no_payload && self.completeness == DependencyCompleteness::Partial
    }

    pub(super) fn make_present(&mut self) {
        if self.no_payload { *self = Self::default(); }
    }

    pub(super) fn produced_address(&mut self) -> &mut AddressValue {
        self.make_present();

        self.address.get_or_insert_with(AddressValue::default)
    }

    pub(super) fn capture_inputs(&mut self, captures: impl IntoIterator<Item = Target>) {
        self.make_present();

        let bounds = self.address_bounds.get_or_insert_with(AddressValue::default);

        bounds.forget_value();
        bounds.targets.extend(captures);
    }

    /// Conservative address information. Actual-value queries read `address` directly.
    pub(super) fn reference(&self) -> Option<&AddressValue> {
        if self.no_payload { return None; }

        self.address.as_ref().or(self.address_bounds.as_ref())
    }

    pub(super) fn reference_mut(&mut self) -> Option<&mut AddressValue> {
        if self.no_payload { return None; }

        self.address.as_mut().or(self.address_bounds.as_mut())
    }

    pub(super) fn references(&self) -> impl Iterator<Item = &AddressValue> {
        self.address.iter().chain(self.address_bounds.iter())
    }

    pub(super) fn possible_references(&self) -> impl Iterator<Item = &AddressValue> {
        let known = self.address.as_ref().is_some_and(AddressValue::is_value_set);

        self.address.iter().chain(self.address_bounds.iter().filter(move |_| !known))
            .filter(|_| !self.no_payload)
    }

    pub(super) fn captured_reference(&self) -> Option<AddressValue> {
        let mut references = self.possible_references();

        // An evaluated snapshot owns descriptors and shares immutable target paths.
        let mut captured = references.next()?.clone();

        for bounds in references { captured.merge(bounds); }

        Some(captured)
    }

    pub(super) fn reference_regions(&self) -> Option<Vec<Target>> {
        let mut references = self.possible_references();
        let mut regions = references.next()?.regions()?;

        for bounds in references { regions.extend(bounds.regions()?); }

        regions.sort_unstable();
        regions.dedup();

        Some(regions)
    }

    pub(super) fn mutation_targets(&self) -> Option<Vec<super::effect::MutationTarget>> {
        Some(super::effect::MutationTarget::reference(self.address.as_ref(), self.reference_regions()?))
    }

    pub(super) fn references_mut(&mut self) -> impl Iterator<Item = &mut AddressValue> {
        self.address.iter_mut().chain(self.address_bounds.iter_mut())
    }

    pub(super) fn clear_address(&mut self) {
        if self.no_payload { self.lose_information(); }

        self.address = None;
        self.address_bounds = None;
    }

    pub(super) fn lose_information(&mut self) {
        if self.no_payload {
            *self = Self { completeness: DependencyCompleteness::Partial, ..Self::default() };
        }

        for address in self.references_mut() { address.forget_value(); }
        self.completeness.lose_completeness();
    }

    pub(super) fn into_dependency_bounds(mut self) -> Self {
        // Dependency bounds describe no payload presence or absence.
        if self.no_payload { self = Self { completeness: self.completeness, ..Self::default() }; }

        if let Some(address) = self.address.take() {
            self.include_address_bound(&address);
        }

        self
    }

    fn include_address_bound(&mut self, incoming: &AddressValue) {
        let bounds = self.address_bounds.get_or_insert_with(AddressValue::default);

        bounds.merge(incoming);
        bounds.bound();
    }

    /// A feasible predecessor without address information contributes an unknown alternative.
    pub(super) fn join(&mut self, incoming: &Self) {
        if !self.merge_presence(incoming) { return; }

        let missing_address = self.address.is_none() || incoming.address.is_none();
        let missing_bounds = self.address_bounds.is_none() || incoming.address_bounds.is_none();

        self.merge_contents(incoming, self.completeness.join(incoming.completeness));

        if missing_address && let Some(address) = &mut self.address {
            address.forget_value();
        }

        if missing_bounds && let Some(bounds) = &mut self.address_bounds {
            bounds.forget_value();
        }
    }

    /// Attach conservative dependency information to an already prepared produced value.
    pub(super) fn include_carried(&mut self, incoming: &Self) {
        if self.no_payload || incoming.no_payload { return; }

        self.sources.extend(incoming.sources.iter().cloned());
        self.completeness = self.completeness.include(incoming.completeness);

        for incoming in incoming.address.iter().chain(incoming.address_bounds.iter()) {
            self.include_address_bound(incoming);
        }
    }

    /// Join alternative evaluated values, including missing generic information.
    pub(super) fn merge(&mut self, incoming: &Self) {
        if self.merge_presence(incoming) {
            self.merge_contents(incoming, self.completeness.join(incoming.completeness));
        }
    }

    /// Assemble captured contents whose explicit reference slots supplement opaque lineage.
    pub(super) fn include_value(&mut self, incoming: &Self) {
        if self.merge_presence(incoming) {
            self.merge_contents(incoming, self.completeness.include(incoming.completeness));
        }
    }

    fn merge_presence(&mut self, incoming: &Self) -> bool {
        if self.no_payload {
            // Live alternatives own descriptors and share immutable target paths.
            *self = if incoming.no_payload { Self::no_payload() } else { incoming.clone() };

            return false;
        }

        !incoming.no_payload
    }

    fn merge_contents(&mut self, incoming: &Self, completeness: DependencyCompleteness) {
        match (&mut self.address, &incoming.address) {
            (Some(current), Some(incoming)) => { current.merge(incoming); }
            (None, Some(incoming)) => {
                // Each captured binding owns alternatives and shares immutable target paths.
                let mut address = incoming.clone();

                if self.completeness.tracked() || self.address_bounds.is_some() { address.bound(); }
                self.address = Some(address);
            }
            (Some(current), None) if incoming.completeness.tracked() || incoming.address_bounds.is_some() => current.bound(),
            _ => {}
        }

        self.sources.extend(incoming.sources.iter().cloned());
        self.completeness = completeness;

        if let Some(bounds) = &incoming.address_bounds { self.include_address_bound(bounds); }
    }
}

impl From<AddressValue> for ValueBinding {
    fn from(mut address: AddressValue) -> Self {
        // A published known value with no nonempty target is explicitly empty.
        if address.has_values() && address.targets.is_empty() { address.nullable = true; }

        Self { address: Some(address), ..Self::default() }
    }
}
