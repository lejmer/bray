use std::collections::BTreeMap;
use std::sync::Arc;

use bray_bound_tree::StorageValueDestination as Destination;

use super::{ExecutionCondition as Condition, ExecutionProof, common_components};

/// A result-shape predicate needs both the admitted input values and the actual completion.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum OutputGuard {
    Bound(Condition),
    Completion(Condition),
}

impl OutputGuard {
    pub(crate) fn bind(&self, entry: &super::ExecutionCallEvidence, result: &Condition) -> Condition {
        match self {
            Self::Bound(value) => value.clone(),
            Self::Completion(formal) => entry.bind_condition(formal, result),
        }
    }

    fn bound(&self) -> Option<&Condition> {
        match self {
            Self::Bound(value) => Some(value),
            Self::Completion(_) => None,
        }
    }
}

/// Values bound from one normal exit against the actual call entry.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ExecutionOutput {
    pub(crate) guard: OutputGuard,
    pub(crate) carriers: Arc<[bray_symbols::DependencyWitnessCarrier<bray_bound_tree::StorageProjection>]>,
    pub(crate) ownership: bray_symbols::DependencyOwnedResult,
    pub(crate) writes: BTreeMap<Destination, Condition>,
}

/// Admitted normal alternatives. An unspecified completion has no established writes.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ExecutionOutputs(pub(crate) Arc<[ExecutionOutput]>);

impl ExecutionOutputs {
    pub(crate) fn conditions(&self) -> impl Iterator<Item = &Condition> {
        self.0.iter().flat_map(|row| row.guard.bound().into_iter().chain(row.writes.values()))
    }

    pub(crate) fn try_map<E>(&self, mut map: impl FnMut(&Condition) -> Result<Condition, E>) -> Result<Self, E> {
        Ok(Self(self.0.iter().map(|row| Ok(ExecutionOutput {
            guard: match &row.guard { OutputGuard::Bound(value) => OutputGuard::Bound(map(value)?), formal => formal.clone() },
            carriers: Arc::clone(&row.carriers),
            ownership: row.ownership.clone(),
            writes: row.writes.iter().map(|(key, value)| Ok((key.clone(), map(value)?))).collect::<Result<_, E>>()?,
        })).collect::<Result<_, E>>()?))
    }

    pub(crate) fn can_complete(&self, proof: &ExecutionProof<'_>, entry: &super::ExecutionCallEvidence, result: &Condition) -> bool {
        self.0.is_empty() || self.0.iter().any(|row| proof.can_assume(&row.guard.bind(entry, result), true))
    }

    fn reachable_rows(&self) -> impl Clone + Iterator<Item = &ExecutionOutput> {
        self.0.iter().filter(|row| !matches!(row.guard.bound().map(Condition::kind), Some(super::ExecutionConditionKind::Boolean(false))))
    }

    pub(crate) fn destinations(&self) -> impl Iterator<Item = &Destination> {
        let mut rows = self.reachable_rows();

        rows.next().into_iter().flat_map(|row| row.writes.keys()).filter(|key| !matches!(key, Destination::Region(_)))
            .filter(move |destination| rows.clone().all(|row| row.writes.contains_key(*destination)))
    }

    /// An observation used before exit selection must identify the same value in every alternative.
    pub(crate) fn common_value(&self, destination: &Destination) -> Option<&Condition> {
        let mut rows = self.reachable_rows();
        let first = rows.next()?.writes.get(destination)?;

        rows.all(|row| row.writes.get(destination) == Some(first)).then_some(first)
    }

    pub(crate) fn publish(&mut self, destination: Destination, value: Condition) {
        if self.0.is_empty() {
            self.0 = [ExecutionOutput { guard: OutputGuard::Bound(Condition::boolean(true)), carriers: Arc::default(), ownership: Default::default(), writes: [(destination, value)].into() }].into();
        } else {
            for row in Arc::make_mut(&mut self.0) {
                row.writes.insert(destination.clone(), value.clone());
            }
        }
    }

    pub(crate) fn intersect(&mut self, incoming: &Self, proofs: (&ExecutionProof<'_>, &ExecutionProof<'_>)) -> bool {
        if self == incoming { return false; }

        // A join of distinct admissions retains only writes established on each path.
        // Rows with the same formal guard remain separate until their own completion.
        let mut rows = Vec::new();

        if self.0.len() == incoming.0.len() && self.0.iter().zip(incoming.0.iter()).all(|(a,b)| a.guard == b.guard) {
            for (left, right) in self.0.iter().zip(incoming.0.iter()) {
                let mut writes: BTreeMap<_, _> = left.writes.iter().filter_map(|(key, value)| {
                    let other = right.writes.get(key)?;

                    Some((key.clone(), common_components(value, other, None, proofs, &mut Vec::new(), &mut { Condition::WORK_LIMIT })))
                }).collect();

                for (key, value) in left.writes.iter().chain(&right.writes) {
                    if matches!(key, Destination::Region(_)) {
                        writes.entry(key.clone()).or_insert_with(|| value.clone());
                    }
                }

                let ownership = bray_symbols::DependencyOwnedResult::choice([left.ownership.clone(), right.ownership.clone()]);
                let carriers = left.carriers.iter().filter(|carrier| right.carriers.binary_search(carrier).is_ok()).cloned().collect();

                rows.push(ExecutionOutput { guard: left.guard.clone(), carriers, ownership, writes });
            }
        } else {
            let common = |table: &Self, key: &Destination, proof| {
                let mut values = table.reachable_rows().map(|row| row.writes.get(key));

                values.next()?.cloned().and_then(|first| values.try_fold(first, |value, other|
                    Some(common_components(&value, other?, None, (proof, proof), &mut Vec::new(), &mut { Condition::WORK_LIMIT }))))
            };

            let mut writes: BTreeMap<_, _> = self.destinations().filter_map(|key| {
                let left = common(self, key, proofs.0)?;
                let right = common(incoming, key, proofs.1)?;

                Some((key.clone(), common_components(&left, &right, None, proofs, &mut Vec::new(), &mut { Condition::WORK_LIMIT })))
            }).collect();

            for (row, proof) in self.reachable_rows().map(|row| (row, proofs.0))
                .chain(incoming.reachable_rows().map(|row| (row, proofs.1))) {
                for (key, value) in &row.writes {
                    if matches!(key, Destination::Region(_)) {
                        writes.entry(key.clone()).and_modify(|previous| {
                            *previous = common_components(previous, value, None, (proof, proof), &mut Vec::new(), &mut { Condition::WORK_LIMIT });
                        }).or_insert_with(|| value.clone());
                    }
                }
            }

            let ownership = |table: &Self| bray_symbols::DependencyOwnedResult::choice(table.reachable_rows().map(|row| row.ownership.clone()));
            let mut alternatives = self.reachable_rows().chain(incoming.reachable_rows());

            let carriers = alternatives.next().into_iter().flat_map(|row| row.carriers.iter())
                .filter(|carrier| alternatives.clone().all(|row| row.carriers.binary_search(carrier).is_ok())).cloned().collect();

            rows.push(ExecutionOutput { guard: OutputGuard::Bound(Condition::boolean(true)), carriers,
                ownership: bray_symbols::DependencyOwnedResult::choice([ownership(self), ownership(incoming)]), writes });
        }

        let changed = self.0.as_ref() != rows;

        if changed { self.0 = rows.into(); }

        changed
    }
}
