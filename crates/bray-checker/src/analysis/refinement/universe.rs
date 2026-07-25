use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundExpressionId, BoundPatternId, CheckedPatternFacts,
    PatternPredicate, RefinementFact, RefinementFactKind, StorageAccessId, StorageAccessPurpose,
    StoragePlan, StorageRelationship,
};

use crate::{CheckerRequestContext, CheckerUnitView};

use super::super::model::{AnalysisRefinement, ControlFlowGraph};
use super::set::FactSet;

const MAX_REFINEMENT_FACTS: usize = 1 << 20;
const MAX_REFINEMENT_CELLS: usize = 1 << 24;

pub(super) struct RefinementUniverse {
    facts: Vec<RefinementFact>,
    indexes: BTreeMap<RefinementFact, usize>,
    edge_facts: BTreeMap<AnalysisRefinement, Box<[usize]>>,
    normal_completion: BTreeMap<BoundExpressionId, usize>,
    trust_boundaries: BTreeMap<BoundExpressionId, usize>,
    invalidating_accesses: BTreeMap<BoundExpressionId, Box<[StorageAccessId]>>,
}

impl RefinementUniverse {
    pub(super) fn new<C>(
        graph: &ControlFlowGraph,
        request: CheckerUnitView<'_, C>,
        patterns: &CheckedPatternFacts,
        storage: &StoragePlan,
    ) -> Result<Self, RefinementUniverseError>
    where
        C: CheckerRequestContext + ?Sized,
    {
        let potential_facts = graph
            .edges()
            .len()
            .checked_add(graph.operations().len())
            .and_then(|count| count.checked_add(patterns.patterns().len()))
            .ok_or(RefinementUniverseError::CapacityExceeded)?;

        if potential_facts > MAX_REFINEMENT_FACTS {
            return Err(RefinementUniverseError::CapacityExceeded);
        }

        let direct_dependencies = direct_expression_dependencies(storage);
        let invalidating_accesses = invalidating_expression_accesses(storage);

        let mut universe = Self {
            facts: Vec::new(),
            indexes: BTreeMap::new(),
            edge_facts: BTreeMap::new(),
            normal_completion: BTreeMap::new(),
            trust_boundaries: BTreeMap::new(),
            invalidating_accesses,
        };

        universe
            .facts
            .try_reserve_exact(potential_facts)
            .map_err(|_| RefinementUniverseError::CapacityExceeded)?;

        for edge in graph.edges() {
            if request.is_cancelled() {
                return Err(RefinementUniverseError::Cancelled);
            }

            let Some(refinement) = edge.refinement() else {
                continue;
            };

            let facts = universe.refinement_facts(
                refinement,
                request.view(),
                patterns,
                &direct_dependencies,
            );

            let indexes = facts
                .into_iter()
                .map(|fact| universe.intern(fact))
                .collect::<Vec<_>>();

            universe.edge_facts.insert(refinement, indexes.into());
        }

        for operation in graph.operations() {
            if request.is_cancelled() {
                return Err(RefinementUniverseError::Cancelled);
            }

            let AnyBoundNodeId::Expression(expression) = operation.kind().node() else {
                continue;
            };

            if expression_completes_normally(request.view(), expression) {
                let fact = RefinementFact::new(
                    RefinementFactKind::NormalCompletion(expression),
                    expression_dependencies(request.view(), &direct_dependencies, expression),
                );

                let index = universe.intern(fact);

                universe.normal_completion.insert(expression, index);
            }
        }

        let words = universe.len().div_ceil(u64::BITS as usize);

        let retained_states = graph
            .blocks()
            .len()
            .checked_add(graph.operations().len())
            .ok_or(RefinementUniverseError::CapacityExceeded)?;

        let bitset_cells = words
            .checked_mul(retained_states)
            .ok_or(RefinementUniverseError::CapacityExceeded)?;

        let published_facts = universe
            .len()
            .checked_mul(graph.operations().len())
            .ok_or(RefinementUniverseError::CapacityExceeded)?;

        if bitset_cells > MAX_REFINEMENT_CELLS || published_facts > MAX_REFINEMENT_CELLS {
            return Err(RefinementUniverseError::CapacityExceeded);
        }

        Ok(universe)
    }

