use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_base::shared_slice;

use crate::{
    AnyBoundNodeId, BoundExpressionId, BoundUnitId, BoundUnitKind,
    PatternPredicate, StorageAccessId, StoragePlan, StorageRelationship,
};

/// The outermost storage boundary reached by an operation's checked writes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageMutationScope {
    /// Only inline slots owned by this invocation can change.
    Invocation,
    /// Direct mutable input referents may also change.
    Inputs,
    /// The operation can reach storage outside the supported observation frame.
    Unbounded,
}

/// Possible writes observed through one exact physical target.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StoragePointeeChange {
    /// The entire view may have changed.
    Whole,
    /// The changed inline components of the checked type.
    Components {
        /// The type of the enclosing pointee.
        ty: bray_symbols::TypeId,
        /// Nonempty paths relative to the enclosing pointee.
        paths: BTreeSet<Arc<[crate::StorageProjection]>>,
    },
}

impl StoragePointeeChange {
    fn merge(&mut self, incoming: &Self) {
        if let (
            Self::Components { ty, paths },
            Self::Components { ty: incoming_ty, paths: incoming_paths },
        ) = (&mut *self, incoming)
            && ty == incoming_ty
        {
            paths.extend(incoming_paths.iter().cloned());
        } else {
            *self = Self::Whole;
        }
    }
}

/// Storage whose previously observed contents may change at one execution site.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageInvalidation {
    /// An opaque operation can reach storage through untracked aliases.
    All {
        /// Unexposed descriptor slots retain identity, without retaining their reachable contents.
        stable_descriptors: BTreeSet<crate::StoragePath>,
        /// Classified inline observations retain their value or only their discriminant.
        stable_values: BTreeMap<crate::StoragePath, crate::StorageObservationKind>,
        /// Physical reference slots retained independently of their current logical aliases.
        stable_reference_slots: BTreeSet<crate::StorageTarget>,
        /// Classified unchanged values held by this invocation's entry or normal result.
        normal_snapshots: BTreeMap<(crate::StorageSnapshotRoot, Arc<[crate::StorageProjection]>), crate::StorageObservationKind>,
        /// Surviving observations of exact typed pointees, without ownership or initialization.
        stable_pointees: BTreeMap<(crate::StorageTarget, bray_symbols::TypeId), BTreeSet<(Arc<[crate::StorageProjection]>, crate::StorageObservationKind)>>,
    },
    /// Execution reached its boundary without changing storage observations.
    Unchanged,
    /// Flow-resolved access paths and their current physical observations.
    /// Access paths are logical. Snapshot targets retain their established origins.
    Observed {
        /// Change classification resolved with this same incoming observation frame.
        transition: crate::StorageObservationTransition,
        /// Mutation confinement established while resolving the actual target identities.
        scope: StorageMutationScope,
        /// Logical paths whose contents may change, indexed by their checked access.
        accesses: BTreeMap<StorageAccessId, Vec<crate::StoragePath>>,
        /// Exact current observations mapped to their physical representative.
        /// Keep the origin, epoch and borrow layer until the consumer interprets them.
        /// These identities preserve no entry facts and establish no guarantees.
        snapshots: BTreeMap<crate::StoragePath, crate::StorageObservedTarget>,
        /// Checked changes to each physical pointee. An unknown frame can affect every pointee.
        affected_pointees: Option<BTreeMap<crate::StorageTarget, StoragePointeeChange>>,
        /// References whose reachable contents may change while their address value survives.
        referents: BTreeSet<crate::StoragePath>,
        /// Slice descriptors proven disjoint from all writes to their reachable elements.
        stable_descriptors: BTreeSet<crate::StoragePath>,
        /// Classified component values or discriminants proven unchanged by these writes.
        stable_values: BTreeMap<crate::StoragePath, crate::StorageObservationKind>,
        /// Physical reference slots retained independently of their current logical aliases.
        stable_reference_slots: BTreeSet<crate::StorageTarget>,
        /// Classified unchanged values held by this invocation's entry or normal result.
        normal_snapshots: BTreeMap<(crate::StorageSnapshotRoot, Arc<[crate::StorageProjection]>), crate::StorageObservationKind>,
        /// Surviving observations of exact typed pointees, without ownership or initialization.
        stable_pointees: BTreeMap<(crate::StorageTarget, bray_symbols::TypeId), BTreeSet<(Arc<[crate::StorageProjection]>, crate::StorageObservationKind)>>,
    },
}

