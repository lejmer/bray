use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::{ExecutionCondition, ExecutionPlace};

pub(crate) fn rename_witnesses(
    witnesses: &mut BTreeMap<ExecutionCondition, Arc<ExecutionWitness>>,
    rename: &impl Fn(&ExecutionCondition) -> ExecutionCondition,
    rename_identity: &impl Fn(&ExecutionCondition) -> ExecutionCondition,
) {
    let mut renamed = BTreeMap::<ExecutionCondition, Arc<ExecutionWitness>>::new();

    for (carrier, witness) in witnesses.iter() {
        let carrier = rename_identity(carrier);

        if carrier == ExecutionCondition::Unknown {
            continue;
        }

        let mut conditions = BTreeMap::<_, ExecutionGuarantee>::new();

        for (condition, guarantee) in &witness.conditions {
            let condition = rename(condition);

            if condition == ExecutionCondition::Unknown {
                continue;
            }

            let retained = conditions.entry(condition).or_default();

            retained.witness |= guarantee.witness;

            for (address, owners) in &guarantee.addresses {
                let address = rename(address);

                if address != ExecutionCondition::Unknown {
                    retained
                        .addresses
                        .entry(address)
                        .or_default()
                        .extend(owners.iter().cloned());
                }
            }
        }

        let witness = if conditions == witness.conditions {
            Arc::clone(witness)
        } else {
            Arc::new(ExecutionWitness {
                available: witness.available,
                dependencies: witness.dependencies.clone(),
                conditions,
            })
        };

        if let Some(previous) = renamed.get_mut(&carrier) {
            previous.merge(&witness);
        } else {
            renamed.insert(carrier, witness);
        }
    }

    *witnesses = renamed;
}

/// Physical storage required by access conditions on captured addresses.
pub(crate) type AddressDependencies = BTreeMap<ExecutionCondition, BTreeSet<ExecutionPlace>>;

/// One carried clause distinguishes ownership authority from copyable access facts.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct ExecutionGuarantee {
    pub(crate) witness: bool,
    pub(crate) addresses: AddressDependencies,
}

impl ExecutionGuarantee {
    fn contains(&self, incoming: &Self) -> bool {
        (!incoming.witness || self.witness)
            && incoming.addresses.iter().all(|(address, owners)| {
                self.addresses
                    .get(address)
                    .is_some_and(|retained| owners.is_subset(retained))
            })
    }

    pub(crate) fn extend(&mut self, incoming: &Self) {
        self.witness |= incoming.witness;

        for (address, owners) in &incoming.addresses {
            self.addresses
                .entry(address.clone())
                .or_default()
                .extend(owners.iter().cloned());
        }
    }
}

pub(super) type WitnessConditions = BTreeMap<ExecutionCondition, ExecutionGuarantee>;

/// Trusted evidence retained by one complete value identity.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ExecutionWitness {
    /// This run can use the evidence. Pending child execution retains validity separately.
    pub(crate) available: bool,
    /// A clause and the address permissions that bound its validity.
    pub(crate) conditions: WitnessConditions,
    /// Storage and scoped capabilities required by the complete carrier itself.
    pub(crate) dependencies: BTreeSet<ExecutionPlace>,
}

impl Default for ExecutionWitness {
    fn default() -> Self {
        Self {
            available: true,
            conditions: BTreeMap::new(),
            dependencies: BTreeSet::new(),
        }
    }
}

impl ExecutionWitness {
    pub(crate) fn has_guards(&self) -> bool {
        self.conditions.keys().any(|condition| {
            matches!(
                condition,
                ExecutionCondition::Operation(bray_bound_tree::BoundOperator::LogicalOr, _)
            )
        })
    }

    pub(crate) fn refine(
        self: &mut Arc<Self>,
        trusted: &mut super::ExecutionAssumptions,
        assumptions: &mut super::ExecutionAssumptions,
        cache: &super::ExecutionProofCache,
    ) {
        if !self.has_guards() {
            return;
        }

        let derived = cache.refined_witness(self, trusted, assumptions);

        // Publication owns fact keys while the cached result retains shared immutable operands.
        for (condition, guarantee) in derived.iter() {
            if condition.contains_trusted() {
                condition.clone().assume(true, trusted);

                if !self
                    .conditions
                    .get(condition)
                    .is_some_and(|retained| retained.contains(guarantee))
                {
                    Arc::make_mut(self)
                        .conditions
                        .entry(condition.clone())
                        .or_default()
                        .extend(guarantee);
                }
            } else {
                condition.clone().assume(true, assumptions);
            }
        }
    }