    fn refinement_facts(
        &mut self,
        refinement: AnalysisRefinement,
        view: bray_bound_tree::BoundUnitView<'_>,
        patterns: &CheckedPatternFacts,
        dependencies: &BTreeMap<BoundExpressionId, BTreeSet<StorageAccessId>>,
    ) -> Vec<RefinementFact> {
        match refinement {
            AnalysisRefinement::Condition { expression, value } => {
                vec![RefinementFact::new(
                    RefinementFactKind::Condition { expression, value },
                    expression_dependencies(view, dependencies, expression),
                )]
            }
            AnalysisRefinement::NullablePresence {
                expression,
                is_present,
            } => {
                vec![RefinementFact::new(
                    RefinementFactKind::NullablePresence {
                        expression,
                        is_present,
                    },
                    expression_dependencies(view, dependencies, expression),
                )]
            }
            AnalysisRefinement::PatternSuccess { subject, pattern } => {
                pattern_refinements(view, patterns, dependencies, subject, pattern)
            }
            AnalysisRefinement::TrustBoundary(expression) => {
                let fact = RefinementFact::new(RefinementFactKind::TrustBoundary(expression), []);

                let index = self.intern(fact.clone());

                self.trust_boundaries.insert(expression, index);

                vec![fact]
            }
        }
    }

    fn intern(&mut self, fact: RefinementFact) -> usize {
        if let Some(index) = self.indexes.get(&fact).copied() {
            return index;
        }

        let index = self.facts.len();

        self.facts.push(fact.clone());
        self.indexes.insert(fact, index);

        index
    }

    pub(super) fn len(&self) -> usize {
        self.facts.len()
    }