fn intersect_preserved_values(
    retained: &mut BTreeMap<crate::StoragePath, crate::StorageObservationKind>,
    incoming: &BTreeMap<crate::StoragePath, crate::StorageObservationKind>,
    unchanged: impl Fn(&crate::StoragePath) -> bool,
) {
    retained.retain(|path, kind| {
        if let Some(other) = incoming.get(path) {
            if *kind == *other || *other == crate::StorageObservationKind::Value {
                true
            } else if *kind == crate::StorageObservationKind::Value {
                *kind = *other;

                true
            } else { false }
        } else { unchanged(path) }
    });
}

impl StorageInvalidation {
    pub(crate) fn is_valid_for(&self, unit: BoundUnitId) -> bool {
        match self {
            Self::All {
                stable_descriptors,
                stable_values,
                stable_reference_slots,
                ..
            } => {
                stable_reference_slots.iter().all(|target| target.is_valid_for(unit))
                    && stable_descriptors.iter().chain(stable_values.keys()).all(|path| path.is_valid_for(unit))
            }
            Self::Unchanged => true,
            Self::Observed {
                accesses,
                snapshots,
                affected_pointees,
                referents,
                stable_descriptors,
                stable_values,
                stable_reference_slots,
                ..
            } => {
                stable_reference_slots.iter().all(|target| target.is_valid_for(unit))
                    && accesses.iter().all(|(access, paths)| {
                        access.unit() == unit && paths.iter().all(|path| path.is_valid_for(unit))
                    })
                    && snapshots.iter().all(|(path, observed)| path.is_valid_for(unit) && observed.target.is_valid_for(unit))
                    && affected_pointees.iter().flatten().all(|(target, change)| {
                        target.is_valid_for(unit) && match change {
                            StoragePointeeChange::Whole => true,
                            StoragePointeeChange::Components { paths, .. } => !paths.is_empty() && paths.iter().all(|path| {
                                !path.is_empty() && crate::StoragePath::inline_projections(path)
                                    && path.iter().all(|projection| projection.is_exact())
                            }),
                        }
                    })
                    && referents.iter().chain(stable_descriptors).chain(stable_values.keys())
                        .all(|path| path.is_valid_for(unit))
            }
        }
    }

    /// Whether the checked transition reached its boundary without changing storage observations.
    pub fn is_unchanged(&self) -> bool {
        matches!(self, Self::Unchanged)
    }

    /// Returns the change classification resolved by composite flow.
    pub fn observation_transition(&self) -> crate::StorageObservationTransition {
        match self {
            Self::Observed { transition, .. } => *transition,
            Self::All { .. } => crate::StorageObservationTransition::Unknown,
            Self::Unchanged => crate::StorageObservationTransition::Unchanged,
        }
    }

    /// Attaches the operation classification after resolving its observation frame.
    /// Opaque frames retain the conservative global invalidation semantics.
    pub fn with_observation_transition(mut self, change: crate::StorageObservationTransition) -> Self {
        match &mut self {
            Self::Observed { transition, .. } => *transition = change,
            Self::All { .. } | Self::Unchanged => {}
        }

        self
    }

    /// Returns the confinement established by the owning storage-flow analysis.
    pub fn mutation_scope(&self) -> StorageMutationScope {
        match self {
            Self::Observed { scope, .. } => *scope,
            Self::All { .. } => StorageMutationScope::Unbounded,
            Self::Unchanged => StorageMutationScope::Invocation,
        }
    }

    /// Creates an opaque effect without any established preservation.
    pub fn unknown() -> Self {
        Self::All {
            stable_descriptors: BTreeSet::new(),
            stable_values: Default::default(),
            stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
        }
    }

