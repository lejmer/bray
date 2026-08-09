use std::sync::Arc;

use bray_base::shared_slice;

use crate::{
    AnyBoundNodeId, BoundExpressionId, BoundPatternId, BoundUnitId, BoundUnitKind,
    PatternPredicate, StorageAccessId,
};

/// One flow-sensitive fact established while checking a bound unit.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RefinementFactKind {
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
    /// A checked structural pattern has matched its subject.
    Pattern {
        /// The matched subject expression.
        subject: BoundExpressionId,
        /// The successful pattern occurrence.
        pattern: BoundPatternId,
        /// The structural fact established by that pattern.
        predicate: PatternPredicate,
    },
    /// The operand is currently inside this explicit trust boundary.
    TrustBoundary(BoundExpressionId),
    /// One expression reached its ordinary completion point.
    NormalCompletion(BoundExpressionId),
}

impl RefinementFactKind {
    const fn is_valid_for(self, unit: BoundUnitId) -> bool {
        match self {
            Self::Condition { expression, .. }
            | Self::NullablePresence { expression, .. }
            | Self::TrustBoundary(expression)
            | Self::NormalCompletion(expression) => expression.unit().raw() == unit.raw(),
            Self::Pattern {
                subject, pattern, ..
            } => subject.unit().raw() == unit.raw() && pattern.unit().raw() == unit.raw(),
        }
    }
}

/// One refinement fact and the evaluated storage accesses it depends on.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RefinementFact {
    kind: RefinementFactKind,
    dependencies: Arc<[StorageAccessId]>,
}

impl RefinementFact {
    /// Creates one refinement fact with canonical storage dependencies.
    pub fn new(
        kind: RefinementFactKind,
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

    /// Returns the fact established by control flow.
    pub const fn kind(&self) -> RefinementFactKind {
        self.kind
    }

    /// Returns the evaluated storage accesses whose values support this fact.
    pub fn dependencies(&self) -> &[StorageAccessId] {
        &self.dependencies
    }
}

/// Refinement facts available immediately before one bound operation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RefinementOccurrence {
    node: AnyBoundNodeId,
    facts: Arc<[RefinementFact]>,
}

impl RefinementOccurrence {
    /// Creates one operation occurrence with facts in canonical order.
    pub fn new(node: AnyBoundNodeId, facts: impl IntoIterator<Item = RefinementFact>) -> Self {
        let mut facts = facts.into_iter().collect::<Vec<_>>();

        facts.sort_unstable();
        facts.dedup();

        Self {
            node,
            facts: shared_slice(facts),
        }
    }

    /// Returns the bound operation occurrence.
    pub const fn node(&self) -> AnyBoundNodeId {
        self.node
    }

    /// Returns facts known immediately before this operation.
    pub fn facts(&self) -> &[RefinementFact] {
        &self.facts
    }
}

/// An invalid durable refinement fact table.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RefinementFactsBuildError {
    /// An occurrence or fact belongs to another bound unit.
    ForeignUnit,
    /// The same bound occurrence was published more than once.
    DuplicateOccurrence,
}

/// Immutable flow-sensitive facts retained for checked operation occurrences.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedRefinementFacts {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    occurrences: Arc<[RefinementOccurrence]>,
    is_recovered: bool,
}

