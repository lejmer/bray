use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundExpressionId, CheckedPatterns, PatternPredicate,
    Refinement, RefinementKind, StorageAccessId, StoragePlan,
};
use bray_diagnostics::{DiagnosticRefinementCapacity, DiagnosticRefinementCapacitySurface};

use crate::{CheckerRequestContext, CheckerUnitView};

use super::super::model::{AnalysisRefinement, ControlFlowGraph};
use super::super::storage_index::{accesses_are_disjoint, invalidating_operation_accesses};
use super::evidence::{
    condition_refinements, equivalent_pattern_accesses, expression_dependencies,
    pattern_refinements,
};
use super::set::RefinementSet;

pub(super) const MAX_REFINEMENT_REFINEMENTS: usize = 1 << 20;
pub(super) const MAX_REFINEMENT_CELLS: usize = 1 << 24;

pub(super) struct RefinementUniverse {
    refinements: Vec<Refinement>,
    indexes: BTreeMap<Refinement, usize>,
    pattern_indexes: BTreeMap<(StorageAccessId, PatternPredicate, bool), usize>,
    equivalent_accesses: BTreeMap<StorageAccessId, StorageAccessId>,
    edge_refinements: BTreeMap<AnalysisRefinement, Box<[usize]>>,
    normal_completion: BTreeMap<BoundExpressionId, usize>,
    trust_boundaries: BTreeMap<BoundExpressionId, usize>,
    invalidating_accesses: BTreeMap<bray_bound_tree::BoundOperationPoint, Box<[StorageAccessId]>>,
}

impl RefinementUniverse {
    pub(super) fn new<C>(
        graph: &ControlFlowGraph,
        request: CheckerUnitView<'_, C>,
        patterns: &CheckedPatterns,
        storage: &StoragePlan,
    ) -> Result<Self, RefinementUniverseError>
    where
        C: CheckerRequestContext + ?Sized,
    {
        let potential_refinements = graph
            .edges()
            .len()
            .checked_add(graph.operations().len())
            .and_then(|count| count.checked_add(patterns.patterns().len()))
            .ok_or(RefinementUniverseError::CountUnrepresentable)?;

        if potential_refinements > MAX_REFINEMENT_REFINEMENTS {
            return Err(capacity_error(
                DiagnosticRefinementCapacitySurface::RefinementEntries,
                potential_refinements,
                MAX_REFINEMENT_REFINEMENTS,
            ));
        }

        let direct_dependencies = direct_expression_dependencies(storage);
        let invalidating_accesses = invalidating_operation_accesses(storage);

        let mut universe = Self {
            refinements: Vec::new(),
            indexes: BTreeMap::new(),
            pattern_indexes: BTreeMap::new(),
            equivalent_accesses: equivalent_pattern_accesses(storage),
            edge_refinements: BTreeMap::new(),
            normal_completion: BTreeMap::new(),
            trust_boundaries: BTreeMap::new(),
            invalidating_accesses,
        };

        universe
            .refinements
            .try_reserve_exact(potential_refinements)
            .map_err(|_| RefinementUniverseError::AllocationFailed)?;

        for edge in graph.edges() {
            if request.is_cancelled() {
                return Err(RefinementUniverseError::Cancelled);
            }

            let Some(refinement) = edge.refinement() else {
                continue;
            };

            let refinements = universe.refinements(
                refinement,
                request.view(),
                patterns,
                storage,
                &direct_dependencies,
            );

            let indexes = refinements
                .into_iter()
                .map(|refinement| universe.intern(refinement))
                .collect::<Vec<_>>();

            universe.edge_refinements.insert(refinement, indexes.into());
        }

        for operation in graph.operations() {
            if request.is_cancelled() {
                return Err(RefinementUniverseError::Cancelled);
            }

            let AnyBoundNodeId::Expression(expression) = operation.kind().node() else {
                continue;
            };

            if expression_completes_normally(request.view(), expression) {
                let refinement = Refinement::new(
                    RefinementKind::NormalCompletion(expression),
                    expression_dependencies(request.view(), &direct_dependencies, expression),
                );

                let index = universe.intern(refinement);

                universe.normal_completion.insert(expression, index);
            }
        }

        let words = universe.len().div_ceil(u64::BITS as usize);

        if universe.len() > MAX_REFINEMENT_REFINEMENTS {
            return Err(capacity_error(
                DiagnosticRefinementCapacitySurface::RefinementEntries,
                universe.len(),
                MAX_REFINEMENT_REFINEMENTS,
            ));
        }

        let retained_states = graph
            .blocks()
            .len()
            .checked_add(graph.operations().len())
            .ok_or(RefinementUniverseError::CountUnrepresentable)?;

        let bitset_cells = words
            .checked_mul(retained_states)
            .ok_or(RefinementUniverseError::CountUnrepresentable)?;

        let published_refinements = universe
            .len()
            .checked_mul(graph.operations().len())
            .ok_or(RefinementUniverseError::CountUnrepresentable)?;

        if bitset_cells > MAX_REFINEMENT_CELLS {
            return Err(capacity_error(
                DiagnosticRefinementCapacitySurface::RetainedStateCells,
                bitset_cells,
                MAX_REFINEMENT_CELLS,
            ));
        }

        if published_refinements > MAX_REFINEMENT_CELLS {
            return Err(capacity_error(
                DiagnosticRefinementCapacitySurface::PublishedRefinements,
                published_refinements,
                MAX_REFINEMENT_CELLS,
            ));
        }

        Ok(universe)
    }