    /// Retains every target that either incoming execution can mutate.
    pub fn merge(&mut self, incoming: &Self) {
        if incoming.is_unchanged() {
            return;
        }

        if self.is_unchanged() {
            *self = incoming.clone();

            return;
        }

        let previous = self.observation_transition();
        let next = incoming.observation_transition();

        use crate::StorageObservationTransition as Change;

        let transition = if previous == Change::Unchanged {
            next
        } else if previous == next || next == Change::Unchanged {
            previous
        } else if previous.changes_contents_only() && next.changes_contents_only() {
            Change::Contents
        } else { Change::Unknown };

        let mut pointees = match &mut *self {
            Self::All { stable_pointees, .. } | Self::Observed { stable_pointees, .. } => std::mem::take(stable_pointees),
            Self::Unchanged => BTreeMap::new(),
        };

        if previous == Change::Unchanged {
            pointees = incoming.stable_pointees().cloned().unwrap_or_default();
        } else if next != Change::Unchanged {
            pointees.retain(|key, observations| {
                let Some(other) = incoming.stable_pointees().and_then(|pointees| pointees.get(key)) else {
                    return false;
                };

                observations.retain(|observation| other.contains(observation));

                !observations.is_empty()
            });
        }

        let mut reference_slots = match &mut *self {
            Self::All { stable_reference_slots, .. } | Self::Observed { stable_reference_slots, .. } => std::mem::take(stable_reference_slots),
            Self::Unchanged => BTreeSet::new(),
        };

        if previous == Change::Unchanged {
            if let Self::All { stable_reference_slots, .. } | Self::Observed { stable_reference_slots, .. } = incoming {
                reference_slots.clone_from(stable_reference_slots);
            }
        } else if next != Change::Unchanged {
            reference_slots.retain(|slot| incoming.preserves_reference_slot(slot));
        }

        let mut normal_snapshots = match &mut *self {
            Self::All { normal_snapshots, .. } | Self::Observed { normal_snapshots, .. } =>
                std::mem::take(normal_snapshots),
            Self::Unchanged => BTreeMap::new(),
        };

        normal_snapshots.retain(|subject, kind| incoming.normal_snapshots()
            .is_some_and(|incoming| incoming.get(subject) == Some(kind)));

        match (&mut *self, incoming) {
            (
                Self::All {
                    stable_descriptors,
                    stable_values,
                    ..
                },
                Self::All {
                    stable_descriptors: descriptors,
                    stable_values: values,
                    ..
                },
            ) => {
                stable_descriptors.retain(|path| descriptors.contains(path));
                intersect_preserved_values(stable_values, values, |_| false);
            }
            (
                Self::All {
                    stable_descriptors,
                    stable_values,
                    ..
                },
                Self::Observed {
                    accesses,
                    referents,
                    stable_descriptors: descriptors,
                    stable_values: values,
                    ..
                },
            ) => {
                // A resolved effect preserves paths outside its closed write set.
                let unchanged = |path: &crate::StoragePath| accesses.values().flatten()
                    .all(|changed| changed.relationship(path) == StorageRelationship::Disjoint);

                stable_descriptors.retain(|path| descriptors.contains(path) || unchanged(path));

                intersect_preserved_values(stable_values, values, |path| unchanged(path)
                    && referents.iter().all(|changed| changed.relationship(path) == StorageRelationship::Disjoint));
            }
            (Self::Observed { .. }, Self::All { .. }) => {
                // Start from the opaque frame, including paths the finite effect
                // preserves implicitly rather than listing as explicit evidence.
                let resolved = std::mem::replace(self, incoming.clone());

                self.merge(&resolved);
            }
            (_, Self::All { .. }) | (Self::All { .. }, _) => *self = Self::unknown(),
            (
                Self::Observed {
                    scope,
                    accesses: current,
                    snapshots,
                    affected_pointees,
                    referents,
                    stable_descriptors,
                    stable_values,
                    ..
                },
                Self::Observed {
                    scope: incoming_scope,
                    accesses: incoming,
                    snapshots: incoming_snapshots,
                    affected_pointees: incoming_pointees,
                    referents: incoming_referents,
                    stable_descriptors: incoming_descriptors,
                    stable_values: incoming_values,
                    ..
                },
            ) => {
                *scope = (*scope).max(*incoming_scope);

                let disjoint = |path: &crate::StoragePath, accesses: &BTreeMap<StorageAccessId, Vec<crate::StoragePath>>| {
                    accesses.values().flatten().all(|changed| changed.relationship(path) == StorageRelationship::Disjoint)
                };

                let descriptors = incoming_descriptors.iter().filter(|path| !stable_descriptors.contains(*path) && disjoint(path, current)).cloned().collect::<Vec<_>>();

                let values = incoming_values.iter().filter(|(path, _)| !stable_values.contains_key(*path) && disjoint(path, current)
                    && referents.iter().all(|changed| changed.relationship(path) == StorageRelationship::Disjoint)).map(|(path, kind)| (path.clone(), *kind)).collect::<Vec<_>>();

                stable_descriptors.retain(|path| incoming_descriptors.contains(path) || disjoint(path, incoming));

                intersect_preserved_values(stable_values, incoming_values, |path| disjoint(path, incoming)
                    && incoming_referents.iter().all(|changed| changed.relationship(path) == StorageRelationship::Disjoint));

                stable_descriptors.extend(descriptors);
                stable_values.extend(values);
                snapshots.retain(|path, target| incoming_snapshots.get(path) == Some(target));
                referents.extend(incoming_referents.iter().cloned());

                if let (Some(current), Some(incoming)) = (&mut *affected_pointees, incoming_pointees) {
                    for (target, paths) in incoming {
                        current.entry(target.clone()).and_modify(|change| change.merge(paths)).or_insert_with(|| paths.clone());
                    }
                } else {
                    *affected_pointees = None;
                }

                for (access, paths) in incoming {
                    let changed = current.entry(*access).or_default();

                    changed.extend_from_slice(paths);
                    changed.sort_unstable();
                    changed.dedup();
                }
            }
            (Self::Unchanged, _) | (_, Self::Unchanged) => unreachable!("unchanged frames were handled before merging"),
        }

        if let Self::All { normal_snapshots: preserved, .. }
            | Self::Observed { normal_snapshots: preserved, .. } = self {
            *preserved = normal_snapshots;
        }

        if let Self::All { stable_reference_slots, .. } | Self::Observed { stable_reference_slots, .. } = self {
            *stable_reference_slots = reference_slots;
        }

        if let Self::All { stable_pointees, .. } | Self::Observed { stable_pointees, .. } = self {
            *stable_pointees = pointees;
        }

        if let Self::Observed { transition: preserved, .. } = self { *preserved = transition; }
    }

