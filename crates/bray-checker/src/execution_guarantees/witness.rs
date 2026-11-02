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

/// Trusted evidence retained by one complete value identity.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ExecutionWitness {
    /// This run can use the evidence. Pending child execution retains validity separately.
    pub(crate) available: bool,
    /// A clause and the address permissions that bound its validity.
    pub(crate) conditions: BTreeMap<ExecutionCondition, ExecutionGuarantee>,
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
        trusted: &mut BTreeSet<(ExecutionCondition, bool)>,
        assumptions: &mut BTreeSet<(ExecutionCondition, bool)>,
    ) {
        if !self.has_guards() {
            return;
        }

        let mut evidence = BTreeSet::new();

        for condition in self.conditions.keys().filter(|condition| {
            condition.prove_mixed(trusted, assumptions, &mut {
                ExecutionCondition::WORK_LIMIT
            }) == Some(true)
        }) {
            condition.clone().assume(true, &mut evidence);
        }

        let known = crate::execution_guarantees::trusted_condition_closure(evidence, assumptions);

        let mut derived =
            BTreeMap::<ExecutionCondition, crate::execution_guarantees::ExecutionGuarantee>::new();

        for (condition, guarantee) in &self.conditions {
            for observation in condition
                .observations()
                .filter(|condition| {
                    !condition.contains_trusted()
                        || matches!(condition, ExecutionCondition::Trusted(_))
                })
                .filter(|condition| known.contains(&((*condition).clone(), true)))
            {
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

        for (condition, guarantee) in derived {
            if matches!(condition, ExecutionCondition::Trusted(_)) {
                condition.clone().assume(true, trusted);

                if !self
                    .conditions
                    .get(&condition)
                    .is_some_and(|retained| retained.contains(&guarantee))
                {
                    Arc::make_mut(self)
                        .conditions
                        .entry(condition)
                        .or_default()
                        .extend(&guarantee);
                }
            } else {
                condition.assume(true, assumptions);
            }
        }
    }

    pub(crate) fn is_owning(
        &self,
        trusted: &BTreeSet<(ExecutionCondition, bool)>,
        assumptions: &BTreeSet<(ExecutionCondition, bool)>,
    ) -> bool {
        self.conditions.iter().any(|(condition, guarantee)| {
            guarantee.witness
                && condition.prove_trusted(trusted, assumptions) == Some(true)
                && condition.prove(assumptions, &mut { ExecutionCondition::WORK_LIMIT })
                    != Some(true)
        })
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
    pub(crate) fn join_values(
        &mut self,
        replacements: &BTreeMap<ExecutionCondition, ExecutionCondition>,
        identities: &BTreeMap<ExecutionCondition, ExecutionCondition>,
    ) {
        let rename =
            |value: &ExecutionCondition| value.with_joined_values(replacements, identities);

        for value in self
            .arguments
            .values_mut()
            .chain(self.copy_sources.values_mut())
        {
            *value = rename(value);
        }

        self.witness_inputs = self
            .witness_inputs
            .iter()
            .map(|owner| owner.with_joined_values(identities, identities))
            .collect();

        for facts in [&mut self.assumptions, &mut self.trusted_assumptions] {
            *facts = facts
                .iter()
                .flat_map(|(condition, holds)| {
                    let mut renamed = BTreeSet::new();

                    rename(condition).assume(*holds, &mut renamed);

                    renamed
                })
                .collect();
        }

        for addresses in self.addresses.values_mut() {
            *addresses = addresses
                .iter()
                .map(|(address, owners)| (rename(address), owners.clone()))
                .collect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ExecutionWitness;
    use crate::{ExecutionCondition, ExecutionPlace};
    use bray_symbols::SymbolOrdinal;
    use std::collections::BTreeSet;

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
