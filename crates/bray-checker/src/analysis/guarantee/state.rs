use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::BoundExpressionId;

use crate::ExecutionCondition;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExecutionState {
    pub(super) trust_boundaries: BTreeSet<BoundExpressionId>,
    pub(super) assumptions: BTreeSet<(ExecutionCondition, bool)>,
    pub(super) witnesses:
        BTreeMap<ExecutionCondition, crate::execution_guarantees::ExecutionWitness>,
    pub(super) trusted_assumptions: BTreeSet<(ExecutionCondition, bool)>,
    pub(super) allocation_owners: BTreeMap<ExecutionCondition, ExecutionCondition>,
    pub(super) current: BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    pub(super) expressions: BTreeMap<BoundExpressionId, ExecutionCondition>,
    pub(super) pending_results: BTreeMap<BoundExpressionId, ExecutionCondition>,
    pub(super) result: ExecutionCondition,
    pub(super) entries: BTreeMap<bray_bound_tree::BoundExecutionSite, crate::ExecutionCallEvidence>,
    pub(super) deferred_invocations:
        BTreeMap<ExecutionCondition, bray_bound_tree::BoundExecutionSite>,
    pub(super) completion_dependencies: BTreeSet<crate::ExecutionCompletionDependency>,
}

impl Default for ExecutionState {
    fn default() -> Self {
        Self {
            trust_boundaries: BTreeSet::new(),
            assumptions: BTreeSet::new(),
            witnesses: BTreeMap::new(),
            trusted_assumptions: BTreeSet::new(),
            allocation_owners: BTreeMap::new(),
            current: BTreeMap::new(),
            expressions: BTreeMap::new(),
            pending_results: BTreeMap::new(),
            result: ExecutionCondition::Unknown,
            entries: BTreeMap::new(),
            deferred_invocations: BTreeMap::new(),
            completion_dependencies: BTreeSet::new(),
        }
    }
}

pub(super) fn update_parent_observations(
    observations: &mut BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    place: &crate::ExecutionPlace,
    value: &ExecutionCondition,
) {
    for length in 0..place.projections.len() {
        let mut parent = place.clone();

        parent.projections = place.projections[..length].into();

        if let Some(snapshot) = observations.get_mut(&parent) {
            *snapshot = snapshot.with_component(&place.projections[length..], value.clone());
        }
    }
}

impl ExecutionState {
    pub(super) fn restore_returned_evidence(
        &mut self,
        returned: crate::execution_guarantees::ExecutionValueEvidence,
    ) {
        // Checked whole-input forwarding has no storage effects on a normal exit.
        // Failure paths never reach this restoration and retain their original invalidation.
        for (carrier, witness) in returned.witnesses {
            for condition in witness.conditions.keys() {
                condition
                    .clone()
                    .assume(true, &mut self.trusted_assumptions);
            }

            let retained = self.witnesses.entry(carrier).or_default();

            retained.dependencies.extend(witness.dependencies);

            for (condition, guarantee) in witness.conditions {
                let entry = retained.conditions.entry(condition).or_default();

                entry.witness |= guarantee.witness;

                for (address, owners) in guarantee.addresses {
                    entry.addresses.entry(address).or_default().extend(owners);
                }
            }
        }

        self.allocation_owners.extend(returned.allocations);
    }

    pub(super) fn call_trusted_assumptions(
        &self,
        transferred: impl IntoIterator<Item = ExecutionCondition>,
    ) -> BTreeSet<(ExecutionCondition, bool)> {
        let mut transferred = transferred.into_iter().collect::<Vec<_>>();
        let mut owners = BTreeSet::new();

        while let Some(value) = transferred.pop() {
            if value == ExecutionCondition::Unknown {
                continue;
            }

            transferred.extend(value.owned_components().cloned());

            owners.insert(value);
        }

        self.trusted_assumptions
            .iter()
            .filter(|(condition, _)| {
                let mut pending = vec![condition];

                while let Some(condition) = pending.pop() {
                    if let Some(owner) = self.allocation_owners.get(condition)
                        && !owners.contains(owner)
                    {
                        return false;
                    }

                    match condition {
                        ExecutionCondition::Trusted(condition) => pending.push(condition),
                        ExecutionCondition::Operation(_, operands) => {
                            pending.extend(operands.iter())
                        }
                        _ => {}
                    }
                }

                true
            })
            .cloned()
            .collect()
    }

    pub(super) fn carry_guarantee(
        &mut self,
        carrier: ExecutionCondition,
        condition: ExecutionCondition,
    ) {
        let witness = self
            .witnesses
            .entry(carrier.owner_identity().clone())
            .or_default();

        witness.carry(&condition, true, &Default::default());
    }