impl CheckedRefinementFacts {
    /// Validates and creates one durable refinement fact table.
    pub fn try_new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        occurrences: impl IntoIterator<Item = RefinementOccurrence>,
        is_recovered: bool,
    ) -> Result<Self, RefinementFactsBuildError> {
        let mut occurrences = occurrences.into_iter().collect::<Vec<_>>();

        if occurrences.iter().any(|occurrence| {
            occurrence.node().unit() != unit
                || occurrence.facts().iter().any(|fact| {
                    !fact.kind().is_valid_for(unit)
                        || fact
                            .dependencies()
                            .iter()
                            .any(|dependency| dependency.unit() != unit)
                })
        }) {
            return Err(RefinementFactsBuildError::ForeignUnit);
        }

        occurrences.sort_unstable_by_key(RefinementOccurrence::node);

        if occurrences
            .windows(2)
            .any(|pair| pair[0].node() == pair[1].node())
        {
            return Err(RefinementFactsBuildError::DuplicateOccurrence);
        }

        Ok(Self {
            unit,
            kind,
            occurrences: occurrences.into(),
            is_recovered,
        })
    }

    /// Returns the exact bound unit described by these facts.
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

    /// Returns facts known immediately before one exact operation.
    pub fn facts_before(&self, node: AnyBoundNodeId) -> &[RefinementFact] {
        self.occurrences
            .binary_search_by_key(&node, RefinementOccurrence::node)
            .ok()
            .map(|index| self.occurrences[index].facts())
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
        CheckedRefinementFacts, RefinementFact, RefinementFactKind, RefinementFactsBuildError,
        RefinementOccurrence,
    };
    use crate::{
        AnyBoundNodeId, BoundExpressionId, BoundPatternId, BoundUnitId, BoundUnitKind,
        PatternPredicate, StorageAccessId,
    };

    #[test]
    fn checked_refinement_facts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CheckedRefinementFacts>();
    }

    #[test]
    fn checked_refinements_canonicalize_facts_and_occurrences() {
        let unit = BoundUnitId::new(4);
        let first = BoundExpressionId::from_slot(unit, 0);
        let second = BoundExpressionId::from_slot(unit, 1);
        let dependency = StorageAccessId::from_slot(unit, 0);

        let fact = RefinementFact::new(
            RefinementFactKind::Condition {
                expression: first,
                value: true,
            },
            [dependency, dependency],
        );

        let result = CheckedRefinementFacts::try_new(
            unit,
            BoundUnitKind::CallableBody,
            [
                RefinementOccurrence::new(second.into(), [fact.clone(), fact.clone()]),
                RefinementOccurrence::new(first.into(), []),
            ],
            false,
        )
        .unwrap_or_else(|error| panic!("valid refinement facts must build: {error:?}"));

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

        assert_eq!(result.facts_before(second.into()), &[fact]);
        assert_eq!(result.facts_before(first.into()), []);
    }

    #[test]
    fn checked_refinements_reject_foreign_facts_and_duplicate_occurrences() {
        let unit = BoundUnitId::new(4);
        let expression = BoundExpressionId::from_slot(unit, 0);
        let foreign = BoundExpressionId::from_slot(BoundUnitId::new(5), 0);
        let foreign_pattern = BoundPatternId::from_slot(BoundUnitId::new(5), 0);

        let foreign_fact = RefinementFact::new(RefinementFactKind::NormalCompletion(foreign), []);

        let foreign_pattern_fact = RefinementFact::new(
            RefinementFactKind::Pattern {
                subject: expression,
                pattern: foreign_pattern,
                predicate: PatternPredicate::NullablePresent,
            },
            [],
        );

        assert_eq!(
            CheckedRefinementFacts::try_new(
                unit,
                BoundUnitKind::CallableBody,
                [RefinementOccurrence::new(expression.into(), [foreign_fact])],
                false,
            ),
            Err(RefinementFactsBuildError::ForeignUnit)
        );

        assert_eq!(
            CheckedRefinementFacts::try_new(
                unit,
                BoundUnitKind::CallableBody,
                [RefinementOccurrence::new(
                    expression.into(),
                    [foreign_pattern_fact]
                )],
                false,
            ),
            Err(RefinementFactsBuildError::ForeignUnit)
        );

        assert_eq!(
            CheckedRefinementFacts::try_new(
                unit,
                BoundUnitKind::CallableBody,
                [
                    RefinementOccurrence::new(expression.into(), []),
                    RefinementOccurrence::new(expression.into(), []),
                ],
                false,
            ),
            Err(RefinementFactsBuildError::DuplicateOccurrence)
        );
    }
}
