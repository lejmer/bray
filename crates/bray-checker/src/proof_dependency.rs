use std::collections::{BTreeMap, BTreeSet, VecDeque};

use bray_bound_tree::{CallableProofKey, CallableProofObligation};
use bray_symbols::ExecutionProperty;

/// The leaf reason a body-local contract proof cannot be certified.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallableProofFailure<U = bray_bound_tree::BoundUnitId> {
    /// The selected call has no implementation evidence in the supplied dependency graph.
    UnverifiedCall(bray_bound_tree::AnyBoundNodeId),
    /// The dependency has no checked implementation candidate.
    MissingCandidate(CallableProofKey<U>),
    /// Total execution depends on itself through a reachable chain of proof obligations.
    CircularTotal(CallableProofKey<U>),
}

/// Rejects unavailable implementation evidence and circular termination arguments.
///
/// Every graph key must represent a locally checked candidate. Dependencies may name missing keys,
/// which fail together with their transitive consumers. Pure-only cycles retain their independently
/// checked local purity evidence.
pub fn check_callable_proof_dependencies<U: Copy + Ord>(
    graph: &BTreeMap<CallableProofKey<U>, BTreeSet<CallableProofKey<U>>>,
    unavailable: &BTreeMap<CallableProofKey<U>, CallableProofFailure<U>>,
) -> BTreeMap<CallableProofKey<U>, CallableProofFailure<U>> {
    // Each validation owns its evolving failure map of small, copyable proof identities.
    let mut failures = unavailable.clone();
    let mut reverse = BTreeMap::<_, BTreeSet<_>>::new();

    for (key, dependencies) in graph {
        for dependency in dependencies {
            reverse.entry(*dependency).or_default().insert(*key);

            if !graph.contains_key(dependency) {
                failures.insert(
                    *dependency,
                    CallableProofFailure::MissingCandidate(*dependency),
                );
            }
        }
    }

    for component in bray_base::strongly_connected_components(graph.keys().copied(), |key| {
        graph.get(&key).into_iter().flatten().copied()
    }) {
        let total = component.iter().find(|key| matches!(key.obligation(),
            CallableProofObligation::Execution(guarantee) if guarantee.property() == ExecutionProperty::Total));

        if let Some(total) = total
            && (component.len() > 1
                || graph
                    .get(total)
                    .is_some_and(|dependencies| dependencies.contains(total)))
        {
            for key in &component {
                failures.insert(*key, CallableProofFailure::CircularTotal(*total));
            }
        }
    }

    let mut pending = failures.keys().copied().collect::<VecDeque<_>>();

    while let Some(key) = pending.pop_front() {
        let Some(failure) = failures.get(&key).copied() else {
            continue;
        };

        for consumer in reverse.get(&key).into_iter().flatten() {
            if let std::collections::btree_map::Entry::Vacant(entry) = failures.entry(*consumer) {
                entry.insert(failure);
                pending.push_back(*consumer);
            }
        }
    }

    failures
}

#[cfg(test)]
mod tests {
    use super::{CallableProofFailure, check_callable_proof_dependencies};
    use bray_bound_tree::{BoundUnitId, CallableProofKey, CallableProofObligation};
    use bray_symbols::{CallableExecutionGuarantee, ExecutionProperty, SymbolOrdinal};
    use std::collections::{BTreeMap, BTreeSet};

    fn key(unit: u32, property: ExecutionProperty) -> CallableProofKey {
        CallableProofKey::new(
            BoundUnitId::new(unit),
            CallableProofObligation::Execution(CallableExecutionGuarantee::new(property, None)),
        )
    }

    #[test]
    fn missing_evidence_invalidates_all_transitive_consumers() {
        let first = key(0, ExecutionProperty::Pure);
        let second = key(1, ExecutionProperty::Pure);
        let missing = key(2, ExecutionProperty::Pure);

        let graph = BTreeMap::from([
            (first, BTreeSet::from([second])),
            (second, BTreeSet::from([missing])),
        ]);

        let failures = check_callable_proof_dependencies(&graph, &BTreeMap::new());
        let failure = CallableProofFailure::MissingCandidate(missing);

        assert_eq!(
            failures,
            BTreeMap::from([(first, failure), (second, failure), (missing, failure)])
        );
    }

    #[test]
    fn pure_cycles_are_valid_but_total_cycles_are_not() {
        for property in [ExecutionProperty::Pure, ExecutionProperty::Total] {
            let first = key(0, property);
            let second = key(1, property);
            let consumer = key(2, ExecutionProperty::Pure);

            let graph = BTreeMap::from([
                (first, BTreeSet::from([second])),
                (second, BTreeSet::from([first])),
                (consumer, BTreeSet::from([first])),
            ]);

            let failures = check_callable_proof_dependencies(&graph, &BTreeMap::new());

            assert_eq!(
                failures.len(),
                if property == ExecutionProperty::Pure {
                    0
                } else {
                    3
                }
            );

            assert!(
                failures
                    .values()
                    .all(|failure| matches!(failure, CallableProofFailure::CircularTotal(_)))
            );
        }
    }

    #[test]
    fn mixed_property_cycles_cannot_hide_a_termination_dependency() {
        let pure = key(0, ExecutionProperty::Pure);
        let total = key(0, ExecutionProperty::Total);

        let graph = BTreeMap::from([
            (pure, BTreeSet::from([total])),
            (total, BTreeSet::from([pure])),
        ]);

        assert_eq!(
            check_callable_proof_dependencies(&graph, &BTreeMap::new()),
            BTreeMap::from([
                (pure, CallableProofFailure::CircularTotal(total)),
                (total, CallableProofFailure::CircularTotal(total))
            ])
        );
    }

    #[test]
    fn independent_domains_and_postconditions_preserve_their_exact_dependencies() {
        let base = CallableProofKey::new(
            BoundUnitId::new(0),
            CallableProofObligation::Execution(CallableExecutionGuarantee::new(
                ExecutionProperty::Total,
                Some(SymbolOrdinal::new(0)),
            )),
        );

        let recursive = key(0, ExecutionProperty::Total);

        let postcondition = CallableProofKey::new(
            BoundUnitId::new(1),
            CallableProofObligation::Postcondition(SymbolOrdinal::new(1)),
        );

        let graph = BTreeMap::from([
            (base, BTreeSet::new()),
            (recursive, BTreeSet::from([recursive])),
            (postcondition, BTreeSet::from([base])),
        ]);

        assert_eq!(
            check_callable_proof_dependencies(&graph, &BTreeMap::new()),
            BTreeMap::from([(recursive, CallableProofFailure::CircularTotal(recursive))])
        );
    }
}
