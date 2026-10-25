use super::super::storage_invalidation::invalidating_operation_accesses;
use bray_bound_tree::StorageInvalidation;
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

pub(crate) const MAX_REFINEMENT_REFINEMENTS: usize = 1 << 20;
pub(crate) const MAX_REFINEMENT_CELLS: usize = 1 << 24;

#[derive(Default)]
pub(crate) struct RefinementUniverse {
    pub(crate) is_recovered: bool,
    refinements: Vec<Refinement>,
    indexes: BTreeMap<Refinement, usize>,
    pattern_indexes: BTreeMap<(StorageAccessId, PatternPredicate, bool), usize>,
    equivalent_accesses: BTreeMap<StorageAccessId, StorageAccessId>,
    edge_refinements: BTreeMap<AnalysisRefinement, Box<[usize]>>,
    trust_boundaries: BTreeMap<BoundExpressionId, usize>,
    pub(crate) invalidating_accesses:
        BTreeMap<bray_bound_tree::BoundExecutionSite, StorageInvalidation>,
}

impl RefinementUniverse {
    pub(crate) fn invalidation_at(
        &self,
        site: bray_bound_tree::BoundExecutionSite,
    ) -> Option<&StorageInvalidation> {
        self.invalidating_accesses.get(&site)
    }

    pub(crate) fn is_recovered(&self) -> bool {
        self.is_recovered
    }

