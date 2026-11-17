use std::collections::BTreeSet;

use super::target::{StorageOrigin as Origin, StorageTarget as Target};

/// Which descriptor incarnation an owning storage transition retires. This is not allocation authority.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageValueRetirement {
    /// Retire inline storage without following separately owned backing.
    Storage,
    /// Retire the producing storage namespace, including its backing descriptors.
    Producer,
}

impl StorageValueRetirement {
    /// Selects the descriptor retirement scope of one produced payload.
    pub fn for_payload(target: &Target) -> Self {
        if target.projections().is_empty()
            && matches!(target.origin, Origin::Returned { .. } | Origin::Allocation { .. })
        {
            Self::Producer
        } else {
            Self::Storage
        }
    }

    /// Whether this retirement scope includes the target within the current namespace.
    pub fn contains(self, current: &Target, target: &Target) -> bool {
        match self {
            Self::Storage => current.contains_storage(target),
            Self::Producer => current.contains(target),
        }
    }
}

/// One evaluated operand and its possible mutation region. Bounds never supply an address.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageMutationTarget {
    /// The actual addressed location, absent when only a region is established.
    pub address: Option<Target>,
    /// Conservative region that execution may affect.
    pub footprint: Target,
}

impl StorageMutationTarget {
    /// Returns the conservative mutation region without asserting an actual address.
    pub fn footprint(&self) -> &Target { &self.footprint }

    /// Captures a checked storage location as an actual mutation operand.
    pub fn location(target: Target) -> Self {
        // The address owns its descriptor while both paths share immutable projections.
        Self {
            address: target.is_exact().then(|| target.clone()),
            footprint: target
        }
    }

    /// Records a mutation region with no corresponding actual-address claim.
    pub fn bounded(footprint: Target) -> Self {
        Self {
            address: None,
            footprint
        }
    }

    /// Captures actual descriptor alternatives separately from their conservative regions.
    pub fn reference(address: Option<&StorageAddressValue>, regions: Vec<Target>) -> Vec<Self> {
        regions.into_iter().map(|footprint| {
            let actual = address.filter(|address| address.has_values()
                && address.targets.contains(&footprint) && footprint.is_exact());

            // A captured address owns its descriptor independently of later slot replacement.
            Self {
                address: actual.map(|_| footprint.clone()),
                footprint
            }
        }).collect()
    }

    /// Follows a resolved effect route against one incoming memory snapshot. A possible
    /// region can bound a final write, but cannot identify the slot for another load.
    pub fn resolve_route(
        mut targets: Vec<Self>,
        projections: &[super::StorageProjection],
        reads: &[u32],
        mut read: impl FnMut(Target) -> Option<Vec<Self>>,
    ) -> Option<Vec<Self>> {
        let mut reads = reads.iter().copied().peekable();

        let mut follow = |targets: Vec<Self>| {
            let mut reached = Vec::new();

            for target in targets {
                reached.extend(read(target.address?)?);
            }

            Some(reached)
        };

        for index in 0..=projections.len() {
            while reads.next_if(|position| *position as usize == index).is_some() {
                targets = follow(targets)?;
            }

            if let Some(projection) = projections.get(index) {
                targets = if *projection == super::StorageProjection::AddressTarget {
                    follow(targets)?
                } else {
                    targets.into_iter().map(|target| target.project(std::slice::from_ref(projection))).collect()
                };
            }
        }

        assert!(reads.next().is_none(), "effect reads lie within the checked route");

        Some(targets)
    }

    /// Selects a component of the captured operand without rereading its descriptor.
    pub fn project(self, suffix: &[super::StorageProjection]) -> Self {
        Self {
            address: self.address.map(|address| address.project(suffix)).filter(Target::is_exact),
            footprint: self.footprint.project(suffix),
        }
    }
}

/// Precision of an evaluated address, independent of allocation identity or authority.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
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

/// Evaluated address alternatives and their precision, separately from ownership or allocation freshness.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageAddressValue {
    /// Address alternatives or region bounds, according to this value's retained precision.
    pub targets: BTreeSet<Target>,
    knowledge: AddressKnowledge,
    /// An actual empty alternative, independent of the nonempty targets.
    pub nullable: bool,
}

impl StorageAddressValue {
    /// Creates one actual evaluated address alternative.
    pub fn one(target: Target) -> Self {
        Self::from_targets([target])
    }

    /// Creates a complete set of evaluated nonempty address alternatives.
    pub fn from_targets(targets: impl IntoIterator<Item = Target>) -> Self {
        Self {
            targets: targets.into_iter().collect(),
            ..Self::default()
        }
    }