    pub(super) fn derived_conditions(
        &self,
        proof: &super::ExecutionProof<'_>,
        assumptions: &super::ExecutionAssumptions,
        cache: &super::ExecutionProofCache,
    ) -> WitnessConditions {
        let mut evidence = super::ExecutionAssumptions::default();

        for condition in self.conditions.keys().filter(|condition| {
            proof.prove_mixed(condition, &mut {
                ExecutionCondition::WORK_LIMIT
            }) == Some(true)
        }) {
            condition.clone().assume(true, &mut evidence);
        }

        let proof = cache.proof(&evidence, assumptions);

        let mut derived = WitnessConditions::new();

        for (condition, guarantee) in &self.conditions {
            for observation in proof.refined_observations(condition).iter() {
                let retained = derived.entry(observation.clone()).or_default();

                retained.witness |= guarantee.witness;

                for (address, owners) in guarantee
                    .addresses
                    .iter()
                    .filter(|(address, _)| observation.observes(address))
                {
                    retained
                        .addresses
                        .entry(address.clone())
                        .or_default()
                        .extend(owners.iter().cloned());
                }
            }
        }

        derived
    }

    pub(crate) fn owning_conditions<'a>(
        &'a self,
        proof: super::ExecutionProof<'_>,
    ) -> impl Iterator<Item = &'a ExecutionCondition> {
        self.conditions.iter().filter(move |(condition, guarantee)| {
            guarantee.witness && proof.owns_condition(condition)
        }).map(|(condition, _)| condition)
    }

    /// Keep each clause on its complete carrier with only the addresses it observes.
    pub(crate) fn carry(
        self: &mut Arc<Self>,
        condition: &ExecutionCondition,
        owning: bool,
        addresses: &AddressDependencies,
    ) {
        if !self.available {
            Arc::make_mut(self).available = true;
        }

        for clause in condition.conjuncts() {
            let changed = self.conditions.get(clause).is_none_or(|retained| {
                (owning && !retained.witness)
                    || addresses.iter().any(|(address, owners)| {
                        clause.observes_trusted(address)
                            && !retained
                                .addresses
                                .get(address)
                                .is_some_and(|retained| owners.is_subset(retained))
                    })
            });

            if !changed {
                continue;
            }

            let retained = Arc::make_mut(self)
                .conditions
                .entry(clause.clone())
                .or_default();

            retained.witness |= owning;

            for (address, owners) in addresses
                .iter()
                .filter(|(address, _)| clause.observes_trusted(address))
            {
                retained
                    .addresses
                    .entry(address.clone())
                    .or_default()
                    .extend(owners.iter().cloned());
            }
        }
    }

    /// Remove invalid clauses without copying unchanged predecessor evidence.
    pub(crate) fn retain_conditions(
        self: &mut Arc<Self>,
        mut keep: impl FnMut(&ExecutionCondition, &ExecutionGuarantee) -> bool,
    ) {
        let removed = self
            .conditions
            .iter()
            .filter(|(condition, guarantee)| !keep(condition, guarantee))
            .map(|(condition, _)| condition.clone())
            .collect::<Vec<_>>();

        if removed.is_empty() {
            return;
        }

        let witness = Arc::make_mut(self);

        for condition in removed {
            witness.conditions.remove(&condition);
        }
    }

    pub(crate) fn merge(self: &mut Arc<Self>, incoming: &Self) -> bool {
        let dependencies = incoming
            .dependencies
            .difference(&self.dependencies)
            .cloned()
            .collect::<Vec<_>>();

        let conditions = incoming
            .conditions
            .iter()
            .filter(|(condition, guarantee)| {
                !self
                    .conditions
                    .get(*condition)
                    .is_some_and(|retained| retained.contains(guarantee))
            })
            .collect::<Vec<_>>();

        if dependencies.is_empty()
            && conditions.is_empty()
            && (!incoming.available || self.available)
        {
            return false;
        }

        let witness = Arc::make_mut(self);

        witness.available |= incoming.available;
        witness.dependencies.extend(dependencies);

        for (condition, guarantee) in conditions {
            witness
                .conditions
                .entry(condition.clone())
                .or_default()
                .extend(guarantee);
        }

        true
    }
}