    fn refinements(
        &mut self,
        refinement: AnalysisRefinement,
        view: bray_bound_tree::BoundUnitView<'_>,
        patterns: &CheckedPatterns,
        storage: &StoragePlan,
        dependencies: &BTreeMap<BoundExpressionId, BTreeSet<StorageAccessId>>,
    ) -> Vec<Refinement> {
        match refinement {
            AnalysisRefinement::CleanupFailure { .. }
            | AnalysisRefinement::CallFailure(_)
            | AnalysisRefinement::MatchExhaustion(_) => Vec::new(),
            AnalysisRefinement::UnionVariant {
                expression,
                variant,
            } => storage
                .expression_plans(expression)
                .filter(|plan| plan.purpose() == bray_bound_tree::StorageAccessPurpose::Read)
                .map(|plan| {
                    Refinement::new(
                        RefinementKind::UnionVariant {
                            subject: expression,
                            access: plan.access(),
                            variant,
                        },
                        [plan.access()],
                    )
                })
                .collect(),
            AnalysisRefinement::Condition { expression, value } => {
                condition_refinements(view, patterns, storage, dependencies, expression, value)
            }
            AnalysisRefinement::NullablePresence {
                expression,
                is_present,
            } => {
                vec![Refinement::new(
                    RefinementKind::NullablePresence {
                        expression,
                        is_present,
                    },
                    expression_dependencies(view, dependencies, expression),
                )]
            }
            AnalysisRefinement::PatternOutcome {
                subject,
                pattern,
                value,
            } => pattern_refinements(view, patterns, storage, subject, pattern, value),
            AnalysisRefinement::TrustBoundary(expression) => {
                let refinement = Refinement::new(RefinementKind::TrustBoundary(expression), []);

                let index = self.intern(refinement.clone());

                self.trust_boundaries.insert(expression, index);

                vec![refinement]
            }
        }
    }

    fn intern(&mut self, mut refinement: Refinement) -> usize {
        let pattern_key =
            if let Some((access, predicate, value)) = refinement.kind().structural_predicate() {
                let access = self
                    .equivalent_accesses
                    .get(&access)
                    .copied()
                    .unwrap_or(access);

                let key = (access, predicate, value);

                if let Some(index) = self.pattern_indexes.get(&key) {
                    return *index;
                }

                let mut kind = refinement.kind();

                match &mut kind {
                    RefinementKind::Pattern { access: target, .. }
                    | RefinementKind::UnionVariant { access: target, .. } => *target = access,
                    _ => unreachable!("structural predicates carry a storage access"),
                }

                refinement = Refinement::new(kind, [access]);

                Some(key)
            } else {
                None
            };

        if let Some(index) = self.indexes.get(&refinement).copied() {
            return index;
        }

        let index = self.refinements.len();

        if let Some(key) = pattern_key {
            self.pattern_indexes.insert(key, index);
        }

        self.refinements.push(refinement.clone());
        self.indexes.insert(refinement, index);

        index
    }

    pub(super) fn len(&self) -> usize {
        self.refinements.len()
    }

