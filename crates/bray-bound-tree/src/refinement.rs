use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_base::shared_slice;

use crate::{
    AnyBoundNodeId, BoundExecutionSite, BoundExpressionId, BoundPatternId, BoundUnitId,
    BoundUnitKind, PatternPredicate, StorageAccessId, StoragePlan, StorageRelationship,
};

/// Storage whose previously observed contents may change at one execution site.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum StorageInvalidation {
    /// An opaque operation can reach storage through untracked aliases.
    All,
    /// Only the listed paths and overlapping storage may change.
    Paths(Vec<crate::StoragePath>),
    /// Flow-resolved accesses and logical component observations changed through them.
    /// Alias resolution is complete. Consumers must not reinterpret these as physical roots.
    Observed {
        /// Logical paths whose contents may change, indexed by their checked access.
        accesses: BTreeMap<StorageAccessId, Vec<crate::StoragePath>>,
        /// Exact current observations mapped to their physical representative.
        /// Self mappings start fresh contents; aliases read that updated snapshot.
        /// These identities preserve no entry facts and establish no guarantees.
        snapshots: BTreeMap<crate::StoragePath, crate::StoragePath>,
        /// References whose reachable contents may change while their address value survives.
        referents: BTreeSet<crate::StoragePath>,
    },
}

impl StorageInvalidation {
    /// Retains every target that either incoming execution can mutate.
    pub fn merge(&mut self, incoming: &Self) {
        match (&mut *self, incoming) {
            (_, Self::All) => *self = Self::All,
            (Self::All, _) => {}
            (
                Self::Observed {
                    accesses: current,
                    snapshots,
                    referents,
                },
                Self::Observed {
                    accesses: incoming,
                    snapshots: incoming_snapshots,
                    referents: incoming_referents,
                },
            ) => {
                snapshots.retain(|path, target| incoming_snapshots.get(path) == Some(target));
                referents.extend(incoming_referents.iter().cloned());

                for (access, paths) in incoming {
                    let changed = current.entry(*access).or_default();

                    changed.extend_from_slice(paths);
                    changed.sort_unstable();
                    changed.dedup();
                }
            }
            (Self::Paths(_), Self::Observed { .. }) | (Self::Observed { .. }, Self::Paths(_)) => {
                panic!("one mutation publication must retain its owning analysis representation");
            }
            (Self::Paths(current), Self::Paths(incoming)) => {
                current.extend_from_slice(incoming);
                current.sort_unstable();
                current.dedup();
            }
        }
    }

    /// Returns whether this operation can change the independently resolved path.
    pub fn invalidates_path(&self, path: &crate::StoragePath, storage: &StoragePlan) -> bool {
        match self {
            Self::All => true,
            Self::Observed { accesses, .. } => accesses
                .values()
                .flatten()
                .any(|changed| changed.relationship(path) != StorageRelationship::Disjoint),
            Self::Paths(accesses) => accesses.iter().any(|access| {
                storage.path_relationship(access, path) != StorageRelationship::Disjoint
            }),
        }
    }

    /// Returns whether this operation invalidates facts about the given access.
    pub fn invalidates(&self, access: StorageAccessId, storage: &StoragePlan) -> bool {
        match self {
            Self::All => true,
            Self::Observed { accesses, .. } => accesses.contains_key(&access),
            Self::Paths(mutations) => mutations.iter().any(|mutation| {
                storage.relationship_to_path(access, mutation) != StorageRelationship::Disjoint
            }),
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
        /// The successful pattern occurrence.
        pattern: BoundPatternId,
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
                subject,
                pattern,
                access,
                ..
            } => {
                subject.unit().raw() == unit.raw()
                    && pattern.unit().raw() == unit.raw()
                    && access.unit().raw() == unit.raw()
            }
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
    invalidations: BTreeMap<BoundExecutionSite, StorageInvalidation>,
    is_recovered: bool,
}

impl CheckedRefinements {
    /// Validates and creates one durable refinement table.
    pub fn try_new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        occurrences: impl IntoIterator<Item = RefinementOccurrence>,
        invalidations: BTreeMap<BoundExecutionSite, StorageInvalidation>,
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

