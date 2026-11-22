use std::sync::Arc;

use bray_symbols::DependencyFailureKind;

use super::ExecutionCondition as Condition;

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

/// One portable failure row bound to the invocation that can actually produce it.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ExecutionFailureRow {
    pub(crate) index: usize,
    kind: Option<DependencyFailureKind>,
    pub(crate) guard: Option<Condition>,
}

/// The memory input obligation established before the operation commits its effects.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum MemoryAccessProof {
    Initialized,
    ProtectedCellInitialized,
    EmptyCopy,
}

impl ExecutionFailureRow {
    pub(crate) fn covers(&self, kind: DependencyFailureKind) -> bool {
        self.kind.is_none_or(|selected| selected == kind)
    }
}

/// Values captured at a selected call's entry, before callee mutation can occur.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExecutionCallEvidence {
    /// Actual normal publications formed from this admitted environment.
    pub(crate) outputs: super::ExecutionOutputs,
    /// Portable failure rows admitted against the same evaluated entry arguments.
    pub(crate) failure_rows: Option<Arc<[ExecutionFailureRow]>>,
    pub(crate) implementation_witnesses: Arc<[bray_symbols::ImplementationInstanceId]>,
    /// Existing witnesses with their support at this value's admission.
    pub(crate) witness_inputs: std::collections::BTreeMap<Condition, Arc<super::ExecutionWitness>>,
    /// Allocations supported by those same admitted carriers.
    pub(crate) witness_allocations: std::collections::BTreeMap<Condition, Condition>,
    /// Existing complete values selected by checked moves out of borrowed slots.
    /// The transfer's input term identifies the original admission at completion.
    pub(crate) extracted_values: std::collections::BTreeMap<bray_symbols::ConstantTermId, Condition>,
    /// Noncopying owned inputs, in receiver-first checked signature order.
    pub(crate) owned_inputs: Arc<[bray_symbols::SymbolOrdinal]>,
    /// Reached storage lifetimes retained by the evaluated loan at each ordinal.
    pub(crate) borrow_owners: std::collections::BTreeMap<bray_symbols::SymbolOrdinal, Arc<[super::ExecutionPlace]>>,
    /// Access dependencies captured before consuming inputs, without owning authority.
    pub(crate) addresses:
        std::collections::BTreeMap<super::ExecutionPlace, super::AddressDependencies>,
    pub(crate) anonymous_target: Option<bray_bound_tree::BoundUnitKey>,
    pub(crate) trusted_boundary: bool,
    pub(crate) pending_execution: bool,
    /// The reached normal transition left storage observations unchanged.
    pub(crate) storage_unchanged: bool,
    pub(crate) memory_access: Option<MemoryAccessProof>,
    pub(crate) arguments:
        std::collections::BTreeMap<super::ExecutionPlace, super::ExecutionCondition>,
    pub(crate) assumptions: super::ExecutionAssumptions,
    pub(crate) trusted_assumptions: super::ExecutionAssumptions,
}

impl ExecutionCallEvidence {
    /// Whether reusing an evaluation can change this reached admission.
    pub(crate) fn observes_retiring_evaluation(&self, stale: &impl Fn(&Condition) -> bool) -> bool {
        (self.pending_execution && self.assumptions.iter().chain(&self.trusted_assumptions).any(|(condition, _)| stale(condition)))
            || self.witness_support_revoked(&|owner, _| stale(owner), &|condition, support, _| {
                stale(condition) || support.addresses.keys().any(stale)
            })
            || self.arguments.values().chain(self.outputs.conditions()).chain(self.extracted_values.values()).any(stale)
    }

    /// Expires observations of a reused name within this one admission.
    pub(crate) fn retire_evaluation(
        &mut self,
        mutation: bray_bound_tree::StorageMutation,
        stale: &impl Fn(&Condition) -> bool,
    ) {
        self.revoke_witness_support(&|owner, _| stale(owner), &|condition, support, _| {
            stale(condition) || support.addresses.keys().any(stale)
        });

        if self.pending_execution {
            self.assumptions.retain(|(condition, _)| !stale(condition));
            self.trusted_assumptions.retain(|(condition, _)| !stale(condition));
        }

        for value in self.arguments.values_mut().filter(|value| stale(value)) {
            *value = value.without_evaluation(mutation);
        }

        if self.outputs.conditions().any(stale) {
            self.outputs = self.outputs.try_map(|value| Ok::<_, std::convert::Infallible>(value.without_evaluation(mutation)))
                .expect("evaluation retirement is total");
        }

        self.extracted_values.retain(|_, value| !stale(value));
    }

