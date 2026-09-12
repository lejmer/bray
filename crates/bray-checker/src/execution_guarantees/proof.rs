use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::ExecutionProperty;

/// The reason a local candidate cannot supply certified execution evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionProofFailure<K> {
    /// A dependency has no locally checked implementation candidate.
    MissingCandidate(K),
    /// Normal termination depends on itself through selected calls or cleanup.
    CircularTotal(K),
}

/// Validates selected proof dependencies using the shared graph analysis.
/// Pure recursion is allowed. Any cycle containing a total obligation is rejected.
/// Missing candidates invalidate all transitive consumers.
pub fn check_execution_proof_dependencies<K: Copy + Ord>(
    graph: &BTreeMap<(K, ExecutionProperty), BTreeSet<(K, ExecutionProperty)>>,
) -> BTreeMap<(K, ExecutionProperty), ExecutionProofFailure<K>> {
    let mut failures = BTreeMap::new();
    let mut reverse = BTreeMap::<_, BTreeSet<_>>::new();

    for (key, dependencies) in graph {
        for dependency in dependencies {
            reverse.entry(*dependency).or_default().insert(*key);

            if !graph.contains_key(dependency) {
                failures.insert(
                    *dependency,
                    ExecutionProofFailure::MissingCandidate(dependency.0),
                );
            }
        }
    }

    for component in bray_base::strongly_connected_components(graph.keys().copied(), |key| {
        graph.get(&key).into_iter().flatten().copied()
    }) {
        if let Some(total) = component
            .iter()
            .find(|key| key.1 == ExecutionProperty::Total)
            && (component.len() > 1 || graph.get(total).is_some_and(|deps| deps.contains(total)))
        {
            for key in &component {
                failures.insert(*key, ExecutionProofFailure::CircularTotal(total.0));
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
    use super::{ExecutionProofFailure, check_execution_proof_dependencies};
    use crate::execution_guarantees::ExecutionProperty::{Pure, Total};
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn pure_recursion_is_independent_of_total_certification() {
        let graph = BTreeMap::from([
            ((0, Pure), BTreeSet::from([(1, Pure)])),
            ((1, Pure), BTreeSet::from([(0, Pure)])),
            ((0, Total), BTreeSet::from([(1, Total)])),
            ((1, Total), BTreeSet::from([(0, Total)])),
            ((2, Total), BTreeSet::from([(1, Total)])),
        ]);

        assert_eq!(
            check_execution_proof_dependencies(&graph),
            BTreeMap::from([
                ((0, Total), ExecutionProofFailure::CircularTotal(0)),
                ((1, Total), ExecutionProofFailure::CircularTotal(0)),
                ((2, Total), ExecutionProofFailure::CircularTotal(0)),
            ])
        );
    }

    #[test]
    fn missing_evidence_invalidates_transitive_consumers() {
        let graph = BTreeMap::from([
            ((0, Pure), BTreeSet::from([(1, Pure)])),
            ((1, Pure), BTreeSet::from([(2, Pure)])),
        ]);

        assert_eq!(
            check_execution_proof_dependencies(&graph),
            BTreeMap::from([
                ((0, Pure), ExecutionProofFailure::MissingCandidate(2)),
                ((1, Pure), ExecutionProofFailure::MissingCandidate(2)),
                ((2, Pure), ExecutionProofFailure::MissingCandidate(2)),
            ])
        );
    }
}