    /// Creates an actual empty address alternative.
    pub fn empty() -> Self {
        Self {
            nullable: true,
            ..Self::default()
        }
    }

    /// Creates an address with unknown value and unknown region.
    pub fn unknown() -> Self {
        Self {
            knowledge: AddressKnowledge::Unknown,
            ..Self::default()
        }
    }

    /// Whether the address has no established finite region.
    pub fn is_unknown(&self) -> bool {
        self.knowledge == AddressKnowledge::Unknown
    }

    /// Whether arithmetic has lost the addressed offset within its allocation.
    pub fn is_displaced(&self) -> bool {
        matches!(self.knowledge, AddressKnowledge::DisplacedValues | AddressKnowledge::DisplacedRegions)
    }

    /// Whether the targets only bound the possible addressed region.
    pub fn is_bounded(&self) -> bool {
        matches!(self.knowledge, AddressKnowledge::Regions | AddressKnowledge::DisplacedRegions)
    }

    /// Whether nonempty targets are actual address alternatives without an additional null alternative.
    pub fn has_values(&self) -> bool {
        self.is_value_set() && (!self.nullable || self.targets.is_empty())
    }

    /// These are evaluated address alternatives, including any known empty alternative.
    fn is_value_set(&self) -> bool {
        self.knowledge == AddressKnowledge::Values
    }

    /// Whether an actual empty alternative remains possible.
    pub fn has_empty_alternative(&self) -> bool {
        !self.is_unknown() && self.nullable
    }

    /// Drops address precision without turning retained bounds into absence.
    pub fn forget_value(&mut self) {
        self.knowledge = AddressKnowledge::Unknown;
    }

    /// Demotes evaluated alternatives to conservative region information.
    pub fn bound(&mut self) {
        self.knowledge = self.knowledge.join(AddressKnowledge::Regions);
    }

    /// Loses offset knowledge after address arithmetic.
    pub fn displace(&mut self) {
        if self.nullable {
            // Arithmetic from a null alternative has no established allocation bound.
            self.forget_value();
            self.nullable = false;
        }

        self.knowledge = self.knowledge.join(AddressKnowledge::DisplacedValues);
    }

    /// Joins alternative address knowledge and retains possible empty alternatives.
    pub fn merge(&mut self, incoming: &Self) -> bool {
        let count = self.targets.len();
        let knowledge = self.knowledge;
        let nullable = self.nullable;

        // Each alternative set owns descriptors and shares immutable projection paths.
        self.targets.extend(incoming.targets.iter().cloned());
        self.knowledge = self.knowledge.join(incoming.knowledge);
        self.nullable |= incoming.nullable;

        self.targets.len() != count || self.knowledge != knowledge || self.nullable != nullable
    }

    /// Moves affected descriptors to the previous incarnation of the storage namespace.
    pub fn retire(&mut self, current: &Target, retirement: StorageValueRetirement) {
        let targets = self.targets.range(current.clone()..)
            .take_while(|target| current.contains(target))
            .filter(|target| retirement.contains(current, target))
            .cloned().collect::<Vec<_>>();

        for target in targets {
            self.targets.remove(&target);

            self.targets.insert(
                Target {
                    previous: true,
                    ..target
                }
            );
        }
    }

    /// Whether one current, undisplaced, nonempty address is established.
    pub fn is_exact(&self) -> bool {
        self.has_values() && self.targets.len() == 1
            && self.targets.first().expect("one address alternative").is_exact()
    }

    /// Returns conservative regions when their enclosing allocation boundaries are known.
    pub fn regions(&self) -> Option<Vec<Target>> {
        if self.is_unknown() {
            return None;
        }

        self.targets.iter().map(|target| {
            if self.is_displaced() {
                target.allocation_region()
            } else {
                Some(target.clone())
            }
        }).collect()
    }
}

/// Whether the possible carried dependencies are exhaustive. Completeness supplies no value identity.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageDependencyCompleteness {
    /// No whole-value dependency relation has been recorded.
    #[default]
    Untracked,
    /// Recorded sources can omit additional possible dependencies.
    Partial,
    /// Recorded sources bound all possible dependencies.
    Complete,
}

impl StorageDependencyCompleteness {
    /// Whether a whole-value dependency relation is present.
    pub fn tracked(self) -> bool {
        self != Self::Untracked
    }

    /// Whether all possible carried dependencies are bounded by the recorded sources.
    pub fn complete(self) -> bool {
        self == Self::Complete
    }

    /// Retains tracked sources while admitting additional unknown dependencies.
    pub fn lose_completeness(&mut self) {
        if self.tracked() {
            *self = Self::Partial;
        }
    }

