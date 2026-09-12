use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::CallableExecutionObligation;

/// The reason an implementation cannot supply certified execution evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionProofFailure<K> {
    /// A dependency has no available implementation candidate.
    MissingCandidate(K),
    /// Normal termination depends on itself through selected calls or cleanup.
    CircularCompletion(K),
}

/// Validates selected proof dependencies using the shared graph analysis.
/// Pure recursion is allowed. Any cycle containing a total or completion obligation is rejected.
/// Missing candidates invalidate all transitive consumers.
pub fn check_execution_proof_dependencies<K: Copy + Ord, C: Copy + Ord>(
    graph: &BTreeMap<
        (K, CallableExecutionObligation<C>),
        BTreeSet<(K, CallableExecutionObligation<C>)>,
    >,
) -> BTreeMap<(K, CallableExecutionObligation<C>), ExecutionProofFailure<K>> {
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
        if let Some(total) = component.iter().find(|key| key.1.requires_acyclic_proof())
            && (component.len() > 1 || graph.get(total).is_some_and(|deps| deps.contains(total)))
        {
            for key in &component {
                failures.insert(*key, ExecutionProofFailure::CircularCompletion(total.0));
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
    use crate::{CallableExecutionObligation, ExecutionProperty};
    type ExecutionObligation = CallableExecutionObligation<u32>;
    const PURE: ExecutionObligation = ExecutionObligation::Property(ExecutionProperty::Pure, None);
    const TOTAL: ExecutionObligation =
        ExecutionObligation::Property(ExecutionProperty::Total, None);
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn pure_recursion_is_independent_of_total_certification() {
        let graph = BTreeMap::from([
            ((0, PURE), BTreeSet::from([(1, PURE)])),
            ((1, PURE), BTreeSet::from([(0, PURE)])),
            ((0, TOTAL), BTreeSet::from([(1, TOTAL)])),
            ((1, TOTAL), BTreeSet::from([(0, TOTAL)])),
            ((2, TOTAL), BTreeSet::from([(1, TOTAL)])),
        ]);

        assert_eq!(
            check_execution_proof_dependencies(&graph),
            BTreeMap::from([
                ((0, TOTAL), ExecutionProofFailure::CircularCompletion(0)),
                ((1, TOTAL), ExecutionProofFailure::CircularCompletion(0)),
                ((2, TOTAL), ExecutionProofFailure::CircularCompletion(0)),
            ])
        );
    }

    #[test]
    fn missing_evidence_invalidates_transitive_consumers() {
        let graph = BTreeMap::from([
            ((0, PURE), BTreeSet::from([(1, PURE)])),
            ((1, PURE), BTreeSet::from([(2, PURE)])),
        ]);

        assert_eq!(
            check_execution_proof_dependencies(&graph),
            BTreeMap::from([
                ((0, PURE), ExecutionProofFailure::MissingCandidate(2)),
                ((1, PURE), ExecutionProofFailure::MissingCandidate(2)),
                ((2, PURE), ExecutionProofFailure::MissingCandidate(2)),
            ])
        );
    }
}