    pub(crate) fn witness_support_revoked(
        &self,
        owner_revoked: &impl Fn(&Condition, &super::ExecutionWitness) -> bool,
        clause_revoked: &impl Fn(&Condition, &super::ExecutionGuarantee, *const super::ExecutionWitness) -> bool,
    ) -> bool {
        self.witness_inputs.iter().any(|(owner, witness)| {
            owner_revoked(owner, witness)
                || witness.conditions.iter().any(|(condition, support)| clause_revoked(condition, support, Arc::as_ptr(witness)))
        }) || self.witness_allocations.iter().any(|(condition, owner)| {
            self.witness_inputs.get(owner).is_none_or(|witness| clause_revoked(condition, &Default::default(), Arc::as_ptr(witness)))
        })
    }

    pub(crate) fn revoke_witness_support(
        &mut self,
        owner_revoked: &impl Fn(&Condition, &super::ExecutionWitness) -> bool,
        clause_revoked: &impl Fn(&Condition, &super::ExecutionGuarantee, *const super::ExecutionWitness) -> bool,
    ) {
        self.witness_allocations.retain(|condition, owner| {
            self.witness_inputs.get(owner).is_some_and(|witness| !clause_revoked(condition, &Default::default(), Arc::as_ptr(witness)))
        });

        self.witness_inputs.retain(|owner, witness| {
            if owner_revoked(owner, witness) { return false; }

            let identity = Arc::as_ptr(witness);

            witness.retain_conditions(|condition, support| !clause_revoked(condition, support, identity));

            !witness.conditions.is_empty() || !witness.dependencies.is_empty()
        });

        self.witness_allocations.retain(|_, owner| self.witness_inputs.contains_key(owner));
    }

    pub(crate) fn select_failure_rows<Report, Write>(
        &self,
        values: &bray_symbols::SemanticValueStore,
        cache: &super::ExecutionProofCache,
        rows: Option<&[bray_symbols::CapturedFailureOutcome<bray_symbols::ConstantTermId, Report, bray_symbols::DependencyStorageEffect, Write>]>,
    ) -> Option<Arc<[ExecutionFailureRow]>> {
        let proof = cache.ordinary_proof(&self.assumptions);

        rows.map(|rows| rows.iter().enumerate().filter_map(|(index, outcome)| {
            let guard = outcome.guard.map(|guard| self.bind_term(values, guard, &Condition::unknown()));

            guard.as_ref().is_none_or(|condition| proof.can_assume(condition, true))
                .then_some(ExecutionFailureRow { index, kind: outcome.kind, guard })
        }).collect())
    }

    pub(crate) fn admits_failure(&self, row: usize) -> bool {
        Self::admits_failure_row(self.failure_rows.as_deref(), row)
    }

    pub(crate) fn admits_failure_row(rows: Option<&[ExecutionFailureRow]>, row: usize) -> bool {
        rows.is_none_or(|rows| rows.binary_search_by_key(&row, |candidate| candidate.index).is_ok())
    }

    pub(crate) fn failure_guard(&self, row: usize) -> Option<&Condition> {
        let rows = self.failure_rows.as_deref()?;
        let index = rows.binary_search_by_key(&row, |candidate| candidate.index).ok()?;

        rows[index].guard.as_ref()
    }

    pub(crate) fn try_map_failure_guards<E>(&self, mut map: impl FnMut(&Condition) -> Result<Condition, E>)
        -> Result<Option<Arc<[ExecutionFailureRow]>>, E> {
        let Some(rows) = &self.failure_rows else { return Ok(None); };

        let mut mapped = None;

        for (index, row) in rows.iter().enumerate() {
            let Some(guard) = &row.guard else { continue; };

            let updated = map(guard)?;

            if updated != *guard {
                mapped.get_or_insert_with(|| rows.to_vec())[index].guard = Some(updated);
            }
        }

        Ok(Some(mapped.map_or_else(|| Arc::clone(rows), Arc::from)))
    }

