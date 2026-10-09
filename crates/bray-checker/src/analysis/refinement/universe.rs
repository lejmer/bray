use super::super::storage_invalidation::{StorageInvalidation, invalidating_operation_accesses};
use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundExpressionId, CheckedPatterns, PatternPredicate, Refinement,
    RefinementKind, StorageAccessId, StoragePlan,
};
use bray_diagnostics::{DiagnosticRefinementCapacity, DiagnosticRefinementCapacitySurface};

use crate::{CheckerRequestContext, CheckerUnitView};

use super::super::model::{AnalysisRefinement, ControlFlowGraph};
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
    trust_boundaries: BTreeMap<BoundExpressionId, usize>,
    invalidating_accesses: BTreeMap<bray_bound_tree::BoundExecutionSite, StorageInvalidation>,
}

impl RefinementUniverse {
    pub(super) fn new<C>(
        graph: &ControlFlowGraph,
        request: CheckerUnitView<'_, C>,
        patterns: &CheckedPatterns,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        storage: &StoragePlan,
        copied_types: &BTreeSet<bray_symbols::TypeId>,
    ) -> Result<Self, RefinementUniverseError>
    where
        C: CheckerRequestContext + ?Sized,
    {
        let direct_dependencies = direct_expression_dependencies(storage);

        let invalidating_accesses =
            invalidating_operation_accesses(request, selections, storage, copied_types);

        let mut universe = Self {
            refinements: Vec::new(),
            indexes: BTreeMap::new(),
            pattern_indexes: BTreeMap::new(),
            equivalent_accesses: equivalent_pattern_accesses(storage),
            edge_refinements: BTreeMap::new(),
            trust_boundaries: BTreeMap::new(),
            invalidating_accesses,
        };

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
            )?;

            let indexes = refinements
                .into_iter()
                .map(|refinement| universe.intern(refinement))
                .collect::<Result<Vec<_>, _>>()?;

            universe.edge_refinements.insert(refinement, indexes.into());
        }

        let words = universe.len().div_ceil(u64::BITS as usize);

        let retained_states = graph
            .blocks()
            .len()
            .checked_add(graph.operations().len())
            .ok_or(RefinementUniverseError::CountUnrepresentable)?;

        let bitset_cells = words
            .checked_mul(retained_states)
            .ok_or(RefinementUniverseError::CountUnrepresentable)?;

        if bitset_cells > MAX_REFINEMENT_CELLS {
            return Err(capacity_error(
                DiagnosticRefinementCapacitySurface::RetainedStateCells,
                bitset_cells,
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
    ) -> Result<Vec<Refinement>, RefinementUniverseError> {
        Ok(match refinement {
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

                let index = self.intern(refinement.clone())?;

                self.trust_boundaries.insert(expression, index);

                vec![refinement]
            }
        })
    }

    fn intern(&mut self, mut refinement: Refinement) -> Result<usize, RefinementUniverseError> {
        let pattern_key = if let RefinementKind::Pattern {
            subject,
            pattern,
            predicate,
            access,
            value,
        } = refinement.kind()
        {
            let access = self
                .equivalent_accesses
                .get(&access)
                .copied()
                .unwrap_or(access);

            let key = (access, predicate, value);

            if let Some(index) = self.pattern_indexes.get(&key) {
                return Ok(*index);
            }

            refinement = Refinement::new(
                RefinementKind::Pattern {
                    subject,
                    pattern,
                    predicate,
                    access,
                    value,
                },
                [access],
            );

            Some(key)
        } else {
            None
        };

        if let Some(index) = self.indexes.get(&refinement).copied() {
            return Ok(index);
        }

        let index = self.refinements.len();

        if index == MAX_REFINEMENT_REFINEMENTS {
            return Err(capacity_error(
                DiagnosticRefinementCapacitySurface::RefinementEntries,
                index + 1,
                MAX_REFINEMENT_REFINEMENTS,
            ));
        }

        self.refinements
            .try_reserve(1)
            .map_err(|_| RefinementUniverseError::AllocationFailed)?;

        if let Some(key) = pattern_key {
            self.pattern_indexes.insert(key, index);
        }

        self.refinements.push(refinement.clone());
        self.indexes.insert(refinement, index);

        Ok(index)
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
        occurrence: impl Into<bray_bound_tree::BoundExecutionSite>,
        storage: &StoragePlan,
    ) {
        let Some(mutations) = self.invalidating_accesses.get(&occurrence.into()) else {
            return;
        };

        set.retain(|index| {
            self.refinements.get(index).is_none_or(|refinement| {
                refinement
                    .dependencies()
                    .iter()
                    .all(|dependency| !mutations.invalidates(*dependency, storage))
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
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum RefinementUniverseError {
    CapacityExceeded(DiagnosticRefinementCapacity),
    CountUnrepresentable,
    AllocationFailed,
    Cancelled,
}

pub(super) fn capacity_error(
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

fn refinements_conflict(left: RefinementKind, right: RefinementKind) -> bool {
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
        (
            RefinementKind::Pattern {
                access: left,
                predicate: left_predicate,
                value: left_value,
                ..
            },
            RefinementKind::Pattern {
                access: right,
                predicate: right_predicate,
                value: right_value,
                ..
            },
        ) if left == right => {
            (left_predicate == right_predicate && left_value != right_value)
                || (left_value
                    && right_value
                    && predicates_conflict(left_predicate, right_predicate))
        }
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
