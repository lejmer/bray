use std::sync::Arc;

/// One failed condition, retaining its declaration meaning and instantiated arguments.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UnprovenExecutionCondition {
    required: Arc<super::ExecutionCondition>,
    actual: Arc<super::ExecutionCondition>,
}

impl UnprovenExecutionCondition {
    pub(crate) fn new(required: super::ExecutionCondition, actual: super::ExecutionCondition) -> Self {
        Self { required: Arc::new(required), actual: Arc::new(actual) }
    }

    /// Returns the condition before binding its declaration inputs.
    pub fn required(&self) -> &super::ExecutionCondition {
        &self.required
    }

    /// Returns the exact condition that could not be proved for the captured arguments.
    pub fn actual(&self) -> &super::ExecutionCondition {
        &self.actual
    }
}

/// Values captured at a selected call's entry, before callee mutation can occur.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ExecutionCallEvidence {
    pub(crate) implementation_witnesses: std::sync::Arc<[bray_symbols::ImplementationInstanceId]>,
    /// Exact existing carriers admitted at invocation. Their ledger clauses stay
    /// private until completion uses this entry's captured truth and actual return.
    pub(crate) witness_inputs: std::collections::BTreeSet<super::ExecutionCondition>,
    /// Existing complete values selected by checked moves out of borrowed slots.
    /// These retain private proof, without transferring ownership of the borrow.
    pub(crate) extracted_values: std::collections::BTreeSet<super::ExecutionCondition>,
    /// Copy origins while their observations remain unchanged. These are value
    /// correlations, never language equality predicates or ownership authority.
    pub(crate) copy_sources:
        std::collections::BTreeMap<super::ExecutionPlace, super::ExecutionCondition>,
    /// Noncopying owned inputs, in receiver-first checked signature order.
    pub(crate) owned_inputs: std::sync::Arc<[bray_symbols::SymbolOrdinal]>,
    /// Access dependencies captured before consuming inputs, without owning authority.
    pub(crate) addresses:
        std::collections::BTreeMap<super::ExecutionPlace, super::AddressDependencies>,
    pub(crate) anonymous_target: Option<bray_bound_tree::BoundUnitKey>,
    pub(crate) trusted_boundary: bool,
    pub(crate) pending_execution: bool,
    pub(crate) arguments:
        std::collections::BTreeMap<super::ExecutionPlace, super::ExecutionCondition>,
    pub(crate) assumptions: super::ExecutionAssumptions,
    pub(crate) trusted_assumptions: super::ExecutionAssumptions,
}

