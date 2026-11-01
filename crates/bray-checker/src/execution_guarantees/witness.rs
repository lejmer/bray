use std::collections::{BTreeMap, BTreeSet};

use super::{ExecutionCondition, ExecutionPlace};

/// Physical storage required by access conditions on captured addresses.
pub(crate) type AddressDependencies = BTreeMap<ExecutionCondition, BTreeSet<ExecutionPlace>>;

/// One carried clause distinguishes ownership authority from copyable access facts.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct ExecutionGuarantee {
    pub(crate) witness: bool,
    pub(crate) addresses: AddressDependencies,
}

/// Trusted evidence retained by one complete value identity.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct ExecutionWitness {
    /// A clause and the address permissions that bound its validity.
    pub(crate) conditions: BTreeMap<ExecutionCondition, ExecutionGuarantee>,
    /// Storage and scoped capabilities required by the complete carrier itself.
    pub(crate) dependencies: BTreeSet<ExecutionPlace>,
}

impl ExecutionWitness {
    /// Keep each clause on its complete carrier with only the addresses it observes.
    pub(crate) fn carry(
        &mut self,
        condition: &ExecutionCondition,
        owning: bool,
        addresses: &AddressDependencies,
    ) {
        for clause in condition.conjuncts() {
            let retained = self.conditions.entry(clause.clone()).or_default();

            retained.witness |= owning;

            for (address, owners) in addresses {
                if clause.observes(address) {
                    retained
                        .addresses
                        .entry(address.clone())
                        .or_default()
                        .extend(owners.iter().cloned());
                }
            }
        }
    }

    pub(crate) fn merge(&mut self, incoming: &Self) -> bool {
        let before = self.dependencies.len();
        let mut changed = false;

        self.dependencies
            .extend(incoming.dependencies.iter().cloned());

        changed |= before != self.dependencies.len();

        for (condition, guarantee) in &incoming.conditions {
            changed |= !self.conditions.contains_key(condition);

            let retained = self.conditions.entry(condition.clone()).or_default();

            changed |= guarantee.witness && !retained.witness;
            retained.witness |= guarantee.witness;

            for (address, owners) in &guarantee.addresses {
                changed |= !retained.addresses.contains_key(address);

                let retained = retained.addresses.entry(address.clone()).or_default();
                let before = retained.len();

                retained.extend(owners.iter().cloned());
                changed |= before != retained.len();
            }
        }

        changed
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
        let mut target = ExecutionWitness::default();

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
