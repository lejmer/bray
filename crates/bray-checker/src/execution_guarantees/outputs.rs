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

    fn is_reachable(&self) -> bool {
        !matches!(self, Self::Bound(value) if matches!(value.kind(), super::ExecutionConditionKind::Boolean(false)))
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
        self.0.iter().filter(|row| row.guard.is_reachable())
    }

    /// Shares the admission decision with the matching physical summary row.
    pub(crate) fn is_admitted(&self, index: usize) -> bool {
        self.0.is_empty() || self.0.get(index).expect("admitted outcomes retain the portable row order").guard.is_reachable()
    }

    pub(crate) fn possible_region_values(&self) -> impl Iterator<Item = (&bray_bound_tree::StorageTarget, &Condition)> {
        self.reachable_rows().flat_map(|row| &row.writes).filter_map(|(destination, value)| {
            match destination {
                Destination::Region { target, .. } => Some((target, value)),
                _ => None,
            }
        })
    }

    pub(crate) fn destinations(&self) -> impl Iterator<Item = &Destination> {
        let mut rows = self.reachable_rows();

        rows.next().into_iter().flat_map(|row| row.writes.keys()).filter(|key| !matches!(key, Destination::Region { .. }))
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
                    if matches!(key, Destination::Region { .. }) {
                        writes.entry(key.clone()).or_insert_with(|| value.clone());
                    }
                }

                let ownership = bray_symbols::DependencyOwnedResult::choice([left.ownership.clone(), right.ownership.clone()]);
                let carriers = bray_symbols::DependencyWitnessCarrier::common([&left.carriers, &right.carriers].into_iter());

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
                    if matches!(key, Destination::Region { .. }) {
                        writes.entry(key.clone()).and_modify(|previous| {
                            *previous = common_components(previous, value, None, (proof, proof), &mut Vec::new(), &mut { Condition::WORK_LIMIT });
                        }).or_insert_with(|| value.clone());
                    }
                }
            }

            let ownership = |table: &Self| bray_symbols::DependencyOwnedResult::choice(table.reachable_rows().map(|row| row.ownership.clone()));

            let carriers = if self.0.is_empty() || incoming.0.is_empty() {
                // An opaque completion establishes no witness, even when every known row has one.
                Arc::default()
            } else {
                bray_symbols::DependencyWitnessCarrier::common(
                    self.reachable_rows().chain(incoming.reachable_rows()).map(|row| &row.carriers)
                )
            };

            rows.push(ExecutionOutput { guard: OutputGuard::Bound(Condition::boolean(true)), carriers,
                ownership: bray_symbols::DependencyOwnedResult::choice([ownership(self), ownership(incoming)]), writes });
        }

        let changed = self.0.as_ref() != rows;

        if changed { self.0 = rows.into(); }

        changed
    }
}

#[cfg(test)]
mod tests {
    use super::{Condition, ExecutionOutput, ExecutionOutputs, OutputGuard};
    use bray_symbols::{ConstantTermData, DependencyWitnessCarrier, SymbolOrdinal};

    #[test]
    fn an_opaque_outcome_does_not_carry_another_outcomes_witness() {
        let values = crate::test_support::semantic_values();

        let condition = values.intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .expect("fixture term belongs to its semantic store");

        let row = ExecutionOutput {
            guard: OutputGuard::Bound(Condition::boolean(true)),
            carriers: [DependencyWitnessCarrier::new(condition, [])].into(),
            ownership: Default::default(),
            writes: Default::default(),
        };

        let cache = crate::execution_guarantees::ExecutionProofCache::default();
        let assumptions = Default::default();
        let proof = cache.ordinary_proof(&assumptions);
        let mut outputs = ExecutionOutputs([row].into());

        outputs.intersect(&ExecutionOutputs::default(), (&proof, &proof));

        assert!(outputs.0.iter().all(|row| row.carriers.is_empty()));
    }
}