    /// Returns physical observation preservation resolved by storage flow.
    pub fn stable_pointees(&self) -> Option<&BTreeMap<(crate::StorageTarget, bray_symbols::TypeId), BTreeSet<(Arc<[crate::StorageProjection]>, crate::StorageObservationKind)>>> {
        match self {
            Self::All { stable_pointees, .. } | Self::Observed { stable_pointees, .. } => Some(stable_pointees),
            Self::Unchanged => None,
        }
    }

    /// Retains a reference's actual slot as well as its logical observation path.
    /// The physical identity remains distinct after a containing reference is reassigned.
    pub fn preserve_reference_slot(&mut self, logical: crate::StoragePath, physical: crate::StorageTarget) {
        match self {
            Self::All { stable_descriptors, stable_reference_slots, .. }
            | Self::Observed { stable_descriptors, stable_reference_slots, .. } => {
                stable_descriptors.insert(logical);
                stable_reference_slots.insert(physical);
            }
            Self::Unchanged => panic!("reference-slot preservation requires flow-resolved effects"),
        }
    }

    /// Whether this operation preserves the descriptor at this exact physical slot.
    pub fn preserves_reference_slot(&self, slot: &crate::StorageTarget) -> bool {
        match self {
            Self::All { stable_reference_slots, .. } | Self::Observed { stable_reference_slots, .. } => stable_reference_slots.contains(slot),
            Self::Unchanged => false,
        }
    }

    /// Returns the original observation kinds of captured invocation snapshots.
    pub fn normal_snapshots(&self) -> Option<&BTreeMap<(crate::StorageSnapshotRoot, Arc<[crate::StorageProjection]>), crate::StorageObservationKind>> {
        match self {
            Self::All { normal_snapshots, .. } | Self::Observed { normal_snapshots, .. } => Some(normal_snapshots),
            Self::Unchanged => None,
        }
    }

    /// Returns whether this operation can change the independently resolved path.
    pub fn invalidates_path(&self, path: &crate::StoragePath) -> bool {
        match self {
            Self::All { .. } => true,
            Self::Observed { stable_values, .. }
                if stable_values.iter().any(|(stable, kind)| kind.is_immutable_inline() && stable.contains(path)) =>
            {
                false
            }
            Self::Observed { accesses, .. } => accesses
                .values()
                .flatten()
                .any(|changed| changed.relationship(path) != StorageRelationship::Disjoint),
            Self::Unchanged => false,
        }
    }

    /// Returns whether the resolved change invalidates a nullable or union discriminant.
    pub fn invalidates_discriminant(&self, path: &crate::StoragePath, observed: bool) -> bool {
        let stable_tag = |values: &BTreeMap<_, crate::StorageObservationKind>| {
            values.get(path).is_some_and(|kind| {
                *kind == crate::StorageObservationKind::Discriminant || kind.is_immutable_inline()
            })
        };

        match self {
            Self::All { stable_descriptors, stable_values, .. } => {
                !observed || !(stable_tag(stable_values) || stable_descriptors.contains(path)
                    || stable_descriptors.contains(&crate::StoragePath::root(path.root)))
            }
            Self::Observed { accesses, stable_descriptors, stable_values, .. } => {
                if observed && (stable_tag(stable_values) || stable_descriptors.contains(path)) {
                    return false;
                }

                accesses.values().flatten().any(|changed| {
                    changed.projections.len() <= path.projections.len()
                        && changed.relationship(path) != StorageRelationship::Disjoint
                })
            }
            Self::Unchanged => false,
        }
    }

    /// Returns whether this operation invalidates facts about the given access.
    pub fn invalidates(&self, access: StorageAccessId, storage: &StoragePlan) -> bool {
        match self {
            Self::All { .. } => true,
            Self::Observed {
                accesses,
                stable_values,
                ..
            } => {
                accesses.contains_key(&access)
                    && !storage.logical_path(access).is_some_and(|path| {
                        stable_values.iter().any(|(stable, kind)| kind.is_immutable_inline() && stable.contains(path))
                    })
            }
            Self::Unchanged => false,
        }
    }
}