    pub(crate) fn bind_condition(&self, formal: &Condition, result: &Condition) -> Condition {
        formal.substitute(&|place| place.value_in(&self.arguments).unwrap_or_else(Condition::unknown),
            result, &mut { Condition::WORK_LIMIT })
    }

    /// Instantiates an entry condition using the admitted values and their equalities.
    pub(crate) fn bind_entry_condition(
        &self,
        formal: &Condition,
        equalities: &std::collections::BTreeMap<Condition, Condition>,
    ) -> Condition {
        self.bind_condition(formal, &Condition::unknown()).with_equalities(equalities)
    }

    pub(crate) fn formal_condition(&self, values: &bray_symbols::SemanticValueStore, term: bray_symbols::ConstantTermId) -> Condition {
        crate::execution_condition_from_type_term(values, term, self.positional_arguments().count())
            .expect("checked condition decodes in the admitted call environment")
    }

    pub(crate) fn bind_term(&self, values: &bray_symbols::SemanticValueStore, term: bray_symbols::ConstantTermId, result: &Condition) -> Condition {
        self.bind_condition(&self.formal_condition(values, term), result)
    }

    /// Resolves entry observations before binding current arguments or the result.
    pub(crate) fn capture_condition(&self, formal: &Condition) -> Condition {
        formal.capture_entry(
            &|place| place.value_in(&self.arguments).unwrap_or_else(Condition::unknown),
            &self.trusted_assumptions,
            &self.assumptions,
            &mut { Condition::WORK_LIMIT },
        )
    }

    pub(crate) fn select_output(&mut self, row: usize) {
        self.outputs = super::ExecutionOutputs([self.outputs.0[row].clone()].into());
    }

    pub(crate) fn conditions(&self) -> impl Iterator<Item = &Condition> {
        self.arguments.values().chain(self.outputs.conditions())
            .chain(self.failure_rows.iter().flat_map(|rows| rows.iter().filter_map(|row| row.guard.as_ref())))
            .chain(self.witness_inputs.keys())
            .chain(self.witness_inputs.values().flat_map(|witness| witness.conditions.iter()
                .flat_map(|(condition, guarantee)| std::iter::once(condition).chain(guarantee.addresses.keys()))))
            .chain(self.witness_allocations.keys())
            .chain(self.extracted_values.values())
            .chain(self.addresses.values().flat_map(|addresses| addresses.keys()))
            .chain(self.assumptions.iter().map(|(condition, _)| condition))
            .chain(self.trusted_assumptions.iter().map(|(condition, _)| condition))
    }

    /// Evaluated arguments in receiver-first formal order, excluding declaration aliases.
    pub(crate) fn positional_arguments(&self) -> impl Iterator<Item = (bray_symbols::SymbolOrdinal, &Condition)> {
        self.arguments.iter().filter_map(|(place, value)| match place.root {
            super::ExecutionInput::Argument(ordinal) if place.projections.is_empty() => Some((ordinal, value)),
            _ => None,
        })
    }

    /// The inline physical snapshot of the evaluated operand at this ordinal.
    pub(crate) fn argument_storage(&self, ordinal: bray_symbols::SymbolOrdinal) -> Option<&bray_bound_tree::StorageCapturedValue> {
        self.arguments.get(&super::ExecutionPlace::argument(ordinal))?.inline_captured_storage()
    }

    /// Complete escape bounds from the evaluated operands, including their actual descriptors.
    pub(crate) fn argument_regions(&self, count: usize) -> Option<Arc<[bray_bound_tree::StorageTarget]>> {
        let mut arguments = self.positional_arguments();
        let mut regions = Vec::new();

        for index in 0..count {
            let (ordinal, argument) = arguments.next()?;

            if ordinal.to_index() != Some(index) { return None; }

            regions.extend(argument.captured_input_regions()?);
        }

        arguments.next().is_none().then(|| bray_base::sorted_unique_shared_slice(regions))
    }