        if invalidations.iter().any(|(site, invalidation)| {
            site.node().unit() != unit
                || match invalidation {
                    StorageInvalidation::All => false,
                    StorageInvalidation::Paths(paths) => {
                        paths.iter().any(|path| !path.is_valid_for(unit))
                    }
                    StorageInvalidation::Observed {
                        accesses,
                        snapshots,
                        referents,
                    } => {
                        accesses.iter().any(|(access, paths)| {
                            access.unit() != unit
                                || paths.iter().any(|path| !path.is_valid_for(unit))
                        }) || snapshots.iter().any(|(path, target)| {
                            !path.is_valid_for(unit) || !target.is_valid_for(unit)
                        }) || referents.iter().any(|path| !path.is_valid_for(unit))
                    }
                }
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
            invalidations,
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

    /// Returns the prepared mutation effects for this unit.
    pub fn invalidations(&self) -> &BTreeMap<BoundExecutionSite, StorageInvalidation> {
        &self.invalidations
    }

    /// Returns the checked mutation effect of one execution site.
    pub fn invalidation_at(&self, site: BoundExecutionSite) -> Option<&StorageInvalidation> {
        self.invalidations.get(&site)
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
        RefinementSetBuildError,
    };
    use crate::{
        AnyBoundNodeId, BoundExpressionId, BoundPatternId, BoundUnitId, BoundUnitKind,
        PatternPredicate, StorageAccessId,
    };

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
            Default::default(),
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
        let foreign_pattern = BoundPatternId::from_slot(BoundUnitId::new(5), 0);

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
                pattern: foreign_pattern,
                predicate: PatternPredicate::NullablePresent,
                access: StorageAccessId::from_slot(unit, 0),
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
                Default::default(),
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
                Default::default(),
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
                Default::default(),
                false,
            ),
            Err(RefinementSetBuildError::DuplicateOccurrence)
        );
    }
    #[test]
    fn published_effects_reject_foreign_path_selectors() {
        let unit = BoundUnitId::new(4);
        let expression = BoundExpressionId::from_slot(unit, 0);

        let path = crate::StoragePath {
            root: crate::StorageIdentityId::from_slot(unit, 0),
            projections: [crate::StorageProjection::Element(
                BoundExpressionId::from_slot(BoundUnitId::new(5), 0),
            )]
            .into(),
        };

        assert_eq!(
            CheckedRefinements::try_new(
                unit,
                BoundUnitKind::CallableBody,
                [],
                [(
                    expression.into(),
                    super::StorageInvalidation::Paths(vec![path])
                )]
                .into(),
                false,
            ),
            Err(RefinementSetBuildError::ForeignUnit)
        );
    }
    #[test]
    fn observation_merges_union_effects_and_intersect_exact_aliases() {
        let unit = BoundUnitId::new(4);
        let access = StorageAccessId::from_slot(unit, 0);
        let first = crate::StoragePath::root(crate::StorageIdentityId::from_slot(unit, 0));
        let second = crate::StoragePath::root(crate::StorageIdentityId::from_slot(unit, 1));
        let alias = crate::StoragePath::root(crate::StorageIdentityId::from_slot(unit, 2));

        let mut left = super::StorageInvalidation::Observed {
            accesses: [(access, vec![first.clone()])].into(),
            snapshots: [
                (first.clone(), first.clone()),
                (alias.clone(), first.clone()),
            ]
            .into(),
            referents: [first.clone()].into(),
        };

        let right = super::StorageInvalidation::Observed {
            accesses: [(access, vec![second.clone()])].into(),
            snapshots: [(first.clone(), first.clone()), (alias, second.clone())].into(),
            referents: [second.clone()].into(),
        };

        left.merge(&right);

        assert_eq!(
            left,
            super::StorageInvalidation::Observed {
                accesses: [(access, vec![first.clone(), second.clone()])].into(),
                snapshots: [(first.clone(), first.clone())].into(),
                referents: [first, second].into(),
            }
        );
    }
}