/// One flow-sensitive refinement established while checking a bound unit.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RefinementKind {
    /// One boolean expression has the stated value on the current path.
    Condition {
        /// The checked boolean expression.
        expression: BoundExpressionId,
        /// The value established by control flow.
        value: bool,
    },
    /// One nullable expression is known to be present or absent.
    NullablePresence {
        /// The checked nullable expression.
        expression: BoundExpressionId,
        /// Whether the nullable value is present.
        is_present: bool,
    },
    /// A checked structural predicate is known to hold or fail at its exact input access.
    Pattern {
        /// The matched subject expression.
        subject: BoundExpressionId,
        /// The structural refinement established by that pattern.
        predicate: PatternPredicate,
        /// The evaluated storage reached by this pattern, including any field projections.
        access: StorageAccessId,
        /// Whether the predicate holds.
        value: bool,
    },
    /// The operand is currently inside this explicit trust boundary.
    TrustBoundary(BoundExpressionId),
}

impl RefinementKind {
    const fn is_valid_for(self, unit: BoundUnitId) -> bool {
        match self {
            Self::Condition { expression, .. }
            | Self::NullablePresence { expression, .. }
            | Self::TrustBoundary(expression) => expression.unit().raw() == unit.raw(),
            Self::Pattern {
                subject, access, ..
            } => subject.unit().raw() == unit.raw() && access.unit().raw() == unit.raw(),
        }
    }
}

/// One refinement and the evaluated storage accesses it depends on.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Refinement {
    kind: RefinementKind,
    dependencies: Arc<[StorageAccessId]>,
}

impl Refinement {
    /// Creates one refinement with canonical storage dependencies.
    pub fn new(
        kind: RefinementKind,
        dependencies: impl IntoIterator<Item = StorageAccessId>,
    ) -> Self {
        let mut dependencies = dependencies.into_iter().collect::<Vec<_>>();

        dependencies.sort_unstable();
        dependencies.dedup();

        Self {
            kind,
            dependencies: shared_slice(dependencies),
        }
    }

    /// Returns the refinement established by control flow.
    pub const fn kind(&self) -> RefinementKind {
        self.kind
    }

    /// Returns the evaluated storage accesses whose values support this refinement.
    pub fn dependencies(&self) -> &[StorageAccessId] {
        &self.dependencies
    }
}

/// Refinements available immediately before one bound operation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RefinementOccurrence {
    node: AnyBoundNodeId,
    refinements: Arc<[Refinement]>,
}

impl RefinementOccurrence {
    /// Creates one operation occurrence with refinements in canonical order.
    pub fn new(node: AnyBoundNodeId, refinements: impl IntoIterator<Item = Refinement>) -> Self {
        let mut refinements = refinements.into_iter().collect::<Vec<_>>();

        refinements.sort_unstable();
        refinements.dedup();

        Self {
            node,
            refinements: shared_slice(refinements),
        }
    }

    /// Returns the bound operation occurrence.
    pub const fn node(&self) -> AnyBoundNodeId {
        self.node
    }

    /// Returns refinements known immediately before this operation.
    pub fn refinements(&self) -> &[Refinement] {
        &self.refinements
    }
}

/// An invalid durable refinement table.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RefinementSetBuildError {
    /// An occurrence or refinement belongs to another bound unit.
    ForeignUnit,
    /// The same bound occurrence was published more than once.
    DuplicateOccurrence,
}

/// Immutable flow-sensitive refinements retained for checked operation occurrences.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedRefinements {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    occurrences: Arc<[RefinementOccurrence]>,
    is_recovered: bool,
}