    /// Snapshot access bounds independently of the owning carrier's later consumption.
    pub(super) fn address_dependencies(
        &self,
        value: &ExecutionCondition,
    ) -> crate::execution_guarantees::AddressDependencies {
        let mut addresses = crate::execution_guarantees::AddressDependencies::new();

        for value in value.owned_values() {
            let mut carrier = value.owner_identity().clone();

            loop {
                if let Some(witness) = self.witnesses.get(&carrier) {
                    for (condition, guarantee) in &witness.conditions {
                        if condition.prove_trusted(&self.trusted_assumptions, &self.assumptions)
                            != Some(true)
                        {
                            continue;
                        }

                        for (address, owners) in &guarantee.addresses {
                            if address.is_owned_projection_of(value) {
                                addresses
                                    .entry(address.clone())
                                    .or_default()
                                    .extend(owners.iter().cloned());
                            }
                        }
                    }
                }

                carrier = match carrier {
                    ExecutionCondition::Projection(_, parent) => parent.owner_identity().clone(),
                    ExecutionCondition::Input(mut place) if !place.projections.is_empty() => {
                        place.projections = place.projections[..place.projections.len() - 1].into();

                        ExecutionCondition::Input(place)
                    }
                    _ => break,
                };
            }
        }

        addresses
    }