impl super::ExecutionCallEvidence {
    pub(crate) fn joined_values(
        &self,
        substitution: &super::ExecutionSubstitution<'_>,
    ) -> Option<Self> {
        if !substitution.has_changes() {
            return None;
        }

        let rename = |value: &ExecutionCondition| substitution.value(value);

        let rename_arguments = |arguments: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>| {
            arguments
                .iter()
                .map(|(place, value)| (place.clone(), rename(value)))
                .collect::<BTreeMap<_, _>>()
        };

        let arguments = rename_arguments(&self.arguments);
        let copy_sources = rename_arguments(&self.copy_sources);

        let witness_inputs = self
            .witness_inputs
            .iter()
            .map(|owner| substitution.owner(owner))
            .collect();

        let extracted_values = self.extracted_values.iter().map(rename).collect();

        let rename_facts = |facts: &BTreeSet<(ExecutionCondition, bool)>| {
            facts
                .iter()
                .flat_map(|(condition, holds)| {
                    let mut renamed = BTreeSet::new();

                    rename(condition).assume(*holds, &mut renamed);

                    renamed
                })
                .collect::<super::ExecutionAssumptions>()
        };

        let assumptions = rename_facts(&self.assumptions);
        let trusted_assumptions = rename_facts(&self.trusted_assumptions);

        let addresses = self
            .addresses
            .iter()
            .map(|(place, addresses)| {
                (
                    place.clone(),
                    addresses
                        .iter()
                        .map(|(address, owners)| (rename(address), owners.clone()))
                        .collect(),
                )
            })
            .collect();

        if arguments == self.arguments
            && copy_sources == self.copy_sources
            && witness_inputs == self.witness_inputs
            && extracted_values == self.extracted_values
            && assumptions == self.assumptions
            && trusted_assumptions == self.trusted_assumptions
            && addresses == self.addresses
        {
            return None;
        }

        // Reuse each mapped field once, without copying the original historical context first.
        Some(Self {
            implementation_witnesses: Arc::clone(&self.implementation_witnesses),
            arguments,
            copy_sources,
            witness_inputs,
            extracted_values,
            assumptions,
            trusted_assumptions,
            addresses,
            owned_inputs: Arc::clone(&self.owned_inputs),
            anonymous_target: self.anonymous_target.clone(),
            trusted_boundary: self.trusted_boundary,
            pending_execution: self.pending_execution,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ExecutionWitness;
    use crate::{ExecutionCondition, ExecutionPlace};
    use bray_symbols::SymbolOrdinal;
    use std::collections::BTreeSet;

    #[test]
    fn refinement_discharges_a_known_outer_guard_without_assuming_the_remaining_guard() {
        use bray_bound_tree::BoundOperator;

        let outer = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
        let inner = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));

        let claim = ExecutionCondition::Trusted(std::sync::Arc::new(ExecutionCondition::Input(
            ExecutionPlace::argument(SymbolOrdinal::new(2)),
        )));

        let guarded = |guard: ExecutionCondition, value: ExecutionCondition| {
            ExecutionCondition::operation(BoundOperator::LogicalOr, vec![
                ExecutionCondition::operation(BoundOperator::LogicalNot, vec![guard]), value,
            ])
        };

        let residual = guarded(inner.clone(), claim.clone());
        let original = guarded(outer.clone(), residual.clone());
        let mut witness = std::sync::Arc::new(ExecutionWitness::default());

        witness.carry(&original, true, &Default::default());

        let unproved = ExecutionCondition::Trusted(std::sync::Arc::new(ExecutionCondition::Input(
            ExecutionPlace::argument(SymbolOrdinal::new(3)),
        )));

        let unproved_residual = guarded(inner.clone(), unproved.clone());

        witness.carry(&guarded(outer.clone(), unproved_residual.clone()), true, &Default::default());

        let mut trusted = [(original, true)].into();
        let mut ordinary = [(outer, true)].into();

        witness.refine(&mut trusted, &mut ordinary, &super::super::ExecutionProofCache::default());

        assert!(witness.conditions.contains_key(&residual));
        assert!(trusted.contains(&(residual.clone(), true)));
        assert!(!witness.conditions.contains_key(&unproved_residual));
        assert_ne!(unproved.prove_trusted(&trusted, &ordinary), Some(true));
        assert_ne!(claim.prove_trusted(&trusted, &ordinary), Some(true));

        ordinary.clear();
        trusted.clear();
        residual.assume(true, &mut trusted);
        inner.assume(true, &mut ordinary);

        assert_eq!(claim.prove_trusted(&trusted, &ordinary), Some(true));
    }

    #[test]
    fn merging_witnesses_retains_all_possible_address_owners_and_reports_new_edges() {
        let carrier_condition = ExecutionCondition::Boolean(true);
        let address = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
        let mut target = std::sync::Arc::new(ExecutionWitness::default());

        for ordinal in [1, 2] {
            let owner = ExecutionPlace::argument(SymbolOrdinal::new(ordinal));
            let mut incoming = ExecutionWitness::default();

            incoming
                .conditions
                .entry(carrier_condition.clone())
                .or_default()
                .addresses
                .insert(address.clone(), BTreeSet::from([owner]));

            assert!(target.merge(&incoming));
            assert!(!target.merge(&incoming));
        }

        assert_eq!(
            target.conditions[&carrier_condition].addresses[&address].len(),
            2
        );
    }
}