    /// Actual values moved into the receiver-first noncopying input slots.
    pub(crate) fn owned_arguments(&self) -> impl Iterator<Item = super::ExecutionCondition> + '_ {
        self.owned_inputs.iter().filter_map(|ordinal| {
            super::ExecutionPlace::argument(*ordinal).value_in(&self.arguments)
        })
    }

    /// Values whose existing evidence is admitted independently of result publication.
    pub(crate) fn witness_sources(&self) -> impl Iterator<Item = super::ExecutionCondition> + '_ {
        self.owned_arguments().chain(self.extracted_values.values().cloned())
    }

    /// Ends admitted proof that no surviving value or result retains.
    pub(crate) fn retiring_witness_values(&self) -> impl Iterator<Item = super::ExecutionCondition> + '_ {
        self.witness_sources().chain(self.witness_inputs.keys().cloned())
    }

    /// Reads the logical facts of the admitted invocation without physical capture metadata.
    pub(crate) fn proof_facts(
        facts: &std::collections::BTreeSet<(super::ExecutionCondition, bool)>,
    ) -> impl Iterator<Item = (super::ExecutionCondition, bool)> + '_ {
        facts.iter().map(|(condition, holds)| (condition.without_captured_storage(), *holds))
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
    pub(crate) fn try_map_conditions<E>(
        &self,
        mut map: impl FnMut(&super::ExecutionCondition) -> Result<super::ExecutionCondition, E>,
    ) -> Result<Self, E> {
        let mut mapped = Self {
            failure_rows: self.try_map_failure_guards(&mut map)?,
            implementation_witnesses: Arc::clone(&self.implementation_witnesses),
            owned_inputs: Arc::clone(&self.owned_inputs),
            borrow_owners: self.borrow_owners.clone(),
            anonymous_target: self.anonymous_target.clone(),
            trusted_boundary: self.trusted_boundary,
            pending_execution: self.pending_execution,
            storage_unchanged: self.storage_unchanged,
            memory_access: self.memory_access,
            ..Self::default()
        };

        mapped.arguments = super::call_proof::map_arguments(&self.arguments, &mut map)?;

        mapped.outputs = self.outputs.try_map(&mut map)?;

        mapped.witness_inputs = self
            .witness_inputs
            .iter()
            .map(|(owner, witness)| Ok((map(owner)?, witness.try_map_conditions(&mut map)?)))
            .collect::<Result<_, E>>()?;

        mapped.witness_allocations = self.witness_allocations.iter()
            .map(|(condition, owner)| Ok((map(condition)?, map(owner)?)))
            .collect::<Result<_, E>>()?;

        mapped.extracted_values = self.extracted_values.iter().map(|(term, value)| Ok((*term, map(value)?)))
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

        mapped.assumptions = super::call_proof::map_assumptions(&self.assumptions, &mut map)?;
        mapped.trusted_assumptions = super::call_proof::map_assumptions(&self.trusted_assumptions, &mut map)?;

        Ok(mapped)
    }

    /// Exact local callable body established by the entry value flow, if known.
    pub fn anonymous_target(&self) -> Option<&bray_bound_tree::BoundUnitKey> {
        self.anonymous_target.as_ref()
    }

    pub(crate) fn intersect(&mut self, other: &Self, cache: &super::ExecutionProofCache) -> bool {
        self.intersect_guarded(other, cache, None)
    }

    pub(super) fn intersect_guarded(
        &mut self,
        other: &Self,
        cache: &super::ExecutionProofCache,
        selections: Option<[&std::collections::BTreeSet<(super::ExecutionCondition, bool)>; 2]>,
    ) -> bool {
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
            self.storage_unchanged,
            self.memory_access,
            self.assumptions.len(),
            self.trusted_assumptions.len(),
            self.arguments.len(),
            self.witness_inputs.len(),
            self.witness_allocations.len(),
            self.extracted_values.len(),
        );

        let mut addresses_changed = false;
        let mut borrow_support_changed = false;

        self.storage_unchanged &= other.storage_unchanged;

        self.memory_access = self.memory_access.zip(other.memory_access).map(|(left, right)| {
            if left == right { left } else { MemoryAccessProof::Initialized }
        });

        self.borrow_owners.retain(|ordinal, owners| {
            let Some(incoming) = other.borrow_owners.get(ordinal) else {
                borrow_support_changed = true;

                return false;
            };

            if incoming.as_ref() != owners.as_ref() {
                let joined = bray_base::sorted_unique_shared_slice(owners.iter().chain(incoming.iter()).cloned());

                if joined.as_ref() != owners.as_ref() {
                    *owners = joined;
                    borrow_support_changed = true;
                }
            }

            true
        });

        self.addresses.retain(|input, addresses| {
            let Some(incoming) = other.addresses.get(input) else {
                addresses_changed = true;

                return false;
            };

            addresses.retain(|address, owners| {
                let Some(incoming) = incoming.get(address) else {
                    addresses_changed = true;

                    return false;
                };

                let count = owners.len();

                owners.extend(incoming.iter().cloned());
                addresses_changed |= owners.len() != count;

                true
            });

            !addresses.is_empty()
        });

        if self.anonymous_target != other.anonymous_target {
            self.anonymous_target = None;
        }

        self.trusted_boundary &= other.trusted_boundary;
        self.pending_execution |= other.pending_execution;

        let mut arguments_changed = false;
        let mut joined_facts = Vec::new();

        // A frame's selector constrains its own incoming capture. Use those
        // facts while joining values. The joined entry retains its common facts.
        let selected = selections.map(|[left, right]| {
            [(&self.assumptions, left), (&other.assumptions, right)]
                .map(|(assumptions, selection)| {
                    let mut assumptions = assumptions.clone();

                    assumptions.extend(selection.iter().cloned());

                    assumptions
                })
        });

        let left = cache.ordinary_proof(selected.as_ref().map_or(&self.assumptions, |proof| &proof[0]));
        let right = cache.ordinary_proof(selected.as_ref().map_or(&other.assumptions, |proof| &proof[1]));
        let outputs_changed = self.outputs.intersect(&other.outputs, (&left, &right));

        for (place, value) in &mut self.arguments {
            let incoming = other.arguments.get(place);

            if incoming == Some(value) { continue; }

            // Invocation entries use the same snapshot join as current values.
            // Loss of contents knowledge does not retire a common carrier or loan.
            let joined = incoming.map_or_else(super::ExecutionCondition::unknown, |incoming| {
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

        self.witness_inputs
            .retain(|owner, witness| other.witness_inputs.get(owner) == Some(&*witness));

        self.witness_allocations.retain(|condition, owner| {
            other.witness_allocations.get(condition) == Some(&*owner) && self.witness_inputs.contains_key(owner)
        });

        self.extracted_values.retain(|term, value| other.extracted_values.get(term) == Some(value));

        let failures = match (self.failure_rows.as_deref(), other.failure_rows.as_deref()) {
            (Some(left), Some(right)) if left == right => self.failure_rows.clone(),
            (Some(left), Some(right)) => {
                let mut rows = std::collections::BTreeMap::<_, ExecutionFailureRow>::new();

                for row in left.iter().chain(right) {
                    rows.entry(row.index).and_modify(|bound| {
                        assert_eq!(bound.kind, row.kind, "one admitted failure row retains its kind");

                        if bound.guard != row.guard { bound.guard = None; }
                    }).or_insert_with(|| row.clone());
                }

                Some(rows.into_values().collect())
            }
            _ => None,
        };

        let failures_changed = self.failure_rows != failures;

        self.failure_rows = failures;

        addresses_changed
            || borrow_support_changed
            || failures_changed
            || outputs_changed
            || arguments_changed
            || assumptions_changed
            || before
                != (
                    self.anonymous_target.is_some(),
                    self.trusted_boundary,
                    self.pending_execution,
                    self.storage_unchanged,
                    self.memory_access,
                    self.assumptions.len(),
                    self.trusted_assumptions.len(),
                    self.arguments.len(),
                    self.witness_inputs.len(),
                    self.witness_allocations.len(),
                    self.extracted_values.len(),
                )
    }

    /// Borrows the inputs that obligation selection reads.
    pub fn proof_inputs(&self) -> super::ExecutionCallProof<'_> {
        super::ExecutionCallProof::new(self)
    }

    /// Proves every entry condition, retaining the first condition and its actual arguments on failure.
    pub fn prove_requirements<'a>(
        &self,
        conditions: impl IntoIterator<Item = &'a super::ExecutionCondition>,
    ) -> Result<(), UnprovenExecutionCondition> {
        self.proof_inputs().prove_requirements(conditions)
    }

    /// Proves every trusted obligation from the captured live evidence.
    pub fn prove_trusted_requirements(&self, conditions: &[super::ExecutionCondition]) -> Result<(), UnprovenExecutionCondition> {
        self.proof_inputs().prove_trusted_requirements(conditions)
    }

    /// Checks a declaration's own inputs against captured receiver-first arguments.
    pub fn prove_trusted_with_inputs(
        &self,
        conditions: &[super::ExecutionCondition],
        inputs: &[bray_bound_tree::BoundReferenceTarget],
    ) -> Result<(), UnprovenExecutionCondition> {
        self.proof_inputs().prove_trusted_with_inputs(conditions, inputs)
    }
}