    /// Records whether an established dependency relation is exhaustive.
    pub fn for_tracked(complete: bool) -> Self {
        if complete {
            Self::Complete
        } else {
            Self::Partial
        }
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
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageValueBinding {
    /// Checked observation roles carried together by this actual value, never by a region bound.
    pub observations: BTreeSet<super::StorageObservationKind>,
    // An inactive payload contributes no value, independently of an actual null address.
    no_payload: bool,
    /// Actual descriptor knowledge, independently of carried dependency bounds.
    pub address: Option<StorageAddressValue>,
    /// Possible address origins carried by generic transport, never evaluated address identity.
    pub address_bounds: Option<StorageAddressValue>,
    /// Possible whole-value dependency origins. These are not identity or ownership edges.
    pub sources: BTreeSet<Target>,
    /// Completeness of the possible dependency origins.
    pub completeness: StorageDependencyCompleteness,
}

impl StorageValueBinding {
    /// Creates a known inactive payload, distinct from null and unknown.
    pub fn no_payload() -> Self {
        Self { no_payload: true, ..Self::default() }
    }

    /// Whether this alternative contains no active payload value.
    pub fn is_no_payload(&self) -> bool {
        self.no_payload
    }

    /// Returns possible whole-value sources and their dependency completeness.
    pub fn whole_sources(&self) -> Option<(&BTreeSet<Target>, bool)> {
        (!self.no_payload && self.completeness.tracked()).then_some((&self.sources, self.completeness.complete()))
    }

    /// Whether the value has exhaustive dependency bounds or is explicitly inactive.
    pub fn whole_complete(&self) -> bool {
        self.no_payload || self.completeness.complete()
    }

    /// Whether additional unrecorded dependencies remain possible.
    pub fn unknown_dependencies(&self) -> bool {
        !self.no_payload && self.completeness == StorageDependencyCompleteness::Partial
    }

    /// Begins a present value after a previously inactive payload.
    pub fn make_present(&mut self) {
        if self.no_payload {
            *self = Self::default();
        }
    }

    /// Returns the actual address component of the produced value.
    pub fn produced_address(&mut self) -> &mut StorageAddressValue {
        self.make_present();

        self.address.get_or_insert_with(StorageAddressValue::default)
    }

    /// Retains conservative input regions without claiming an evaluated address.
    pub fn capture_inputs(&mut self, captures: impl IntoIterator<Item = Target>) {
        self.make_present();

        let bounds = self.address_bounds.get_or_insert_with(StorageAddressValue::default);

        bounds.forget_value();
        bounds.targets.extend(captures);
    }

    /// Conservative address information. Actual-value queries read `address` directly.
    pub fn reference(&self) -> Option<&StorageAddressValue> {
        if self.no_payload {
            return None;
        }

        self.address.as_ref().or(self.address_bounds.as_ref())
    }

    /// Returns the retained address or bounds for conservative knowledge updates.
    pub fn reference_mut(&mut self) -> Option<&mut StorageAddressValue> {
        if self.no_payload {
            return None;
        }

        self.address.as_mut().or(self.address_bounds.as_mut())
    }

    /// Visits actual address knowledge and carried address bounds.
    pub fn references(&self) -> impl Iterator<Item = &StorageAddressValue> {
        self.address.iter().chain(self.address_bounds.iter())
    }

    /// Visits conservative alternatives, preferring a complete actual address value.
    pub fn possible_references(&self) -> impl Iterator<Item = &StorageAddressValue> {
        let known = self.address.as_ref().is_some_and(StorageAddressValue::is_value_set);

        self.address.iter().chain(self.address_bounds.iter().filter(move |_| !known))
            .filter(|_| !self.no_payload)
    }

    /// Captures descriptor knowledge while retaining its actual or bounded precision.
    pub fn captured_reference(&self) -> Option<StorageAddressValue> {
        let mut references = self.possible_references();

        // An evaluated snapshot owns descriptors and shares immutable target paths.
        let mut captured = references.next()?.clone();

        for bounds in references {
            captured.merge(bounds);
        }

        Some(captured)
    }

    /// Returns the possible regions reached by this value.
    pub fn reference_regions(&self) -> Option<Vec<Target>> {
        let mut references = self.possible_references();
        let mut regions = references.next()?.regions()?;

        for bounds in references {
            regions.extend(bounds.regions()?);
        }

        regions.sort_unstable();
        regions.dedup();

        Some(regions)
    }

    /// Captures mutation operands without promoting possible regions to actual addresses.
    pub fn mutation_targets(&self) -> Option<Vec<StorageMutationTarget>> {
        Some(StorageMutationTarget::reference(self.address.as_ref(), self.reference_regions()?))
    }

    /// Visits address components for conservative incarnation and knowledge updates.
    pub fn references_mut(&mut self) -> impl Iterator<Item = &mut StorageAddressValue> {
        self.address.iter_mut().chain(self.address_bounds.iter_mut())
    }

    /// Forgets the descriptor and its bounds without claiming an empty payload.
    pub fn clear_address(&mut self) {
        if self.no_payload {
            self.lose_information();
        }

        self.address = None;
        self.address_bounds = None;
    }

    /// Drops value precision and completeness while retaining possible dependencies.
    pub fn lose_information(&mut self) {
        if self.no_payload {
            *self = Self { completeness: StorageDependencyCompleteness::Partial, ..Self::default() };
        }

        for address in self.references_mut() {
            address.forget_value();
        }

        self.completeness.lose_completeness();
    }

    /// Demotes a value snapshot to possible dependency information only.
    pub fn into_dependency_bounds(mut self) -> Self {
        self.observations.clear();

        // Dependency bounds describe no payload presence or absence.
        if self.no_payload {
            self = Self {
                completeness: self.completeness, ..Self::default()
            };
        }

        if let Some(address) = self.address.take() {
            self.include_address_bound(&address);
        }

        self
    }

    fn include_address_bound(&mut self, incoming: &StorageAddressValue) {
        let bounds = self.address_bounds.get_or_insert_with(StorageAddressValue::default);

        bounds.merge(incoming);
        bounds.bound();
    }

    /// A feasible predecessor without address information contributes an unknown alternative.
    pub fn join(&mut self, incoming: &Self) {
        if !self.merge_presence(incoming) {
            return;
        }

        let missing_address = self.address.is_none() || incoming.address.is_none();
        let missing_bounds = self.address_bounds.is_none() || incoming.address_bounds.is_none();

        self.observations.retain(|kind| incoming.observations.contains(kind));

        self.merge_contents(incoming, self.completeness.join(incoming.completeness));

        if missing_address && let Some(address) = &mut self.address {
            address.forget_value();
        }

        if missing_bounds && let Some(bounds) = &mut self.address_bounds {
            bounds.forget_value();
        }
    }

    /// Attach conservative dependency information to an already prepared produced value.
    pub fn include_carried(&mut self, incoming: &Self) {
        if self.no_payload || incoming.no_payload {
            return;
        }

        self.sources.extend(incoming.sources.iter().cloned());
        self.completeness = self.completeness.include(incoming.completeness);

        for incoming in incoming.address.iter().chain(incoming.address_bounds.iter()) {
            self.include_address_bound(incoming);
        }
    }

    /// Join alternative evaluated values, including missing generic information.
    pub fn merge(&mut self, incoming: &Self) {
        if self.merge_presence(incoming) {
            self.observations.retain(|kind| incoming.observations.contains(kind));
            self.merge_contents(incoming, self.completeness.join(incoming.completeness));
        }
    }

    /// Assemble captured contents whose explicit reference slots supplement opaque lineage.
    pub fn include_value(&mut self, incoming: &Self) {
        if self.merge_presence(incoming) {
            if self.observations.is_empty() { self.observations.clone_from(&incoming.observations); }
            self.merge_contents(incoming, self.completeness.include(incoming.completeness));
        }
    }

    fn merge_presence(&mut self, incoming: &Self) -> bool {
        if self.no_payload {
            // Live alternatives own descriptors and share immutable target paths.
            *self = if incoming.no_payload {
                Self::no_payload()
            } else {
                incoming.clone()
            };

            return false;
        }

        !incoming.no_payload
    }

    fn merge_contents(&mut self, incoming: &Self, completeness: StorageDependencyCompleteness) {
        match (&mut self.address, &incoming.address) {
            (Some(current), Some(incoming)) => { current.merge(incoming); }
            (None, Some(incoming)) => {
                // Each captured binding owns alternatives and shares immutable target paths.
                let mut address = incoming.clone();

                if self.completeness.tracked() || self.address_bounds.is_some() {
                    address.bound();
                }

                self.address = Some(address);
            }
            (Some(current), None) if incoming.completeness.tracked() || incoming.address_bounds.is_some() => current.bound(),
            _ => {}
        }

        self.sources.extend(incoming.sources.iter().cloned());
        self.completeness = completeness;

        if let Some(bounds) = &incoming.address_bounds {
            self.include_address_bound(bounds);
        }
    }
}

impl From<StorageAddressValue> for StorageValueBinding {
    fn from(mut address: StorageAddressValue) -> Self {
        // A published known value with no nonempty target is explicitly empty.
        if address.has_values() && address.targets.is_empty() {
            address.nullable = true;
        }

        Self {
            address: Some(address),
            ..Self::default()
        }
    }
}