impl CheckedRefinements {
    /// Validates and creates one durable refinement table.
    pub fn try_new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        occurrences: impl IntoIterator<Item = RefinementOccurrence>,
        is_recovered: bool,
    ) -> Result<Self, RefinementSetBuildError> {
        let mut occurrences = occurrences.into_iter().collect::<Vec<_>>();

        if occurrences.iter().any(|occurrence| {
            occurrence.node().unit() != unit
                || occurrence.refinements().iter().any(|refinement| {
                    !refinement.kind().is_valid_for(unit)
                        || refinement
                            .dependencies()
                            .iter()
                            .any(|dependency| dependency.unit() != unit)
                })
        }) {
            return Err(RefinementSetBuildError::ForeignUnit);
        }

        occurrences.sort_unstable_by_key(RefinementOccurrence::node);

        if occurrences
            .windows(2)
            .any(|pair| pair[0].node() == pair[1].node())
        {
            return Err(RefinementSetBuildError::DuplicateOccurrence);
        }

        Ok(Self {
            unit,
            kind,
            occurrences: occurrences.into(),
            is_recovered,
        })
    }

    /// Returns the exact bound unit described by these refinements.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the semantic category of the checked bound unit.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns operation occurrences in bound node identity order.
    pub fn occurrences(&self) -> &[RefinementOccurrence] {
        &self.occurrences
    }

    /// Returns refinements known immediately before one exact operation.
    pub fn refinements_before(&self, node: AnyBoundNodeId) -> &[Refinement] {
        self.occurrences
            .binary_search_by_key(&node, RefinementOccurrence::node)
            .ok()
            .map(|index| self.occurrences[index].refinements())
            .unwrap_or_default()
    }

    /// Returns whether recovery prevented complete refinement evidence.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CheckedRefinements, Refinement, RefinementKind, RefinementOccurrence,
        RefinementSetBuildError, StoragePointeeChange,
    };
    use crate::{
        AnyBoundNodeId, BoundExpressionId, BoundUnitId, BoundUnitKind, PatternPredicate,
        StorageAccessId,
    };
    use std::collections::BTreeSet;

    #[test]
    fn preserved_values_subsume_tags_and_effect_merges_keep_only_common_survival() {
        use crate::StorageObservationKind;

        let path = crate::StoragePath::root(crate::StorageIdentityId::from_slot(BoundUnitId::new(0), 0));

        let mut descriptors = BTreeSet::new();
        let mut contents = std::collections::BTreeMap::new();

        StorageObservationKind::Identity.record_preservation(path.clone(), &mut descriptors, &mut contents);
        assert!(descriptors.contains(&path));
        assert!(contents.is_empty(), "identity survival does not preserve reached contents");

        for weaker in [StorageObservationKind::Discriminant, StorageObservationKind::Address] {
        for kinds in [[StorageObservationKind::Value, weaker], [weaker, StorageObservationKind::Value]] {
            let mut values = std::collections::BTreeMap::new();
            let mut descriptors = BTreeSet::new();

            for kind in kinds { kind.record_preservation(path.clone(), &mut descriptors, &mut values); }

            assert_eq!(values[&path], StorageObservationKind::Value);

            let mut retained = super::StorageInvalidation::All {
                stable_descriptors: Default::default(), stable_values: values, stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
            };

            let tag = super::StorageInvalidation::All {
                stable_descriptors: Default::default(),
                stable_values: [(path.clone(), weaker)].into(),
                stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
            };

            retained.merge(&tag);
            assert_eq!(retained, tag);
        }
        }
    }

    #[test]
    fn checked_refinements_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CheckedRefinements>();
    }

    #[test]
    fn checked_refinements_canonicalize_refinements_and_occurrences() {
        let unit = BoundUnitId::new(4);
        let first = BoundExpressionId::from_slot(unit, 0);
        let second = BoundExpressionId::from_slot(unit, 1);
        let dependency = StorageAccessId::from_slot(unit, 0);

        let refinement = Refinement::new(
            RefinementKind::Condition {
                expression: first,
                value: true,
            },
            [dependency, dependency],
        );

        let result = CheckedRefinements::try_new(
            unit,
            BoundUnitKind::CallableBody,
            [
                RefinementOccurrence::new(second.into(), [refinement.clone(), refinement.clone()]),
                RefinementOccurrence::new(first.into(), []),
            ],
            false,
        )
        .unwrap_or_else(|error| panic!("valid refinements must build: {error:?}"));

        assert_eq!(
            result
                .occurrences()
                .iter()
                .map(RefinementOccurrence::node)
                .collect::<Vec<_>>(),
            [
                AnyBoundNodeId::Expression(first),
                AnyBoundNodeId::Expression(second)
            ]
        );

        assert_eq!(result.refinements_before(second.into()), &[refinement]);
        assert_eq!(result.refinements_before(first.into()), []);
    }

    #[test]
    fn checked_refinements_reject_foreign_refinements_and_duplicate_occurrences() {
        let unit = BoundUnitId::new(4);
        let expression = BoundExpressionId::from_slot(unit, 0);
        let foreign = BoundExpressionId::from_slot(BoundUnitId::new(5), 0);
        let foreign_access = StorageAccessId::from_slot(BoundUnitId::new(5), 0);

        let foreign_refinement = Refinement::new(
            RefinementKind::Condition {
                expression: foreign,
                value: true,
            },
            [],
        );

        let foreign_pattern_refinement = Refinement::new(
            RefinementKind::Pattern {
                subject: expression,
                predicate: PatternPredicate::NullablePresent,
                access: foreign_access,
                value: true,
            },
            [],
        );

        assert_eq!(
            CheckedRefinements::try_new(
                unit,
                BoundUnitKind::CallableBody,
                [RefinementOccurrence::new(
                    expression.into(),
                    [foreign_refinement]
                )],
                false,
            ),
            Err(RefinementSetBuildError::ForeignUnit)
        );

        assert_eq!(
            CheckedRefinements::try_new(
                unit,
                BoundUnitKind::CallableBody,
                [RefinementOccurrence::new(
                    expression.into(),
                    [foreign_pattern_refinement]
                )],
                false,
            ),
            Err(RefinementSetBuildError::ForeignUnit)
        );

        assert_eq!(
            CheckedRefinements::try_new(
                unit,
                BoundUnitKind::CallableBody,
                [
                    RefinementOccurrence::new(expression.into(), []),
                    RefinementOccurrence::new(expression.into(), []),
                ],
                false,
            ),
            Err(RefinementSetBuildError::DuplicateOccurrence)
        );
    }
    #[test]
    fn published_effects_reject_foreign_path_selectors() {
        let unit = BoundUnitId::new(4);

        let path = crate::StoragePath {
            root: crate::StorageIdentityId::from_slot(unit, 0),
            projections: [crate::StorageProjection::Element(
                BoundExpressionId::from_slot(BoundUnitId::new(5), 0),
            )]
            .into(),
        };

        let mutation = super::StorageInvalidation::Observed {
            transition: Default::default(),
            scope: super::StorageMutationScope::Invocation,
            affected_pointees: None,
            accesses: [(StorageAccessId::from_slot(unit, 0), vec![path])].into(),
            snapshots: Default::default(),
            referents: Default::default(),
            stable_descriptors: Default::default(),
            stable_values: Default::default(),
            stable_reference_slots: Default::default(),
            normal_snapshots: Default::default(),
            stable_pointees: Default::default(),
        };

        assert!(!mutation.is_valid_for(unit));
    }

    #[test]
    fn observation_merges_union_effects_and_intersect_exact_aliases() {
        let unit = BoundUnitId::new(4);
        let access = StorageAccessId::from_slot(unit, 0);
        let first = crate::StoragePath::root(crate::StorageIdentityId::from_slot(unit, 0));
        let second = crate::StoragePath::root(crate::StorageIdentityId::from_slot(unit, 1));
        let alias = crate::StoragePath::root(crate::StorageIdentityId::from_slot(unit, 2));
        let observation = |path| crate::StorageObservedTarget::direct(crate::StorageTarget::storage(path));

        let mut left = super::StorageInvalidation::Observed {
            transition: crate::StorageObservationTransition::Contents,
            scope: super::StorageMutationScope::Unbounded,
            affected_pointees: Some([(crate::StorageTarget::storage(first.clone()), StoragePointeeChange::Whole)].into()),
            accesses: [(access, vec![first.clone()])].into(),
            snapshots: [
                (first.clone(), observation(first.clone())),
                (alias.clone(), observation(first.clone())),
            ]
            .into(),
            referents: [first.clone()].into(),
            stable_descriptors: [first.clone(), second.clone()].into(),
            stable_values: [first.clone(), second.clone()].into_iter().map(|path| (path, crate::StorageObservationKind::Value)).collect(),
            stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
        };

        let right = super::StorageInvalidation::Observed {
            transition: crate::StorageObservationTransition::Contents,
            scope: super::StorageMutationScope::Unbounded,
            affected_pointees: Some([(crate::StorageTarget::storage(second.clone()), StoragePointeeChange::Whole)].into()),
            accesses: [(access, vec![second.clone()])].into(),
            snapshots: [
                (first.clone(), observation(first.clone())),
                (alias, observation(second.clone())),
            ]
            .into(),
            referents: [second.clone()].into(),
            stable_descriptors: [first.clone()].into(),
            stable_values: [first.clone()].into_iter().map(|path| (path, crate::StorageObservationKind::Value)).collect(),
            stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
        };

        left.merge(&right);

        let opaque = super::StorageInvalidation::All {
            stable_descriptors: [first.clone()].into(),
            stable_values: [first.clone()].into_iter().map(|path| (path, crate::StorageObservationKind::Value)).collect(),
            stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
        };

        let mut global = opaque.clone();
        let mut resolved = left.clone();

        global.merge(&left);
        resolved.merge(&opaque);
        assert_eq!(global, opaque);
        assert_eq!(resolved, opaque);

        assert_eq!(
            left,
            super::StorageInvalidation::Observed {
                transition: crate::StorageObservationTransition::Contents,
                scope: super::StorageMutationScope::Unbounded,
                affected_pointees: Some([
                    (crate::StorageTarget::storage(first.clone()), StoragePointeeChange::Whole),
                    (crate::StorageTarget::storage(second.clone()), StoragePointeeChange::Whole),
                ].into()),
                accesses: [(access, vec![first.clone(), second.clone()])].into(),
                snapshots: [(first.clone(), observation(first.clone()))].into(),
                stable_descriptors: [first.clone()].into(),
                stable_values: [first.clone()].into_iter().map(|path| (path, crate::StorageObservationKind::Value)).collect(),
                stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
                referents: [first, second].into(),
            }
        );

        left.merge(&right.with_observation_transition(crate::StorageObservationTransition::Transfer));

        assert_eq!(left.observation_transition(), crate::StorageObservationTransition::Unknown,
            "a join cannot turn different changes into unconditional contents preservation");
    }
    #[test]
    fn closed_effect_merges_preserve_disjoint_observations_in_both_orders() {
        let unit = BoundUnitId::new(4);
        let owner = crate::StoragePath::root(crate::StorageIdentityId::from_slot(unit, 0));

        let field = |ordinal| crate::StoragePath {
            root: owner.root,
            projections: [crate::StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(ordinal))].into(),
        };

        let changed = field(0);
        let sibling = field(1);
        let other = crate::StoragePath::root(crate::StorageIdentityId::from_slot(unit, 1));
        let retained = [owner, changed.clone(), sibling.clone(), other.clone()].into_iter().collect::<BTreeSet<_>>();

        let precise = super::StorageInvalidation::Observed {
            transition: Default::default(),
            scope: super::StorageMutationScope::Unbounded,
            affected_pointees: None,
            accesses: [(StorageAccessId::from_slot(unit, 0), vec![changed])].into(),
            snapshots: Default::default(),
            referents: [other.clone()].into(),
            stable_descriptors: Default::default(),
            stable_values: Default::default(),
            stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
        };

        for opaque in [false, true] {
            let frame = if opaque {
                super::StorageInvalidation::All {
                    stable_descriptors: retained.clone(),
                    stable_values: retained.iter().cloned().map(|path| (path, crate::StorageObservationKind::Value)).collect(),
                    stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
                }
            } else {
                super::StorageInvalidation::Observed {
                    transition: Default::default(),
                    scope: super::StorageMutationScope::Unbounded,
                    affected_pointees: None,
                    accesses: Default::default(),
                    snapshots: Default::default(),
                    referents: Default::default(),
                    stable_descriptors: retained.clone(),
                    stable_values: retained.iter().cloned().map(|path| (path, crate::StorageObservationKind::Value)).collect(),
                    stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
                }
            };

            let mut left = frame.clone();
            let mut right = precise.clone();

            left.merge(&precise);
            right.merge(&frame);
            assert_eq!(left, right);

            let (super::StorageInvalidation::All { stable_descriptors, stable_values, .. }
                | super::StorageInvalidation::Observed { stable_descriptors, stable_values, .. }) = left else {
                panic!("resolved merges retain an observation frame");
            };

            let expected = [sibling.clone(), other.clone()].into_iter().collect::<BTreeSet<_>>();

            assert_eq!(stable_descriptors, expected);
            assert_eq!(stable_values, [(sibling.clone(), crate::StorageObservationKind::Value)].into());
        }
    }

    #[test]
    fn opaque_effects_intersect_descriptor_preservation() {
        let unit = BoundUnitId::new(4);
        let first = crate::StoragePath::root(crate::StorageIdentityId::from_slot(unit, 0));
        let second = crate::StoragePath::root(crate::StorageIdentityId::from_slot(unit, 1));

        let mut effects = super::StorageInvalidation::All {
            stable_descriptors: [first.clone(), second].into(),
            stable_values: Default::default(),
            stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
        };

        effects.merge(&super::StorageInvalidation::All {
            stable_descriptors: [first.clone()].into(),
            stable_values: Default::default(),
            stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
        });

        assert_eq!(
            effects,
            super::StorageInvalidation::All {
                stable_descriptors: [first.clone()].into(),
                stable_values: Default::default(),
                stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default()
            }
        );

        let physical = crate::StorageTarget { previous: false, origin: crate::StorageOrigin::entry(first.clone(), bray_symbols::DependencyReferenceKind::Borrow) };
        let mut retained = super::StorageInvalidation::unknown();

        retained.preserve_reference_slot(first.clone(), physical.clone());

        let mut same = retained.clone();

        same.merge(&retained);
        assert_eq!(same, retained, "physical preservation stabilizes on replay");

        let mut retired = physical.clone();

        retired.previous = true;

        let mut different = super::StorageInvalidation::unknown();

        different.preserve_reference_slot(first.clone(), retired.clone());

        for (mut left, right) in [(retained.clone(), different.clone()), (different, retained)] {
            left.merge(&right);
            assert!(!left.preserves_reference_slot(&physical));
            assert!(!left.preserves_reference_slot(&retired), "different physical epochs cannot retain each other");
        }

    }
}