#[cfg(test)]
mod tests {
    use super::ExecutionCallEvidence;
    use crate::{ExecutionCondition, ExecutionPlace};

    #[test]
    fn joined_admission_requires_live_address_support_on_both_predecessors() {
        let argument = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0));
        let address = ExecutionCondition::new(crate::ExecutionConditionKind::Input(argument.clone()));
        let left = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(1));
        let right = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(2));

        let entry = |owner| ExecutionCallEvidence {
            arguments: [(argument.clone(), address.clone())].into(),
            addresses: [(argument.clone(), [(address.clone(), [owner].into())].into())].into(),
            ..Default::default()
        };

        let cache = crate::execution_guarantees::ExecutionProofCache::default();
        let mut joined = entry(left.clone());

        assert!(joined.intersect(&entry(right.clone()), &cache));
        assert_eq!(joined.addresses[&argument][&address], [left, right].into());

        let mut expired = joined.clone();

        expired.addresses.clear();

        assert!(joined.intersect(&expired, &cache));
        assert!(joined.addresses.is_empty());
    }

    #[test]
    fn borrowed_boolean_requirements_read_current_contents_without_creating_authority() {
        let argument = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0));
        let owner = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(1));
        let field = bray_symbols::ConstantProjectionKind::TupleElement(bray_symbols::SymbolOrdinal::new(0));
        let required = ExecutionCondition::new(crate::ExecutionConditionKind::Input(argument.clone().component(field)));
        let contents = ExecutionCondition::new(crate::ExecutionConditionKind::Input(owner.clone().component(field)));
        let mut entry = ExecutionCallEvidence::default();

        entry.arguments.insert(argument.clone(), ExecutionCondition::new(crate::ExecutionConditionKind::Input(owner)).borrowed());
        entry.assumptions.insert((contents.clone(), true));

        assert!(entry.prove_requirements([&required]).is_ok());
        assert!(entry.prove_requirements([&ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(std::sync::Arc::new(required.clone())))]).is_err());

        entry.assumptions.clear();

        assert!(entry.prove_requirements([&required]).is_err());

        entry.assumptions.insert((contents, false));

        assert!(entry.prove_requirements([&required]).is_err());

        entry.arguments.insert(argument, ExecutionCondition::new(crate::ExecutionConditionKind::Input(
            ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(2)))).borrowed());

        assert!(entry.prove_requirements([&required]).is_err());
    }

    #[test]
    fn failed_entry_proofs_retain_the_requirement_and_actual_value() {
        let input = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0));
        let required = ExecutionCondition::new(crate::ExecutionConditionKind::Input(input.clone()));

        for actual in [ExecutionCondition::boolean(false), ExecutionCondition::unknown()] {
            let mut entry = ExecutionCallEvidence::default();

            entry.arguments.insert(input.clone(), actual.clone());

            let failure = entry.prove_requirements([&required]).expect_err("unproven input cannot supply authority");

            assert_eq!(failure.required(), &required);
            assert_eq!(failure.actual(), &actual);
        }

        let mut entry = ExecutionCallEvidence::default();

        entry.arguments.insert(input, ExecutionCondition::boolean(true));

        assert!(entry.prove_requirements([&required]).is_ok());
    }
}