    pub(crate) fn new<C>(
        graph: &ControlFlowGraph,
        request: CheckerUnitView<'_, C>,
        patterns: &CheckedPatterns,
        expressions: &bray_bound_tree::CheckedExpressionSemantics,
        storage: &StoragePlan,
        memory: &bray_bound_tree::CheckedMemoryOperations,
        copied_types: &BTreeSet<bray_symbols::TypeId>,
    ) -> Result<Self, RefinementUniverseError>
    where
        C: CheckerRequestContext + ?Sized,
    {
        let direct_dependencies = direct_expression_dependencies(storage);

        let invalidating_accesses =
            invalidating_operation_accesses(request, expressions, storage, memory, copied_types);

        let mut universe = Self {
            is_recovered: false,
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
                expressions,
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
            .expect("refinement state counts must fit the host address space");

        let bitset_cells = words
            .checked_mul(retained_states)
            .expect("refinement state counts must fit the host address space");

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
        expressions: &bray_bound_tree::CheckedExpressionSemantics,
        storage: &StoragePlan,
        dependencies: &BTreeMap<BoundExpressionId, BTreeSet<StorageAccessId>>,
    ) -> Result<Vec<Refinement>, RefinementUniverseError> {
        Ok(match refinement {
            AnalysisRefinement::CleanupRequired { .. }
            | AnalysisRefinement::InvocationFailure(_)
            | AnalysisRefinement::InvocationCancellation(_)
            | AnalysisRefinement::PreconditionFailure(_)
            | AnalysisRefinement::InvocationCompletion(_) => Vec::new(),
            AnalysisRefinement::InvocationPresence(invocation, value) => {
                let access = storage
                    .invocation_result(invocation)
                    .expect("presence edge retains its actual nullable result");

                vec![Refinement::new(
                    RefinementKind::Pattern {
                        subject: invocation
                            .expression()
                            .expect("protocol result has an expression owner"),
                        access,
                        predicate: PatternPredicate::NullablePresent,
                        value,
                    },
                    [access],
                )]
            }
            AnalysisRefinement::InvocationBoolean(_, _) => Vec::new(),
            AnalysisRefinement::PropagationOutcome {
                expression,
                succeeded,
            } => {
                let Some(bray_bound_tree::SemanticSelection::Propagation(selection)) =
                    expressions.selections().expression(expression)
                else {
                    unreachable!("checked propagation edge retains its selection");
                };

                let Some(bray_bound_tree::BoundExpression::Structured(operation)) =
                    view.expression(expression)
                else {
                    unreachable!("checked propagation retains its structured expression");
                };

                let subject = *operation
                    .operands()
                    .first()
                    .expect("propagation retains its operand");

                let Some((variant, _)) = selection.success() else {
                    return Ok(vec![Refinement::new(
                        RefinementKind::NullablePresence {
                            expression: subject,
                            is_present: succeeded,
                        },
                        expression_dependencies(view, dependencies, subject),
                    )]);
                };

                let ty = expressions
                    .types()
                    .expression(subject)
                    .expect("propagation operand retains its type")
                    .ty();

                storage
                    .node_plans((subject).into())
                    .filter(|plan| {
                        storage
                            .access(plan.access())
                            .is_some_and(|access| access.reached_type() == ty)
                    })
                    .map(|plan| {
                        Refinement::new(
                            RefinementKind::Pattern {
                                subject,
                                access: plan.access(),
                                predicate: PatternPredicate::ActiveUnionVariant(variant),
                                value: succeeded,
                            },
                            [plan.access()],
                        )
                    })
                    .collect()
            }
            AnalysisRefinement::Condition { expression, value } => {
                condition_refinements(view, patterns, storage, dependencies, expression, value)
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
            .unwrap_or_else(|error| panic!("failed to reserve refinement storage: {error}"));

        if let Some(key) = pattern_key {
            self.pattern_indexes.insert(key, index);
        }

        self.refinements.push(refinement.clone());
        self.indexes.insert(refinement, index);

        Ok(index)
    }

    pub(crate) fn len(&self) -> usize {
        self.refinements.len()
    }

    pub(crate) fn active_refinements<'refinements>(
        &'refinements self,
        set: &'refinements RefinementSet,
    ) -> impl Iterator<Item = &'refinements Refinement> + Clone + 'refinements {
        set.indexes().map(|index| {
            self.refinements
                .get(index)
                .expect("active refinement index belongs to the prepared universe")
        })
    }

    pub(crate) fn edge_facts(
        &self,
        refinement: AnalysisRefinement,
    ) -> impl Iterator<Item = &Refinement> {
        self.edge_refinements
            .get(&refinement)
            .into_iter()
            .flatten()
            .map(|index| &self.refinements[*index])
    }

    pub(crate) fn insert_refinement(
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

    pub(crate) fn invalidate_for_operation(
        &self,
        set: &mut RefinementSet,
        occurrence: impl Into<bray_bound_tree::BoundExecutionSite>,
        storage: &StoragePlan,
        invalidation: Option<&StorageInvalidation>,
        discriminants: &BTreeMap<
            bray_bound_tree::StorageIdentityId,
            BTreeSet<bray_bound_tree::StoragePath>,
        >,
    ) {
        let Some(mutations) =
            invalidation.or_else(|| self.invalidating_accesses.get(&occurrence.into()))
        else {
            return;
        };

        set.retain(|index| {
            self.refinements.get(index).is_none_or(|refinement| {
                let descriptor = match refinement.kind() {
                    RefinementKind::Pattern {
                        access,
                        predicate:
                            PatternPredicate::NullableAbsent
                            | PatternPredicate::NullablePresent
                            | PatternPredicate::ActiveUnionVariant(_),
                        ..
                    } => Some(access),
                    RefinementKind::NullablePresence { expression, .. } => storage
                        .node_plans((expression).into())
                        .find(|plan| plan.purpose() == bray_bound_tree::StorageAccessPurpose::Read)
                        .map(|plan| plan.access()),
                    _ => None,
                };

                if let Some(path) = descriptor.and_then(|access| storage.logical_path(access)) {
                    return !crate::analysis::storage_invalidation::discriminant_invalidated(
                        mutations,
                        path,
                        storage,
                        discriminants,
                    );
                }

                refinement
                    .dependencies()
                    .iter()
                    .all(|dependency| !mutations.invalidates(*dependency, storage))
            })
        });
    }

    pub(crate) fn finish_operation(&self, set: &mut RefinementSet, node: AnyBoundNodeId) {
        let AnyBoundNodeId::Expression(expression) = node else {
            return;
        };

        if let Some(index) = self.trust_boundaries.get(&expression) {
            set.remove(*index);
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum RefinementUniverseError {
    CapacityExceeded(DiagnosticRefinementCapacity),
    Cancelled,
}

pub(crate) fn capacity_error(
    surface: DiagnosticRefinementCapacitySurface,
    actual: usize,
    maximum: usize,
) -> RefinementUniverseError {
    let actual = u64::try_from(actual)
        .expect("refinement demand must fit the diagnostic count representation");

    let maximum = u64::try_from(maximum)
        .expect("refinement limit must fit the diagnostic count representation");

    let capacity = DiagnosticRefinementCapacity::try_new(surface, actual, maximum)
        .unwrap_or_else(|| panic!("refinement capacity rejection requires demand {actual} to exceed limit {maximum} for {surface:?}"));

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