    pub(super) fn active_facts<'facts>(
        &'facts self,
        set: &'facts FactSet,
    ) -> impl Iterator<Item = RefinementFact> + 'facts {
        set.indexes()
            .filter_map(|index| self.facts.get(index).cloned())
    }

    pub(super) fn insert_refinement(&self, set: &mut FactSet, refinement: AnalysisRefinement) {
        let Some(indexes) = self.edge_facts.get(&refinement) else {
            return;
        };

        for index in indexes {
            self.remove_conflicts(set, *index);
            set.insert(*index);
        }
    }

    fn remove_conflicts(&self, set: &mut FactSet, added: usize) {
        let Some(added) = self.facts.get(added) else {
            return;
        };

        set.retain(|index| {
            self.facts
                .get(index)
                .is_none_or(|fact| !facts_conflict(fact.kind(), added.kind()))
        });
    }

    pub(super) fn invalidate_for_operation(
        &self,
        set: &mut FactSet,
        node: AnyBoundNodeId,
        storage: &StoragePlan,
    ) {
        let AnyBoundNodeId::Expression(expression) = node else {
            return;
        };

        let Some(mutations) = self.invalidating_accesses.get(&expression) else {
            return;
        };

        set.retain(|index| {
            self.facts.get(index).is_none_or(|fact| {
                fact.dependencies().iter().all(|dependency| {
                    mutations.iter().all(|mutation| {
                        storage.relationship(*dependency, *mutation)
                            == StorageRelationship::Disjoint
                    })
                })
            })
        });
    }

    pub(super) fn finish_operation(&self, set: &mut FactSet, node: AnyBoundNodeId) {
        let AnyBoundNodeId::Expression(expression) = node else {
            return;
        };

        if let Some(index) = self.trust_boundaries.get(&expression) {
            set.remove(*index);
        }

        if let Some(index) = self.normal_completion.get(&expression) {
            self.remove_conflicts(set, *index);
            set.insert(*index);
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum RefinementUniverseError {
    CapacityExceeded,
    Cancelled,
}

fn expression_dependencies(
    view: bray_bound_tree::BoundUnitView<'_>,
    direct: &BTreeMap<BoundExpressionId, BTreeSet<StorageAccessId>>,
    expression: BoundExpressionId,
) -> BTreeSet<StorageAccessId> {
    let mut dependencies = BTreeSet::new();
    let mut pending = vec![expression];
    let mut visited = BTreeSet::new();

    while let Some(expression) = pending.pop() {
        if !visited.insert(expression) {
            continue;
        }

        dependencies.extend(direct.get(&expression).into_iter().flatten().copied());

        if let Some(expression) = view.expression(expression) {
            pending.extend(expression.child_expressions());
        }
    }

    dependencies
}

fn pattern_refinements(
    view: bray_bound_tree::BoundUnitView<'_>,
    patterns: &CheckedPatternFacts,
    direct: &BTreeMap<BoundExpressionId, BTreeSet<StorageAccessId>>,
    subject: BoundExpressionId,
    root: BoundPatternId,
) -> Vec<RefinementFact> {
    let dependencies = expression_dependencies(view, direct, subject);
    let mut pending = vec![root];
    let mut facts = Vec::new();

    while let Some(pattern) = pending.pop() {
        let Some(bound) = view.pattern(pattern) else {
            continue;
        };

        pending.extend(bound.children().iter().copied());

        let Some(predicate) = patterns
            .pattern(pattern)
            .and_then(|entry| entry.refinement())
        else {
            continue;
        };

        facts.push(RefinementFact::new(
            RefinementFactKind::Pattern {
                subject,
                pattern,
                predicate,
            },
            dependencies.iter().copied(),
        ));
    }

    facts
}

fn direct_expression_dependencies(
    storage: &StoragePlan,
) -> BTreeMap<BoundExpressionId, BTreeSet<StorageAccessId>> {
    let mut dependencies = BTreeMap::<BoundExpressionId, BTreeSet<StorageAccessId>>::new();

    for plan in storage.access_plans() {
        dependencies
            .entry(plan.expression())
            .or_default()
            .insert(plan.access());
    }

    dependencies
}

fn invalidating_expression_accesses(
    storage: &StoragePlan,
) -> BTreeMap<BoundExpressionId, Box<[StorageAccessId]>> {
    let mut accesses = BTreeMap::<BoundExpressionId, Vec<StorageAccessId>>::new();

    for plan in storage
        .access_plans()
        .iter()
        .filter(|plan| access_invalidates_facts(plan.purpose()))
    {
        accesses
            .entry(plan.expression())
            .or_default()
            .push(plan.access());
    }

    accesses
        .into_iter()
        .map(|(expression, accesses)| (expression, accesses.into_boxed_slice()))
        .collect()
}

fn expression_completes_normally(
    view: bray_bound_tree::BoundUnitView<'_>,
    expression: BoundExpressionId,
) -> bool {
    matches!(
        view.expression(expression),
        Some(BoundExpression::Call(_) | BoundExpression::Await(_))
    )
}

const fn access_invalidates_facts(purpose: StorageAccessPurpose) -> bool {
    matches!(
        purpose,
        StorageAccessPurpose::Write
            | StorageAccessPurpose::Move
            | StorageAccessPurpose::ValueTransfer
            | StorageAccessPurpose::Borrow(bray_symbols::BorrowKind::Mutable)
            | StorageAccessPurpose::Assignment
    )
}

fn facts_conflict(left: RefinementFactKind, right: RefinementFactKind) -> bool {
    match (left, right) {
        (
            RefinementFactKind::Condition {
                expression: left,
                value: left_value,
            },
            RefinementFactKind::Condition {
                expression: right,
                value: right_value,
            },
        ) => left == right && left_value != right_value,
        (
            RefinementFactKind::NullablePresence {
                expression: left,
                is_present: left_present,
            },
            RefinementFactKind::NullablePresence {
                expression: right,
                is_present: right_present,
            },
        ) => left == right && left_present != right_present,
        (
            RefinementFactKind::Pattern {
                subject: left,
                predicate: left_predicate,
                ..
            },
            RefinementFactKind::Pattern {
                subject: right,
                predicate: right_predicate,
                ..
            },
        ) if left == right => predicates_conflict(left_predicate, right_predicate),
        _ => false,
    }
}

const fn predicates_conflict(left: PatternPredicate, right: PatternPredicate) -> bool {
    matches!(
        (left, right),
        (
            PatternPredicate::NullableAbsent,
            PatternPredicate::NullablePresent
        ) | (
            PatternPredicate::NullablePresent,
            PatternPredicate::NullableAbsent
        )
    ) || matches!(
        (left, right),
        (
            PatternPredicate::ActiveUnionVariant(left),
            PatternPredicate::ActiveUnionVariant(right)
        ) if left.symbol_id().raw() != right.symbol_id().raw()
    )
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundErrorExpression, BoundExpression, BoundNodeOrigin, BoundTreeBuilder, BoundUnitId,
        RefinementFact, RefinementFactKind,
    };

    use super::facts_conflict;
    use crate::test_support::{callable_key, error_type};

    #[test]
    fn opposite_condition_facts_conflict() {
        let unit = BoundUnitId::new(4);
        let mut builder = BoundTreeBuilder::new(unit);
        let origin = BoundNodeOrigin::source(callable_key().source());
        let expression = BoundExpression::Error(BoundErrorExpression::new(origin, error_type()));

        let Ok(expression) = builder.push_expression(expression) else {
            panic!("one test expression must fit");
        };

        let positive = RefinementFact::new(
            RefinementFactKind::Condition {
                expression,
                value: true,
            },
            [],
        );

        let negative = RefinementFact::new(
            RefinementFactKind::Condition {
                expression,
                value: false,
            },
            [],
        );

        assert!(facts_conflict(positive.kind(), negative.kind()));
        assert_ne!(positive, negative);
    }
}