impl ExecutionCallEvidence {
    /// Actual values moved into the receiver-first noncopying input slots.
    pub(crate) fn owned_arguments(&self) -> impl Iterator<Item = super::ExecutionCondition> + '_ {
        self.owned_inputs.iter().filter_map(|ordinal| {
            super::ExecutionPlace::argument(*ordinal).value_in(&self.arguments)
        })
    }

    /// Values whose existing evidence is admitted independently of result publication.
    pub(crate) fn witness_sources(&self) -> impl Iterator<Item = super::ExecutionCondition> + '_ {
        self.owned_arguments().chain(self.extracted_values.iter().cloned())
    }

    /// Ends admitted proof that no surviving value or result retains.
    pub(crate) fn retiring_witness_values(&self) -> impl Iterator<Item = super::ExecutionCondition> + '_ {
        self.witness_sources().chain(self.witness_inputs.iter().cloned())
    }

    /// Project current ordinary knowledge into immutable copied input identities.
    pub(crate) fn captured_facts(
        &self,
        ordinary: &std::collections::BTreeSet<(super::ExecutionCondition, bool)>,
        index: &super::ConditionIndex,
    ) -> std::collections::BTreeSet<(super::ExecutionCondition, bool)> {
        use super::ExecutionCondition as Condition;

        let bindings = self.copy_bindings();
        let mut facts = std::collections::BTreeSet::new();

        if bindings.is_empty() {
            return facts;
        }

        let snapshots = super::SnapshotSubstitution::new(&bindings, index);

        for (condition, holds) in ordinary {
            if condition.contains_trusted() || condition.observes_live(None) {
                continue;
            }

            let captured = snapshots.apply(condition);

            if captured != *condition {
                captured.assume(*holds, &mut facts);
            }
        }

        for (source, captured) in bindings {
            if let Some(holds) = source.prove(ordinary, &mut { Condition::WORK_LIMIT }) {
                captured.assume(holds, &mut facts);
            }
        }

        facts
    }

    /// Read historical proof through still-valid copy origins. This projection
    /// supplies call-entry proof only. It never publishes copied live authority.
    pub(crate) fn proof_facts<'a>(
        &self,
        facts: &'a std::collections::BTreeSet<(super::ExecutionCondition, bool)>,
    ) -> impl Iterator<Item = (super::ExecutionCondition, bool)> + 'a {
        let bindings = self.copy_bindings();

        facts
            .iter()
            .map(move |(condition, holds)| (condition.with_copy_origins(&bindings), *holds))
    }

    fn copy_bindings(
        &self,
    ) -> std::collections::BTreeMap<super::ExecutionCondition, super::ExecutionCondition> {
        let mut bindings = std::collections::BTreeMap::new();

        for (place, source) in &self.copy_sources {
            let Some(captured) = place.value_in(&self.arguments) else {
                continue;
            };

            bindings.insert(source.clone(), captured.clone());

            bindings.insert(
                source.owner_identity().clone(),
                captured,
            );
        }

        bindings
    }

    /// Captures only constant argument values, with no trusted authority or runtime execution lane.
    pub fn from_constants(
        values: &bray_symbols::SemanticValueStore,
        arguments: &[bray_symbols::ConstantValueId],
    ) -> Self {
        Self {
            arguments: arguments
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    (
                        super::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(
                            u32::try_from(index)
                                .expect("constant call argument index fits its signature"),
                        )),
                        super::ExecutionCondition::from_constant(values, *value),
                    )
                })
                .collect(),
            ..Self::default()
        }
    }

    /// Retains the implementation choices selected for this invocation.
    pub fn with_implementation_witnesses(
        mut self,
        witnesses: impl IntoIterator<Item = bray_symbols::ImplementationInstanceId>,
    ) -> Self {
        self.implementation_witnesses = witnesses.into_iter().collect::<Vec<_>>().into();

        self
    }

    /// Returns the selected invocation's implementation choices.
    pub fn implementation_witnesses(&self) -> &[bray_symbols::ImplementationInstanceId] {
        &self.implementation_witnesses
    }

    /// Instantiates call identities in captured values without changing the reusable entry snapshot.
    pub fn try_map_conditions<E>(
        &self,
        mut map: impl FnMut(&super::ExecutionCondition) -> Result<super::ExecutionCondition, E>,
    ) -> Result<Self, E> {
        let mut mapped = Self {
            implementation_witnesses: std::sync::Arc::clone(&self.implementation_witnesses),
            owned_inputs: std::sync::Arc::clone(&self.owned_inputs),
            anonymous_target: self.anonymous_target.clone(),
            trusted_boundary: self.trusted_boundary,
            pending_execution: self.pending_execution,
            ..Self::default()
        };

        mapped.arguments = self
            .arguments
            .iter()
            .map(|(place, condition)| Ok((place.clone(), map(condition)?)))
            .collect::<Result<_, E>>()?;

        mapped.copy_sources = self
            .copy_sources
            .iter()
            .map(|(place, condition)| Ok((place.clone(), map(condition)?)))
            .collect::<Result<_, E>>()?;

        mapped.witness_inputs = self
            .witness_inputs
            .iter()
            .map(&mut map)
            .collect::<Result<_, E>>()?;

        mapped.extracted_values = self.extracted_values.iter().map(&mut map)
            .collect::<Result<_, E>>()?;

        // Proof queries retain the physical dependencies of each independently mapped address.
        for (place, addresses) in &self.addresses {
            let mapped_addresses = mapped.addresses.entry(place.clone()).or_default();

            for (address, owners) in addresses {
                mapped_addresses
                    .entry(map(address)?)
                    .or_default()
                    .extend(owners.iter().cloned());
            }
        }

        mapped.assumptions = self
            .assumptions
            .iter()
            .map(|(condition, value)| Ok((map(condition)?, *value)))
            .collect::<Result<_, E>>()?;

        mapped.trusted_assumptions = self
            .trusted_assumptions
            .iter()
            .map(|(condition, value)| Ok((map(condition)?, *value)))
            .collect::<Result<_, E>>()?;

        Ok(mapped)
    }

    /// Exact local callable body established by the entry value flow, if known.
    pub fn anonymous_target(&self) -> Option<&bray_bound_tree::BoundUnitKey> {
        self.anonymous_target.as_ref()
    }

    pub(crate) fn intersect(&mut self, other: &Self, cache: &super::ExecutionProofCache) -> bool {
        assert_eq!(
            self.implementation_witnesses, other.implementation_witnesses,
            "a selected invocation retains its implementation choices across control flow"
        );

        assert_eq!(
            self.owned_inputs, other.owned_inputs,
            "one checked invocation retains its input ownership modes"
        );

        let before = (
            self.anonymous_target.is_some(),
            self.trusted_boundary,
            self.pending_execution,
            self.assumptions.len(),
            self.trusted_assumptions.len(),
            self.arguments.len(),
            self.copy_sources.len(),
            self.witness_inputs.len(),
            self.extracted_values.len(),
        );

        let mut addresses_changed = false;

        for (input, addresses) in &other.addresses {
            let retained = self.addresses.entry(input.clone()).or_default();

            for (address, owners) in addresses {
                addresses_changed |= !retained.contains_key(address);

                let retained = retained.entry(address.clone()).or_default();
                let before = retained.len();

                retained.extend(owners.iter().cloned());
                addresses_changed |= retained.len() != before;
            }
        }

        if self.anonymous_target != other.anonymous_target {
            self.anonymous_target = None;
        }

        self.trusted_boundary &= other.trusted_boundary;
        self.pending_execution |= other.pending_execution;

        let mut arguments_changed = false;
        let mut joined_facts = Vec::new();
        let left = cache.ordinary_proof(&self.assumptions);
        let right = cache.ordinary_proof(&other.assumptions);

        for (place, value) in &mut self.arguments {
            let incoming = other.arguments.get(place);

            if incoming == Some(value) { continue; }

            // Invocation entries use the same snapshot join as current values.
            // Loss of contents knowledge does not retire a common carrier or loan.
            let joined = incoming.map_or(super::ExecutionCondition::Unknown, |incoming| {
                super::common_components(
                    value, incoming, None,
                    (&left, &right),
                    &mut joined_facts, &mut { super::ExecutionCondition::WORK_LIMIT },
                )
            });

            if *value != joined {
                *value = joined;
                arguments_changed = true;
            }
        }

        let previous_assumptions = self.assumptions.len();

        self.assumptions.retain(|condition| other.assumptions.contains(condition));

        let mut assumptions_changed = previous_assumptions != self.assumptions.len();

        for fact in joined_facts { assumptions_changed |= self.assumptions.insert(fact); }

        self.trusted_assumptions.retain(|condition| other.trusted_assumptions.contains(condition));

        self.copy_sources
            .retain(|place, value| other.copy_sources.get(place) == Some(value)
                && self.arguments.get(place).is_some_and(|value| *value != super::ExecutionCondition::Unknown));

        self.witness_inputs
            .retain(|owner| other.witness_inputs.contains(owner));

        self.extracted_values.retain(|value| other.extracted_values.contains(value));

        addresses_changed
            || arguments_changed
            || assumptions_changed
            || before
                != (
                    self.anonymous_target.is_some(),
                    self.trusted_boundary,
                    self.pending_execution,
                    self.assumptions.len(),
                    self.trusted_assumptions.len(),
                    self.arguments.len(),
                    self.copy_sources.len(),
                    self.witness_inputs.len(),
                    self.extracted_values.len(),
                )
    }

    /// Proves every entry condition, retaining the first condition and its actual arguments on failure.
    pub fn prove_requirements<'a>(
        &self,
        conditions: impl IntoIterator<Item = &'a super::ExecutionCondition>,
    ) -> Result<(), UnprovenExecutionCondition> {
        self.prove_from(conditions, &self.assumptions, &[])
    }

    /// Proves every trusted obligation from the captured live evidence.
    pub fn prove_trusted_requirements(&self, conditions: &[super::ExecutionCondition]) -> Result<(), UnprovenExecutionCondition> {
        self.prove_from(conditions, &self.trusted_assumptions, &[])
    }

    /// Checks a declaration's own inputs against captured receiver-first arguments.
    pub fn prove_trusted_with_inputs(
        &self,
        conditions: &[super::ExecutionCondition],
        inputs: &[bray_bound_tree::BoundReferenceTarget],
    ) -> Result<(), UnprovenExecutionCondition> {
        self.prove_from(conditions, &self.trusted_assumptions, inputs)
    }

    fn prove_from<'a>(
        &self,
        conditions: impl IntoIterator<Item = &'a super::ExecutionCondition>,
        assumptions: &std::collections::BTreeSet<(super::ExecutionCondition, bool)>,
        inputs: &[bray_bound_tree::BoundReferenceTarget],
    ) -> Result<(), UnprovenExecutionCondition> {
        let mut conditions = conditions.into_iter().peekable();

        if conditions.peek().is_none() {
            return Ok(());
        }

        let equalities = super::ExecutionCondition::equalities(&self.assumptions, None);

        let known = self
            .proof_facts(assumptions)
            .map(|(condition, value)| (condition.with_equalities(&equalities), value))
            .collect();

        let known = super::trusted_condition_closure(known, &self.assumptions);
        let proof = super::ExecutionProof::new(&known, &self.assumptions);

        for required in conditions {
            let condition = required.substitute(
                &|input| {
                    input
                        .value_in(&self.arguments)
                        .or_else(|| {
                            let ordinal = inputs
                                .iter()
                                .position(|reference| Some(*reference) == input.reference())?;

                            let mut argument =
                                super::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(
                                    u32::try_from(ordinal)
                                        .expect("callable parameter ordinal fits"),
                                ));

                            // The captured argument retains this immutable component path.
                            argument.projections = std::sync::Arc::clone(&input.projections);

                            argument.value_in(&self.arguments)
                        })
                        .or_else(|| {
                            input
                                .constant_term()
                                .map(|_| super::ExecutionCondition::Input(input.clone()))
                        })
                        .or_else(|| match input.reference() {
                            Some(bray_bound_tree::BoundReferenceTarget::Surface(
                                bray_symbols::AnySymbolId::GenericConstParameter(_),
                            )) => {
                                // Generic substitution has already mapped this immutable compile-time input into the caller.
                                Some(super::ExecutionCondition::Input(input.clone()))
                            }
                            _ => None,
                        })
                        .unwrap_or(super::ExecutionCondition::Unknown)
                },
                &super::ExecutionCondition::Unknown,
                &mut { crate::ExecutionCondition::WORK_LIMIT },
            );

            let condition = condition.with_equalities(&equalities);

            if proof.prove_mixed(&condition, &mut {
                super::ExecutionCondition::WORK_LIMIT
            }) != Some(true) {
                return Err(UnprovenExecutionCondition::new(required.clone(), condition));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ExecutionCallEvidence;
    use crate::{ExecutionCondition, ExecutionPlace};

    #[test]
    fn borrowed_boolean_requirements_read_current_contents_without_creating_authority() {
        let argument = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0));
        let owner = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(1));
        let field = bray_symbols::ConstantProjectionKind::TupleElement(bray_symbols::SymbolOrdinal::new(0));
        let required = ExecutionCondition::Input(argument.clone().component(field));
        let contents = ExecutionCondition::Input(owner.clone().component(field));
        let mut entry = ExecutionCallEvidence::default();

        entry.arguments.insert(argument.clone(), ExecutionCondition::Input(owner).borrowed());
        entry.assumptions.insert((contents.clone(), true));

        assert!(entry.prove_requirements([&required]).is_ok());
        assert!(entry.prove_requirements([&ExecutionCondition::Trusted(std::sync::Arc::new(required.clone()))]).is_err());

        entry.assumptions.clear();
        assert!(entry.prove_requirements([&required]).is_err());
        entry.assumptions.insert((contents, false));
        assert!(entry.prove_requirements([&required]).is_err());

        entry.arguments.insert(argument, ExecutionCondition::Input(
            ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(2))).borrowed());

        assert!(entry.prove_requirements([&required]).is_err());
    }

    #[test]
    fn failed_entry_proofs_retain_the_requirement_and_actual_value() {
        let input = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0));
        let required = ExecutionCondition::Input(input.clone());

        for actual in [ExecutionCondition::Boolean(false), ExecutionCondition::Unknown] {
            let mut entry = ExecutionCallEvidence::default();

            entry.arguments.insert(input.clone(), actual.clone());

            let failure = entry.prove_requirements([&required]).expect_err("unproven input cannot supply authority");

            assert_eq!(failure.required(), &required);
            assert_eq!(failure.actual(), &actual);
        }

        let mut entry = ExecutionCallEvidence::default();

        entry.arguments.insert(input, ExecutionCondition::Boolean(true));
        assert!(entry.prove_requirements([&required]).is_ok());
    }
}
