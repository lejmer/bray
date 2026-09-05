use std::sync::Arc;

use bray_base::shared_slice;

use crate::{
    AnyBoundNodeId, BoundExpressionId, BoundPatternId, BoundUnitId, BoundUnitKind,
    PatternPredicate, StorageAccessId,
};

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
    /// One expression reached its ordinary completion point.
    NormalCompletion(BoundExpressionId),
}

impl RefinementKind {
    const fn is_valid_for(self, unit: BoundUnitId) -> bool {
        match self {
            Self::Condition { expression, .. }
            | Self::NullablePresence { expression, .. }
            | Self::TrustBoundary(expression)
            | Self::NormalCompletion(expression) => expression.unit().raw() == unit.raw(),
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

        let foreign_refinement = Refinement::new(RefinementKind::NormalCompletion(foreign), []);

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
}