    pub(super) fn directly_carries_witness(&self, value: &ExecutionCondition) -> bool {
        self.witnesses
            .get(value.owner_identity())
            .is_some_and(|witness| {
                witness.conditions.iter().any(|(condition, guarantee)| {
                    guarantee.witness
                        && (self
                            .trusted_assumptions
                            .contains(&(condition.clone(), true))
                            || condition
                                .prove_trusted(&self.trusted_assumptions, &self.assumptions)
                                == Some(true))
                        && condition
                            .prove(&self.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
                            != Some(true)
                })
            })
    }

    pub(super) fn is_witness(&self, value: &ExecutionCondition) -> bool {
        self.witnesses.keys().any(|carrier| {
            value
                .owned_values()
                .any(|value| carrier.is_owned_projection_of(value))
                && self.directly_carries_witness(carrier)
        })
    }

    pub(super) fn witness_projection(&self, value: &ExecutionCondition) -> (bool, bool) {
        let mut whole = self.is_witness(value);
        let mut partial = false;
        let mut projected_part = false;
        let mut value = value;

        while let ExecutionCondition::Projection(projection, parent) = value {
            projected_part |= *projection != bray_symbols::ConstantProjectionKind::NullableValue;

            if self.directly_carries_witness(parent) {
                whole |= !projected_part;
                partial |= projected_part;
            }

            value = parent;
        }

        (whole, partial)
    }
    /// Reusing a finite evaluation identity must not revive a previous iteration's evidence.
    pub(super) fn retire_evaluation(&mut self, mutation: bray_bound_tree::StorageMutation) {
        let stale = |value: &ExecutionCondition| value.depends_on(mutation);

        self.witnesses.retain(|value, witness| {
            witness.conditions.retain(|condition, guarantee| {
                !stale(condition) && !guarantee.addresses.keys().any(stale)
            });

            !stale(value) && (!witness.conditions.is_empty() || !witness.dependencies.is_empty())
        });

        self.deferred_invocations.retain(|value, source| {
            !stale(value)
                && !source
                    .expression()
                    .is_some_and(|expression| stale(&ExecutionCondition::Expression(expression)))
        });

        self.trusted_assumptions
            .retain(|(condition, _)| !stale(condition));

        self.assumptions.retain(|(condition, _)| !stale(condition));

        self.allocation_owners
            .retain(|condition, owner| !stale(condition) && !stale(owner));

        for entry in self
            .entries
            .values_mut()
            .filter(|entry| entry.pending_execution)
        {
            if entry
                .returned
                .witnesses
                .iter()
                .any(|(carrier, witness)| stale(carrier) || witness.conditions.keys().any(stale))
            {
                entry.returned = Default::default();
            }

            entry.arguments.retain(|_, value| !stale(value));
            entry.assumptions.retain(|(condition, _)| !stale(condition));

            entry
                .trusted_assumptions
                .retain(|(condition, _)| !stale(condition));
        }

        for value in self
            .current
            .values_mut()
            .chain(self.expressions.values_mut())
            .chain(self.pending_results.values_mut())
            .chain(std::iter::once(&mut self.result))
        {
            if !stale(value) {
                continue;
            }

            if let ExecutionCondition::Constructed(identity, fields) = value
                && !stale(identity)
            {
                // A repeated payload mutation does not recreate the descriptor or its siblings.
                let fields = fields
                    .iter()
                    .map(|(projection, component)| {
                        (
                            *projection,
                            if stale(component) {
                                ExecutionCondition::Unknown
                            } else {
                                component.clone()
                            },
                        )
                    })
                    .collect();

                *value = ExecutionCondition::Constructed(std::sync::Arc::clone(identity), fields);
            } else {
                *value = ExecutionCondition::Unknown;
            }
        }
    }

    pub(super) fn invalidate_cleanup(
        &mut self,
        stable_values: &BTreeSet<ExecutionCondition>,
        stable_owners: &BTreeSet<ExecutionCondition>,
    ) {
        self.refine_witnesses();

        self.assumptions.retain(|(condition, _)| {
            !condition.observes_borrowed(None) || condition.observes_only(stable_values)
        });

        let mut retained = BTreeSet::new();

        self.witnesses.retain(|carrier, witness| {
            if !stable_owners
                .iter()
                .any(|owner| carrier.is_owned_projection_of(owner))
            {
                return false;
            }

            witness
                .conditions
                .retain(|condition, _| condition.observes_only(stable_values));

            for condition in witness.conditions.keys() {
                condition.clone().assume(true, &mut retained);
            }

            !witness.conditions.is_empty()
        });

        // Opaque predicates cannot observe mutable state absent from their arguments.
        // Preserve established facts with no observations, without creating new evidence.
        let no_values = BTreeSet::new();

        let preserves = |fact: &(ExecutionCondition, bool)| {
            retained.contains(fact) || fact.0.observes_only(&no_values)
        };

        self.trusted_assumptions.retain(preserves);

        self.allocation_owners.clear();

        for entry in self
            .entries
            .values_mut()
            .filter(|entry| entry.pending_execution)
        {
            entry
                .assumptions
                .retain(|(condition, _)| condition.observes_only(stable_values));

            entry.trusted_assumptions.retain(preserves);

            entry.trusted_boundary = false;
        }
    }

    pub(super) fn join_values(
        &mut self,
        replacements: &BTreeMap<ExecutionCondition, ExecutionCondition>,
        identities: &BTreeMap<ExecutionCondition, ExecutionCondition>,
    ) {
        if replacements.is_empty() && identities.is_empty() {
            return;
        }

        let rename =
            |value: &ExecutionCondition| value.with_joined_values(replacements, identities);

        let rename_identity =
            |value: &ExecutionCondition| value.with_joined_values(identities, identities);

        for facts in [&mut self.assumptions, &mut self.trusted_assumptions] {
            let mut renamed = BTreeSet::new();

            for (condition, value) in facts.iter() {
                rename(condition).assume(*value, &mut renamed);
            }

            *facts = renamed;
        }

        let mut witnesses = BTreeMap::<_, crate::execution_guarantees::ExecutionWitness>::new();

        for (carrier, witness) in &self.witnesses {
            let carrier = rename_identity(carrier);

            if carrier == ExecutionCondition::Unknown {
                continue;
            }

            let retained = witnesses.entry(carrier).or_default();

            retained
                .dependencies
                .extend(witness.dependencies.iter().cloned());

            for (condition, guarantee) in &witness.conditions {
                let condition = rename(condition);

                if condition == ExecutionCondition::Unknown {
                    continue;
                }

                let entry = retained.conditions.entry(condition).or_default();

                entry.witness |= guarantee.witness;

                for (address, owners) in &guarantee.addresses {
                    let address = rename(address);

                    if address != ExecutionCondition::Unknown {
                        entry
                            .addresses
                            .entry(address)
                            .or_default()
                            .extend(owners.iter().cloned());
                    }
                }
            }
        }

        self.witnesses = witnesses;

        self.allocation_owners = self
            .allocation_owners
            .iter()
            .map(|(condition, owner)| (rename(condition), rename_identity(owner)))
            .filter(|(condition, _)| *condition != ExecutionCondition::Unknown)
            .collect();

        let mut computations = BTreeMap::<_, Option<_>>::new();

        for (value, source) in &self.deferred_invocations {
            let value = rename_identity(value);

            if value != ExecutionCondition::Unknown {
                computations
                    .entry(value)
                    .and_modify(|previous| {
                        if *previous != Some(*source) {
                            *previous = None;
                        }
                    })
                    .or_insert(Some(*source));
            }
        }

        self.deferred_invocations = computations
            .into_iter()
            .filter_map(|(value, source)| Some((value, source?)))
            .collect();

        for value in self.current.values_mut() {
            *value = rename(value);
        }
    }

    pub(super) fn assign(&mut self, place: crate::ExecutionPlace, value: ExecutionCondition) {
        let replaced = self
            .current
            .range(place.clone()..)
            .take_while(|(observed, _)| place.contains(observed))
            .map(|(observed, _)| observed.clone())
            .collect::<Vec<_>>();

        for observed in replaced {
            self.current.remove(&observed);
        }

        update_parent_observations(&mut self.current, &place, &value);

        self.current.insert(place, value);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use bray_symbols::{ConstantProjectionKind, SymbolOrdinal};

    use super::ExecutionState;
    use crate::{ExecutionCondition, ExecutionPlace};

    fn input(ordinal: u32) -> ExecutionCondition {
        ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal)))
    }

    fn permission(ordinal: u32) -> ExecutionCondition {
        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>(
            "ValidRead",
        );

        ExecutionCondition::Trusted(Arc::new(ExecutionCondition::Predicate(
            symbol.into(),
            crate::test_support::empty_substitution(symbol.into()),
            [input(ordinal), ExecutionCondition::Boolean(true)].into(),
        )))
    }

    #[test]
    fn call_join_retains_possible_address_bounds_without_preserving_unproven_facts() {
        let argument = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let address = input(2);
        let condition = permission(2);
        let mut left = crate::ExecutionCallEvidence::default();
        let mut right = crate::ExecutionCallEvidence::default();

        for (entry, owner) in [(&mut left, 30), (&mut right, 31)] {
            entry.addresses.entry(argument.clone()).or_default().insert(
                address.clone(),
                BTreeSet::from([ExecutionPlace::argument(SymbolOrdinal::new(owner))]),
            );
        }

        condition
            .clone()
            .assume(true, &mut left.trusted_assumptions);

        assert!(left.intersect(&right));
        assert_eq!(left.addresses[&argument][&address].len(), 2);
        assert!(!left.proves_trusted(&[condition]));
        assert!(!left.intersect(&right));
    }

    #[test]
    fn address_bounds_lookup_uses_only_live_facts_on_the_complete_carrier() {
        let owner = ExecutionPlace::argument(SymbolOrdinal::new(30));
        let carrier = input(2);

        let address = ExecutionCondition::Projection(
            ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)),
            Arc::new(carrier.clone()),
        );

        let access = ExecutionCondition::Trusted(Arc::new(address.clone()));
        let mut state = ExecutionState::default();

        state
            .witnesses
            .entry(carrier)
            .or_default()
            .conditions
            .entry(access.clone())
            .or_default()
            .addresses
            .insert(address.clone(), BTreeSet::from([owner.clone()]));

        assert!(state.address_dependencies(&address).is_empty());
        access.clone().assume(true, &mut state.trusted_assumptions);

        assert_eq!(
            state.address_dependencies(&address),
            std::collections::BTreeMap::from([(address.clone(), BTreeSet::from([owner]))])
        );

        assert!(state.address_dependencies(&input(3)).is_empty());
        state.trusted_assumptions.clear();
        assert!(state.address_dependencies(&address).is_empty());
    }

    #[test]
    fn copyable_access_facts_expire_without_creating_ownership_witnesses() {
        let pointer = input(2);
        let access = permission(2);
        let owner = ExecutionPlace::argument(SymbolOrdinal::new(30));
        let mut state = ExecutionState::default();

        state.current.insert(owner.clone(), input(30));

        state
            .witnesses
            .entry(pointer.clone())
            .or_default()
            .conditions
            .entry(access.clone())
            .or_default()
            .addresses
            .insert(pointer.clone(), BTreeSet::from([owner.clone()]));

        access.clone().assume(true, &mut state.trusted_assumptions);
        assert!(!state.directly_carries_witness(&pointer));
        assert!(state.trusted_assumptions.contains(&(access.clone(), true)));
        state.invalidate_trusted_roots(&BTreeSet::from([owner.root]));
        assert!(!state.trusted_assumptions.contains(&(access, true)));
        assert!(!state.directly_carries_witness(&pointer));
    }

    #[test]
    fn address_expiry_retires_only_bound_clauses_of_a_complete_carrier() {
        for (whole_lifetime, by_root) in [(false, false), (false, true), (true, true)] {
            let carrier = input(20);
            let marker = permission(1);
            let access = permission(2);
            let owner = ExecutionPlace::argument(SymbolOrdinal::new(30));
            let mut state = ExecutionState::default();

            state.current.insert(owner.clone(), input(30));

            let compound = ExecutionCondition::operation(
                bray_bound_tree::BoundOperator::LogicalAnd,
                vec![marker.clone(), access.clone()],
            );

            state.carry_guarantee(carrier.clone(), compound.clone());
            compound.assume(true, &mut state.trusted_assumptions);

            let witness = state
                .witnesses
                .get_mut(&carrier)
                .expect("the complete carrier owns its clauses");

            witness
                .conditions
                .get_mut(&access)
                .expect("the access conjunct is retained")
                .addresses
                .insert(input(2), BTreeSet::from([owner.clone()]));

            if whole_lifetime {
                witness.dependencies.insert(owner.clone());
            }

            if by_root {
                state.invalidate_trusted_roots(&BTreeSet::from([owner.root]));
            } else {
                state.invalidate_trusted(&input(30));
            }

            assert!(!state.trusted_assumptions.contains(&(access, true)));

            assert_eq!(
                state.trusted_assumptions.contains(&(marker.clone(), true)),
                !whole_lifetime
            );

            if !whole_lifetime {
                assert!(state.directly_carries_witness(&carrier));
                state.invalidate_trusted(&carrier);
                assert!(!state.trusted_assumptions.contains(&(marker, true)));
            }
        }
    }

    #[test]
    fn equal_payload_values_do_not_merge_distinct_address_storage_lifetimes() {
        let carrier = input(20);
        let mut state = ExecutionState::default();

        for ordinal in [30, 31] {
            let owner = ExecutionPlace::argument(SymbolOrdinal::new(ordinal));
            let access = permission(ordinal);

            state
                .current
                .insert(owner.clone(), ExecutionCondition::Boolean(false));

            state.carry_guarantee(carrier.clone(), access.clone());
            access.clone().assume(true, &mut state.trusted_assumptions);

            state
                .witnesses
                .get_mut(&carrier)
                .expect("one carrier retains both addresses")
                .conditions
                .get_mut(&access)
                .expect("each access has its own clause")
                .addresses
                .insert(input(ordinal), BTreeSet::from([owner]));
        }

        state.invalidate_trusted_roots(&BTreeSet::from([ExecutionPlace::argument(
            SymbolOrdinal::new(30),
        )
        .root]));

        state.invalidate_trusted(&ExecutionCondition::Boolean(false));
        assert!(!state.trusted_assumptions.contains(&(permission(30), true)));
        assert!(state.trusted_assumptions.contains(&(permission(31), true)));
    }

    #[test]
    fn ordinary_nullable_guards_do_not_share_sibling_address_bounds() {
        let root = input(0);

        let payload = root
            .clone()
            .component(ConstantProjectionKind::NullableValue);

        let marker = payload
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));

        let address =
            payload.component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)));

        let absent = ExecutionCondition::Literal(Arc::new(bray_symbols::ConstantValueData::new(
            crate::test_support::error_type(),
            bray_symbols::ConstantValueKind::NullableAbsent,
        )));

        let guard = ExecutionCondition::operation(
            bray_bound_tree::BoundOperator::Equal,
            vec![root, absent],
        );

        let guarded = |claim| {
            ExecutionCondition::operation(
                bray_bound_tree::BoundOperator::LogicalOr,
                vec![guard.clone(), claim],
            )
        };

        let marker = guarded(permission(1).with_equalities(&[(input(1), marker)].into()));
        let access = guarded(permission(2).with_equalities(&[(input(2), address.clone())].into()));

        let condition = ExecutionCondition::operation(
            bray_bound_tree::BoundOperator::LogicalAnd,
            vec![marker.clone(), access.clone()],
        );

        let owners = BTreeSet::from([ExecutionPlace::argument(SymbolOrdinal::new(30))]);
        let mut witness = crate::execution_guarantees::ExecutionWitness::default();

        witness.carry(
            &condition,
            true,
            &[(address.clone(), owners.clone())].into(),
        );

        assert!(witness.conditions[&marker].addresses.is_empty());

        assert_eq!(
            witness.conditions[&access].addresses.get(&address),
            Some(&owners)
        );
    }

    #[test]
    fn guarded_refinement_preserves_only_the_derived_clauses_address_dependencies() {
        for select_marker in [false, true] {
            let carrier = input(20);
            let marker = permission(1);
            let access = permission(2);
            let flag = input(10);
            let owner = ExecutionPlace::argument(SymbolOrdinal::new(30));

            let guarded = ExecutionCondition::operation(
                bray_bound_tree::BoundOperator::LogicalOr,
                vec![
                    ExecutionCondition::operation(
                        bray_bound_tree::BoundOperator::LogicalAnd,
                        vec![flag.clone(), marker.clone()],
                    ),
                    ExecutionCondition::operation(
                        bray_bound_tree::BoundOperator::LogicalAnd,
                        vec![
                            ExecutionCondition::operation(
                                bray_bound_tree::BoundOperator::LogicalNot,
                                vec![flag.clone()],
                            ),
                            access.clone(),
                        ],
                    ),
                ],
            );

            let mut state = ExecutionState::default();

            state.carry_guarantee(carrier.clone(), guarded.clone());
            guarded.clone().assume(true, &mut state.trusted_assumptions);

            state
                .witnesses
                .get_mut(&carrier)
                .expect("the conditional has one complete carrier")
                .conditions
                .get_mut(&guarded)
                .expect("the guarded clause is retained")
                .addresses
                .insert(input(2), BTreeSet::from([owner.clone()]));

            state.assumptions.insert((flag, select_marker));
            state.refine_witnesses();

            assert_eq!(
                state.trusted_assumptions.contains(&(marker.clone(), true)),
                select_marker
            );

            assert_eq!(
                state.trusted_assumptions.contains(&(access.clone(), true)),
                !select_marker
            );

            state.invalidate_trusted_roots(&BTreeSet::from([owner.root]));

            assert_eq!(
                state.trusted_assumptions.contains(&(marker, true)),
                select_marker
            );

            assert!(!state.trusted_assumptions.contains(&(access, true)));
        }
    }

    #[test]
    fn guarded_refinement_never_promotes_unproved_carrier_obligations() {
        for guard_holds in [false, true] {
            let predicate = permission(2);
            let guard = input(3);

            let conditional = ExecutionCondition::operation(
                bray_bound_tree::BoundOperator::LogicalOr,
                vec![guard.clone(), predicate.clone()],
            );

            let mut state = ExecutionState::default();

            state.carry_guarantee(input(0), predicate.clone());
            state.carry_guarantee(input(0), conditional.clone());
            conditional.assume(true, &mut state.trusted_assumptions);
            state.assumptions.insert((guard, guard_holds));
            state.refine_witnesses();

            assert_eq!(
                state.trusted_assumptions.contains(&(predicate, true)),
                !guard_holds
            );
        }
    }

    #[test]
    fn discharging_a_guard_never_borrows_authority_from_another_carrier() {
        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>(
            "ValidWrite",
        );

        let substitution = crate::test_support::empty_substitution(symbol.into());

        let predicate = ExecutionCondition::Trusted(Arc::new(ExecutionCondition::Predicate(
            symbol.into(),
            substitution,
            [input(2).borrowed(), ExecutionCondition::Boolean(true)].into(),
        )));

        let guard = input(3);

        let conditional = ExecutionCondition::operation(
            bray_bound_tree::BoundOperator::LogicalOr,
            vec![guard.clone(), predicate.clone()],
        );

        let mut state = ExecutionState::default();

        state.carry_guarantee(input(0), conditional.clone());
        state.carry_guarantee(input(1), predicate.clone());
        conditional.assume(true, &mut state.trusted_assumptions);

        predicate
            .clone()
            .assume(true, &mut state.trusted_assumptions);

        state.assumptions.insert((guard.clone(), true));

        state.refine_witnesses();

        assert!(
            !state.witnesses[&input(0)]
                .conditions
                .contains_key(&predicate)
        );

        assert!(
            state.witnesses[&input(1)]
                .conditions
                .contains_key(&predicate)
        );

        state.assumptions = [(guard, false)].into();
        state.refine_witnesses();

        assert!(
            state.witnesses[&input(0)]
                .conditions
                .contains_key(&predicate)
        );
    }

    #[test]
    fn repeated_cleanup_does_not_revive_old_evidence_or_retire_its_sibling() {
        use crate::test_support::{error_type, expression_unit, push_block, push_expression};

        use bray_bound_tree::{
            BoundErrorExpression, BoundExpression, BoundUnitId, BoundUnitKind, StorageAccess,
            StorageAccessRoot, StorageIdentity, StorageMutation, StoragePlanBuilder,
        };

        use bray_symbols::TypeAssociatedLifecycleSlot;

        let unit = BoundUnitId::new(1);
        let mut boundary = None;

        let (bound, expressions) = expression_unit(unit, |tree, origin| {
            boundary = Some(push_block(tree, origin, []));

            vec![push_expression(
                tree,
                BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
            )]
        });

        let occurrence =
            bray_bound_tree::BoundExecutionSite::Node(boundary.expect("test boundary").into());

        let mut storage = StoragePlanBuilder::new(unit, BoundUnitKind::CallableBody);

        let root = storage
            .push_identity(StorageIdentity::Temporary(expressions[0]))
            .expect("fixture identity must fit its identity space");

        let source = bound
            .tree()
            .expression(expressions[0])
            .expect("fixture expression must exist in its bound unit")
            .origin()
            .source_anchor();

        let access = storage
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(root),
                [],
                error_type(),
                source,
                false,
            ))
            .expect("fixture access must fit its identity space");

        let mutation = |part| StorageMutation::Lifecycle {
            occurrence,
            access,
            part,
            slot: TypeAssociatedLifecycleSlot::Destructor,
        };

        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let old = ExecutionCondition::PostState(mutation(0), place.clone());
        let sibling = ExecutionCondition::PostState(mutation(1), place.clone());
        let mut state = ExecutionState::default();

        state.carry_guarantee(old.clone(), old.clone());
        state.carry_guarantee(sibling.clone(), sibling.clone());

        state
            .trusted_assumptions
            .extend([(old.clone(), true), (sibling.clone(), true)]);

        state.current.insert(place.clone(), old.clone());

        state.entries.insert(
            occurrence,
            crate::ExecutionCallEvidence {
                returned: Default::default(),
                addresses: Default::default(),
                anonymous_target: None,
                trusted_boundary: true,
                pending_execution: true,
                arguments: [(place.clone(), old.clone())].into(),
                assumptions: [(old.clone(), true)].into(),
                trusted_assumptions: [(old.clone(), true)].into(),
            },
        );

        state.retire_evaluation(mutation(0));
        state.assign(place, old.clone());

        assert!(!state.is_witness(&old));
        assert!(!state.trusted_assumptions.contains(&(old, true)));
        assert!(state.is_witness(&sibling));
        assert!(state.trusted_assumptions.contains(&(sibling, true)));
        assert!(state.entries[&occurrence].arguments.is_empty());
        assert!(state.entries[&occurrence].trusted_assumptions.is_empty());
    }

    #[test]
    fn content_changes_keep_spatial_evidence_but_never_extend_its_carrier_or_storage() {
        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>(
            "ValidWrite",
        );

        let substitution = crate::test_support::empty_substitution(symbol.into());
        let owner = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let carrier = input(1);

        let spatial = ExecutionCondition::Trusted(Arc::new(ExecutionCondition::Predicate(
            symbol.into(),
            substitution,
            [carrier.clone(), ExecutionCondition::Boolean(true)].into(),
        )));

        let observed = input(0).borrowed();
        let mut state = ExecutionState::default();

        state.current.insert(owner.clone(), input(0));

        state
            .witnesses
            .entry(carrier.clone())
            .or_default()
            .dependencies = BTreeSet::from([owner.clone()]);

        state.carry_guarantee(carrier.clone(), spatial.clone());
        state.carry_guarantee(carrier.clone(), observed.clone());

        state
            .trusted_assumptions
            .extend([(spatial.clone(), true), (observed, true)]);

        let stable = BTreeSet::from([symbol.into()]);

        state.invalidate_contents(&BTreeSet::from([owner.clone()]), [input(0)], &stable);

        assert_eq!(
            state.trusted_assumptions,
            BTreeSet::from([(spatial.clone(), true)])
        );

        assert!(!state.witnesses[&carrier].dependencies.is_empty());

        let mut ended = state.clone();

        ended.invalidate_trusted_roots(&BTreeSet::from([owner.root]));
        assert!(ended.trusted_assumptions.is_empty());

        assert!(
            ended
                .witnesses
                .values()
                .all(|witness| witness.dependencies.is_empty())
        );

        // A carrier can establish a condition about a separate pointer. Overwriting
        // that carrier must revoke the condition even though the pointer is unchanged.
        let external = input(2);

        state.witnesses.clear();
        state.carry_guarantee(external.clone(), spatial.clone());
        state.invalidate_contents(&BTreeSet::new(), [external], &stable);
        assert!(state.trusted_assumptions.is_empty());

        assert!(
            state
                .witnesses
                .values()
                .all(|witness| witness.conditions.is_empty())
        );

        let mut overwritten = ExecutionState::default();

        overwritten.carry_guarantee(carrier.clone(), spatial.clone());
        overwritten.trusted_assumptions.insert((spatial, true));
        overwritten.invalidate_contents(&BTreeSet::new(), [carrier], &stable);
        assert!(overwritten.trusted_assumptions.is_empty());
    }

    #[test]
    fn content_changes_retain_intermediate_lifetime_dependencies_without_old_guarantees() {
        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>(
            "ValidWrite",
        );

        let substitution = crate::test_support::empty_substitution(symbol.into());
        let owner = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let intermediate = ExecutionPlace::argument(SymbolOrdinal::new(1));
        let carrier = input(2);

        let spatial = ExecutionCondition::Trusted(Arc::new(ExecutionCondition::Predicate(
            symbol.into(),
            substitution,
            [carrier.clone(), ExecutionCondition::Boolean(true)].into(),
        )));

        let mut state = ExecutionState::default();

        state.current.insert(owner.clone(), input(0));
        state.current.insert(intermediate.clone(), input(1));

        state.witnesses.entry(input(1)).or_default().dependencies = BTreeSet::from([owner.clone()]);

        state
            .witnesses
            .entry(carrier.clone())
            .or_default()
            .dependencies = BTreeSet::from([intermediate]);

        state.carry_guarantee(carrier, spatial.clone());
        state.trusted_assumptions.insert((spatial.clone(), true));

        state.invalidate_contents(
            &BTreeSet::from([owner.clone()]),
            [input(0)],
            &BTreeSet::from([symbol.into()]),
        );

        assert_eq!(state.trusted_assumptions, BTreeSet::from([(spatial, true)]));
        assert!(!state.witnesses[&input(1)].dependencies.is_empty());

        state.invalidate_trusted_roots(&BTreeSet::from([owner.root]));
        assert!(state.trusted_assumptions.is_empty());

        assert!(
            state
                .witnesses
                .values()
                .all(|witness| witness.dependencies.is_empty())
        );
    }
    #[test]
    fn retiring_a_container_preserves_only_its_transferred_child_value() {
        let root = ExecutionPlace::argument(SymbolOrdinal::new(0));

        let left = root
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));

        let right = root
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)));

        let child = ExecutionCondition::Input(left);
        let mut state = ExecutionState::default();

        for value in [
            ExecutionCondition::Input(root.clone()),
            child.clone(),
            ExecutionCondition::Input(right),
        ] {
            state.carry_guarantee(value.clone(), value.clone());
            state.trusted_assumptions.insert((value, true));
        }

        let retained = BTreeSet::from([child.clone()]);
        let mut moved = state.clone();

        moved.retire_trusted_values([ExecutionCondition::Input(root.clone())], &retained);

        assert_eq!(
            moved
                .witnesses
                .iter()
                .filter(|(_, witness)| !witness.conditions.is_empty())
                .map(|(carrier, _)| carrier)
                .cloned()
                .collect::<BTreeSet<_>>(),
            retained
        );

        assert_eq!(
            moved.trusted_assumptions,
            BTreeSet::from([(child.clone(), true)])
        );

        // Retaining a value cannot extend the storage required by its witness.
        state.witnesses.entry(child).or_default().dependencies = BTreeSet::from([root.clone()]);

        state.invalidate_trusted_roots(&BTreeSet::from([root.root.clone()]));
        state.retire_trusted_values([ExecutionCondition::Input(root)], &retained);

        assert!(
            state
                .witnesses
                .values()
                .all(|witness| witness.conditions.is_empty())
        );

        assert!(state.trusted_assumptions.is_empty());
    }

    #[test]
    fn invalidation_follows_long_chains_and_cycles_without_retiring_independent_witnesses() {
        let mut state = ExecutionState::default();

        for ordinal in 1..=512 {
            let dependency = ExecutionPlace::argument(SymbolOrdinal::new(ordinal));
            let carrier = input(ordinal + 1);

            state.current.insert(dependency.clone(), input(ordinal));

            state
                .witnesses
                .entry(carrier.clone())
                .or_default()
                .dependencies = BTreeSet::from([dependency]);

            state.carry_guarantee(carrier.clone(), carrier.clone());
            state.trusted_assumptions.insert((carrier, true));
        }

        let cycle = ExecutionPlace::argument(SymbolOrdinal::new(514));
        let independent = input(1000);

        state.current.insert(cycle.clone(), input(513));

        state.witnesses.entry(input(1)).or_default().dependencies = BTreeSet::from([cycle]);

        state.carry_guarantee(independent.clone(), independent.clone());

        state
            .trusted_assumptions
            .insert((independent.clone(), true));

        for batch in 0..3 {
            let mut state = state.clone();

            if batch == 1 {
                state.invalidate_trusted_roots(&BTreeSet::from([
                    ExecutionPlace::argument(SymbolOrdinal::new(1)).root,
                    ExecutionPlace::argument(SymbolOrdinal::new(256)).root,
                ]));
            } else if batch == 2 {
                state.invalidate_contents(
                    &BTreeSet::from([
                        ExecutionPlace::argument(SymbolOrdinal::new(1)),
                        ExecutionPlace::argument(SymbolOrdinal::new(256)),
                    ]),
                    [input(1), input(256)],
                    &BTreeSet::new(),
                );

                assert!(state.trusted_assumptions.contains(&(input(2), true)));
                assert!(!state.trusted_assumptions.contains(&(input(256), true)));

                assert_eq!(
                    state
                        .witnesses
                        .values()
                        .filter(|witness| !witness.dependencies.is_empty())
                        .count(),
                    513
                );

                state.invalidate_trusted_roots(&BTreeSet::from([ExecutionPlace::argument(
                    SymbolOrdinal::new(1),
                )
                .root]));
            } else {
                state.invalidate_trusted(&input(1));
            }

            assert_eq!(
                state
                    .witnesses
                    .iter()
                    .filter(|(_, witness)| !witness.conditions.is_empty())
                    .map(|(carrier, _)| carrier)
                    .cloned()
                    .collect::<BTreeSet<_>>(),
                BTreeSet::from([independent.clone()])
            );

            assert_eq!(
                state.trusted_assumptions,
                BTreeSet::from([(independent.clone(), true)])
            );

            assert!(
                state
                    .witnesses
                    .values()
                    .all(|witness| witness.dependencies.is_empty())
            );
        }
    }

    #[test]
    fn invalidation_overlaps_parent_and_child_paths_but_preserves_siblings_and_snapshots() {
        let root = ExecutionPlace::argument(SymbolOrdinal::new(0));

        let left = root
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));

        let child = left
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));

        let right = root
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)));

        let changed = ExecutionCondition::Input(left);

        let captured = ExecutionCondition::Entry {
            condition: Arc::new(changed.clone()),
            captured: true,
        };

        let live = ExecutionCondition::Entry {
            condition: Arc::new(changed.clone()),
            captured: false,
        };

        let mut state = ExecutionState::default();

        for (place, ordinal) in [root, child, right].into_iter().zip(0u32..) {
            let dependency = ExecutionPlace::argument(SymbolOrdinal::new(ordinal + 10));
            let carrier = input(ordinal + 20);

            state
                .current
                .insert(dependency.clone(), ExecutionCondition::Input(place));

            state
                .witnesses
                .entry(carrier.clone())
                .or_default()
                .dependencies = BTreeSet::from([dependency]);

            state.carry_guarantee(carrier.clone(), carrier.clone());
            state.trusted_assumptions.insert((carrier, true));
        }

        state.assumptions.insert((changed.clone(), true));
        state.assumptions.insert((changed.clone().borrowed(), true));
        state.trusted_assumptions.insert((captured.clone(), true));
        state.trusted_assumptions.insert((live, true));
        state.invalidate_trusted(&changed);

        assert_eq!(
            state
                .witnesses
                .iter()
                .filter(|(_, witness)| !witness.conditions.is_empty())
                .map(|(carrier, _)| carrier)
                .cloned()
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([input(22)])
        );

        assert_eq!(
            state.trusted_assumptions,
            BTreeSet::from([(captured, true), (input(22), true)])
        );

        assert_eq!(state.assumptions, BTreeSet::from([(changed, true)]));

        assert_eq!(
            state
                .witnesses
                .values()
                .filter(|witness| !witness.dependencies.is_empty())
                .count(),
            1
        );
    }

    #[test]
    fn witness_lookup_preserves_component_overlap_and_entry_snapshots() {
        let root = ExecutionPlace::argument(SymbolOrdinal::new(0));

        let child = ExecutionCondition::Input(
            root.clone()
                .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0))),
        );

        let sibling = ExecutionCondition::Input(
            root.clone()
                .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1))),
        );

        let parent = ExecutionCondition::Input(root);
        let mut state = ExecutionState::default();

        for value in (1..=512).map(input) {
            state.carry_guarantee(value.clone(), value);
        }

        state.carry_guarantee(child.clone(), parent.clone());
        state.trusted_assumptions.insert((parent.clone(), true));

        assert!(state.is_witness(&child));
        assert!(!state.is_witness(&sibling));

        state.assumptions.insert((parent.clone(), true));

        assert!(!state.is_witness(&child));

        state.assumptions.clear();

        state.trusted_assumptions = BTreeSet::from([(
            ExecutionCondition::Entry {
                condition: Arc::new(parent.clone()),
                captured: true,
            },
            true,
        )]);

        assert!(!state.is_witness(&child));

        let live = ExecutionCondition::Entry {
            condition: Arc::new(parent),
            captured: false,
        };

        for witness in state.witnesses.values_mut() {
            witness.conditions.clear();
        }

        state.carry_guarantee(child.clone(), live.clone());
        state.trusted_assumptions = BTreeSet::from([(live, true)]);

        assert!(state.is_witness(&child));
    }
    #[test]
    fn retiring_a_carrier_revokes_its_unobserved_guarantee_but_keeps_moved_children() {
        let child = input(1);

        let owner = ExecutionCondition::Constructed(
            Arc::new(input(0)),
            [(
                bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(0)),
                child.clone(),
            )]
            .into(),
        );

        let owner_condition = input(20);
        let child_condition = input(21);
        let mut state = ExecutionState::default();

        state.carry_guarantee(owner.clone(), owner_condition.clone());
        state.carry_guarantee(child.clone(), child_condition.clone());

        state.trusted_assumptions.extend([
            (owner_condition.clone(), true),
            (child_condition.clone(), true),
        ]);

        state.retire_trusted_values([owner.clone()], &BTreeSet::from([child.clone()]));

        assert!(
            state
                .witnesses
                .get(&owner)
                .is_none_or(|witness| witness.conditions.is_empty())
        );

        assert!(state.is_witness(&child));
        assert!(!state.trusted_assumptions.contains(&(owner_condition, true)));
        assert!(state.trusted_assumptions.contains(&(child_condition, true)));
    }
}