    pub(super) fn active_refinements<'refinements>(
        &'refinements self,
        set: &'refinements RefinementSet,
    ) -> impl Iterator<Item = Refinement> + 'refinements {
        set.indexes()
            .filter_map(|index| self.refinements.get(index).cloned())
    }

    pub(super) fn insert_refinement(
        &self,
        set: &mut RefinementSet,
        refinement: AnalysisRefinement,
    ) {
        let Some(indexes) = self.edge_refinements.get(&refinement) else {
            return;
        };

        for index in indexes {
            self.remove_conflicts(set, *index);
            set.insert(*index);
        }
    }

    fn remove_conflicts(&self, set: &mut RefinementSet, added: usize) {
        let Some(added) = self.refinements.get(added) else {
            return;
        };

        set.retain(|index| {
            self.refinements
                .get(index)
                .is_none_or(|refinement| !refinements_conflict(refinement.kind(), added.kind()))
        });
    }

    pub(super) fn invalidate_for_operation(
        &self,
        set: &mut RefinementSet,
        point: bray_bound_tree::BoundOperationPoint,
        storage: &StoragePlan,
    ) {
        let Some(mutations) = self.invalidating_accesses.get(&point) else {
            return;
        };

        set.retain(|index| {
            self.refinements.get(index).is_none_or(|refinement| {
                accesses_are_disjoint(
                    storage,
                    refinement.dependencies().iter().copied(),
                    mutations,
                )
            })
        });
    }

    pub(super) fn finish_operation(&self, set: &mut RefinementSet, node: AnyBoundNodeId) {
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
    CapacityExceeded(DiagnosticRefinementCapacity),
    CountUnrepresentable,
    AllocationFailed,
    Cancelled,
}

fn capacity_error(
    surface: DiagnosticRefinementCapacitySurface,
    actual: usize,
    maximum: usize,
) -> RefinementUniverseError {
    let Ok(actual) = u64::try_from(actual) else {
        return RefinementUniverseError::CountUnrepresentable;
    };

    let Ok(maximum) = u64::try_from(maximum) else {
        return RefinementUniverseError::CountUnrepresentable;
    };

    let Some(capacity) = DiagnosticRefinementCapacity::try_new(surface, actual, maximum) else {
        return RefinementUniverseError::CountUnrepresentable;
    };

    RefinementUniverseError::CapacityExceeded(capacity)
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

fn expression_completes_normally(
    view: bray_bound_tree::BoundUnitView<'_>,
    expression: BoundExpressionId,
) -> bool {
    matches!(
        view.expression(expression),
        Some(BoundExpression::Call(_) | BoundExpression::Await(_))
    )
}

fn refinements_conflict(left: RefinementKind, right: RefinementKind) -> bool {
    if let (Some((left, left_predicate, left_value)), Some((right, right_predicate, right_value))) =
        (left.structural_predicate(), right.structural_predicate())
    {
        return left == right
            && ((left_predicate == right_predicate && left_value != right_value)
                || (left_value
                    && right_value
                    && predicates_conflict(left_predicate, right_predicate)));
    }

    match (left, right) {
        (
            RefinementKind::Condition {
                expression: left,
                value: left_value,
            },
            RefinementKind::Condition {
                expression: right,
                value: right_value,
            },
        ) => left == right && left_value != right_value,
        (
            RefinementKind::NullablePresence {
                expression: left,
                is_present: left_present,
            },
            RefinementKind::NullablePresence {
                expression: right,
                is_present: right_present,
            },
        ) => left == right && left_present != right_present,
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
        Refinement, RefinementKind,
    };

    use super::refinements_conflict;
    use crate::test_support::{callable_key, error_type};

    #[test]
    fn opposite_condition_refinements_conflict() {
        let unit = BoundUnitId::new(4);
        let mut builder = BoundTreeBuilder::new(unit);
        let origin = BoundNodeOrigin::source(callable_key().source());
        let expression = BoundExpression::Error(BoundErrorExpression::new(origin, error_type()));

        let Ok(expression) = builder.push_expression(expression) else {
            panic!("one test expression must fit");
        };

        let positive = Refinement::new(
            RefinementKind::Condition {
                expression,
                value: true,
            },
            [],
        );

        let negative = Refinement::new(
            RefinementKind::Condition {
                expression,
                value: false,
            },
            [],
        );

        assert!(refinements_conflict(positive.kind(), negative.kind()));
        assert_ne!(positive, negative);
    }
}
