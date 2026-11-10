use std::collections::{BTreeMap, BTreeSet};

use super::state::ExecutionState;
use crate::ExecutionCondition;

impl ExecutionState {
    pub(super) fn invalidate_trusted(&mut self, value: &ExecutionCondition) {
        self.invalidate_trusted_values([value.clone()]);
    }

    pub(super) fn invalidate_trusted_values(
        &mut self,
        values: impl IntoIterator<Item = ExecutionCondition>,
    ) {
        self.retire_trusted_values(values, &BTreeSet::new());
    }

    pub(super) fn retire_trusted_values(
        &mut self,
        values: impl IntoIterator<Item = ExecutionCondition>,
        retained: &BTreeSet<ExecutionCondition>,
    ) {
        self.retire_values(
            values,
            &crate::execution_guarantees::ObservationSet::default(),
            Some(retained),
            &BTreeSet::new(),
            (bray_symbols::DependencyMutationKind::Retire, &BTreeSet::new()),
            None,
        );
    }

    fn retire_values(
        &mut self,
        values: impl IntoIterator<Item = ExecutionCondition>,
        retained_observations: &crate::execution_guarantees::ObservationSet,
        retained_snapshots: Option<&BTreeSet<ExecutionCondition>>,
        stable_predicates: &BTreeSet<bray_symbols::PredicateDefinitionSymbolId>,
        effect: (bray_symbols::DependencyMutationKind, &BTreeSet<crate::ExecutionPlace>),
        admitted: Option<bray_bound_tree::BoundExecutionSite>,
    ) {
        let (kind, places) = effect;

        let private = self.private_witness_inputs(admitted);
        let private_allocations = self.private_allocation_conditions(admitted, &private);
        let pending = values.into_iter().collect::<Vec<_>>();
        let retiring_snapshots = retained_snapshots.map(|_| pending.iter().cloned().collect::<BTreeSet<_>>());

        let retiring_observations = pending.iter().flat_map(|value| value.observations().cloned())
            .collect::<crate::execution_guarantees::ObservationSet>();

        // A spatial fact survives a contents change, but replacing its actual
        // carrier revokes it. Components reached through that carrier are distinct.
        let directly_changed = if stable_predicates.is_empty() {
            BTreeSet::new()
        } else {
            pending
                .iter()
                .map(|value| value.owner_identity().clone())
                .collect()
        };

        if pending.is_empty() && places.is_empty() {
            return;
        }

        self.refine_deferred_witnesses();

        let retained_owner = |value: &ExecutionCondition| retained_snapshots.map_or_else(
            || retained_observations.contains(value),
            |snapshots| crate::execution_guarantees::ObservationSet::retains_owner(snapshots, value),
        );

        let retained_data = |value: &ExecutionCondition| retained_snapshots.map_or_else(
            || retained_observations.contains(value),
            |snapshots| crate::execution_guarantees::ObservationSet::retains_contents(snapshots, value),
        );

        let invalidated = self.retired_owners(pending, kind, retained_snapshots.is_some(), &retained_owner);

        let invalidated_observations = invalidated.iter().cloned()
            .collect::<crate::execution_guarantees::ObservationSet>();

        let invalidated_owners = invalidated.iter()
            .filter_map(ExecutionCondition::ownership_identity)
            .collect::<BTreeSet<_>>();

        let retired_owner = |value: &ExecutionCondition| if retained_snapshots.is_some() {
            value.ownership_identity().is_some_and(|owner| invalidated_owners.contains(&owner))
        } else {
            invalidated_observations.contains(value)
        };

        let observes = |condition: &ExecutionCondition, selection: bool| {
            condition
                .observations_except(&retained_data)
                .any(|observed| invalidated_observations.overlaps(observed)
                    || selection && match observed {
                        ExecutionCondition::Joined(_, _, place) => place.overlaps_any(places),
                        ExecutionCondition::PostState(_, identity) => matches!(identity.as_ref(),
                            ExecutionCondition::Input(place) if place.overlaps_any(places)),
                        _ => false,
                    })
        };

        let borrowed_slot_changed = |condition: &ExecutionCondition| {
            condition.observations_except(&retained_data).any(|observed| {
                let ExecutionCondition::Borrowed(value) = observed else { return false; };

                if retained_data(value) { return false; }

                match value.owner_identity() {
                    ExecutionCondition::Input(place) | ExecutionCondition::Joined(_, _, place) => place.overlaps_any(places),
                    ExecutionCondition::PostState(_, identity) => matches!(identity.as_ref(),
                        ExecutionCondition::Input(place) if place.overlaps_any(places)),
                    _ => false,
                }
            })
        };
        let borrowed_changed = |condition: &ExecutionCondition| condition.observes_live_where(|value| {
            observes(value, false)
                || matches!((&retiring_snapshots, retained_snapshots), (Some(before), Some(after))
                    if retiring_observations.overlaps(value)
                        && !crate::execution_guarantees::ObservationSet::retains_contents_between(before, after, value))
        }, &retained_data) || borrowed_slot_changed(condition);

        let retained_contents = |condition: &ExecutionCondition| {
            kind.is_write()
                && !borrowed_slot_changed(condition)
                && condition.observes_only(|value| retained_observations.preserves(value))
                && condition
                    .observations()
                    .any(|value| retained_observations.preserves(value))
        };

        let stable = |condition: &ExecutionCondition| {
            let predicate = match condition {
                ExecutionCondition::Trusted(value) => value.as_ref(),
                value => value,
            };

            // Spatial facts describe the captured pointer value and its allocation, not
            // the slot that supplied that value. Contents writes neither free nor resize
            // allocations. A changed slot receives a new observation below its caller.
            matches!(predicate, ExecutionCondition::Predicate(symbol, _, _) if stable_predicates.contains(symbol))
                || retained_contents(condition)
        };

        // A surviving carrier keeps facts about immutable captured contents after
        // their former container ends. Borrowed observations and physical bounds
        // still expire. Writes also invalidate affected by-value observations.
        let revokes = |condition: &ExecutionCondition| {
            (observes(condition, false) || borrowed_changed(condition)) && !stable(condition)
        };

        let revokes_guarantee = |condition: &ExecutionCondition| {
            revokes(condition) && (kind.is_write() || borrowed_changed(condition))
        };

        let availability_expired = |condition: &ExecutionCondition| {
            !kind.is_write() && condition.observations().any(|observed| {
                matches!(observed, ExecutionCondition::InitializedSnapshot(_, _, owner)
                    if retired_owner(owner)
                        && !retained_owner(owner))
            })
        };

        self.assumptions
            .retain(|(condition, _)| !availability_expired(condition)
                && (retained_contents(condition) || !borrowed_changed(condition)));

        let mut retired_conditions = BTreeSet::new();

        self.witnesses.retain(|carrier, witness| {
            if kind.is_write() && private.contains(carrier) {
                return true;
            }

            let retired = retired_owner(carrier)
                && !retained_owner(carrier);

            witness.retain_conditions(|condition, guarantee| {
                let address_expired = !kind.is_write()
                    && guarantee.addresses.values().flatten().any(|owner| {
                        owner.value_in(&self.current).is_some_and(|value| {
                            value.observations().any(|observed| {
                                !matches!(
                                    observed,
                                    ExecutionCondition::Literal(_)
                                        | ExecutionCondition::Boolean(_)
                                        | ExecutionCondition::Variant(_)
                                ) && invalidated_observations.overlaps(observed)
                            })
                        })
                    });

                // Explicitly preserved components still belong to this live
                // carrier. Only replacement or retirement can end their owner.
                if (retired
                    && !retained_contents(condition)
                    && (directly_changed.contains(carrier.owner_identity()) || !stable(condition)))
                    || revokes_guarantee(condition)
                    || address_expired
                    || availability_expired(condition)
                {
                    condition.clone().assume(true, &mut retired_conditions);

                    false
                } else {
                    true
                }
            });

            if !kind.is_write()
                && retired
                && witness.conditions.is_empty()
                && (stable_predicates.is_empty()
                    || directly_changed.contains(carrier.owner_identity()))
            {
                if !witness.dependencies.is_empty() {
                    std::sync::Arc::make_mut(witness).dependencies.clear();
                }
            }

            !witness.conditions.is_empty() || !witness.dependencies.is_empty()
        });

        // Another live carrier may independently establish the same condition.
        self.exclude_live_witnesses(&mut retired_conditions);

        self.trusted_assumptions
            .retain(|fact| !revokes_guarantee(&fact.0) && !retired_conditions.contains(fact));

        self.allocation_owners.retain(|condition, owner| {
            kind.is_write() && private_allocations.contains(condition)
                || !revokes_guarantee(condition) && !invalidated.contains(owner)
        });

        self.retire_pending_conditions(&revokes, &retired_conditions);

        for entries in self.deferred_invocations.values_mut() {
            // Current branch selectors can outlive a slot's loop incarnation.
            // Frozen call-entry arguments remain owned by their captured frame.
            entries.retire_selection(|condition| observes(condition, true));
        }

        // A destroyed frame cannot participate in a later incarnation's ownership join.
        // An active await retains its entry until its completion transition consumes it.
        self.deferred_invocations.retain(|owner, value| {
            !invalidated.contains(owner)
                || value
                    .producers
                    .values()
                    .any(|candidate| !candidate.entry.pending_execution)
        });
    }

    fn retired_owners(
        &self,
        mut pending: Vec<ExecutionCondition>,
        kind: bray_symbols::DependencyMutationKind,
        select_descendants: bool,
        retained_owner: &impl Fn(&ExecutionCondition) -> bool,
    ) -> BTreeSet<ExecutionCondition> {
        if select_descendants {
            // A retained aggregate must not hide recorded children of its old
            // actual components, including fields with independent identities.
            let mut components = pending.clone();
            let mut owners = BTreeSet::new();

            while let Some(value) = components.pop() {
                if owners.insert(value.clone()) {
                    components.extend(value.owned_components().cloned());
                }
            }

            pending.extend(self.witnesses.keys().filter(|carrier| {
                owners.iter().any(|owner| carrier.is_owned_projection_of(owner))
            }).cloned());
        }

        let (dependents, input_dependents) = self.retirement_dependents(kind, retained_owner);

        let mut visited = BTreeSet::new();
        let mut invalidated = BTreeSet::new();

        while let Some(value) = pending.pop() {
            // A write changes storage observations, not an independently held constant.
            if kind.is_write() && matches!(value,
                ExecutionCondition::Literal(_) | ExecutionCondition::Boolean(_) | ExecutionCondition::Variant(_))
                || !select_descendants && retained_owner(&value)
                || !visited.insert(value.clone())
            {
                continue;
            }

            pending.extend(value.owned_components().cloned());

            if select_descendants && value.ownership_identity().is_none() {
                continue;
            }

            if value.owner_identity() != &value
                && (!select_descendants || value.ownership_identity().map(|(role, _)| role)
                    == value.owner_identity().ownership_identity().map(|(role, _)| role))
            {
                pending.push(value.owner_identity().clone());
            }

            if retained_owner(&value) {
                continue;
            }

            invalidated.insert(value.clone());

            if let ExecutionCondition::Input(place) = &value {
                for length in 0..place.projections.len() {
                    let mut prefix = place.clone();

                    prefix.projections = place.projections[..length].into();

                    if let Some(carriers) = input_dependents.get(&prefix) {
                        pending.extend(carriers.iter().cloned());
                    }
                }

                for (_, carriers) in input_dependents.range(place.clone()..)
                    .take_while(|(observed, _)| place.contains(observed))
                {
                    pending.extend(carriers.iter().cloned());
                }
            } else if let Some(carriers) = dependents.get(&value) {
                pending.extend(carriers.iter().cloned());
            }
        }

        invalidated
    }

    fn retirement_dependents(
        &self,
        kind: bray_symbols::DependencyMutationKind,
        retained_owner: &impl Fn(&ExecutionCondition) -> bool,
    ) -> (
        BTreeMap<ExecutionCondition, BTreeSet<ExecutionCondition>>,
        BTreeMap<crate::ExecutionPlace, BTreeSet<ExecutionCondition>>,
    ) {
        let mut dependents = BTreeMap::<ExecutionCondition, BTreeSet<ExecutionCondition>>::new();

        let mut input_dependents =
            BTreeMap::<crate::ExecutionPlace, BTreeSet<ExecutionCondition>>::new();

        // Index current dependencies once, then visit only carriers reached by the worklist.
        for (carrier, witness) in &self.witnesses {
            // Captured inputs belong to a frame. Ordinary projection identities can
            // survive extraction, so their former containing slot is not a lifetime owner.
            // A retained captured owner also severs its former frame's ownership edge.
            for owner in carrier
                .ownership_ancestors()
                .take_while(|value| !retained_owner(value))
                .filter_map(|value| match value {
                    ExecutionCondition::CapturedInput { owner, .. } => Some(owner.owner_identity()),
                    _ => None,
                })
            {
                if let ExecutionCondition::Input(place) = owner {
                    input_dependents
                        .entry(place.clone())
                        .or_default()
                        .insert(carrier.clone());
                } else {
                    dependents
                        .entry(owner.clone())
                        .or_default()
                        .insert(carrier.clone());
                }
            }

            if kind.is_write() {
                continue;
            }

            for dependency in witness
                .dependencies
                .iter()
                .filter_map(|place| place.value_in(&self.current))
            {
                for observed in dependency.observations() {
                    if let ExecutionCondition::Input(place) = observed {
                        input_dependents
                            .entry(place.clone())
                            .or_default()
                            .insert(carrier.clone());
                    } else {
                        let mut owner = observed;

                        loop {
                            dependents
                                .entry(owner.owner_identity().clone())
                                .or_default()
                                .insert(carrier.clone());

                            let ExecutionCondition::Projection(_, parent) = owner else {
                                break;
                            };

                            owner = parent;
                        }
                    }
                }
            }
        }

        (dependents, input_dependents)
    }

    fn retire_pending_conditions(
        &mut self,
        revokes: &impl Fn(&ExecutionCondition) -> bool,
        retired: &BTreeSet<(ExecutionCondition, bool)>,
    ) {
        for entry in self.pending_entries_mut(&|entry| {
            entry
                .assumptions
                .iter()
                .chain(&entry.trusted_assumptions)
                .any(|fact| revokes(&fact.0) || retired.contains(fact))
                || entry.copy_sources.values().any(revokes)
        }) {
            entry.copy_sources.retain(|_, value| !revokes(value));

            entry
                .assumptions
                .retain(|fact| !revokes(&fact.0) && !retired.contains(fact));

            entry
                .trusted_assumptions
                .retain(|fact| !revokes(&fact.0) && !retired.contains(fact));
        }
    }

    pub(super) fn invalidate_referents(
        &mut self,
        values: &[ExecutionCondition],
        address_slots: &BTreeSet<ExecutionCondition>,
        stable_predicates: &BTreeSet<bray_symbols::PredicateDefinitionSymbolId>,
        preserved: &BTreeSet<ExecutionCondition>,
        stable_values: &crate::execution_guarantees::ObservationSet,
        admitted: Option<bray_bound_tree::BoundExecutionSite>,
    ) {
        if values.is_empty() {
            return;
        }

        let mut referents = crate::execution_guarantees::ObservationSet::default();
        let private = self.private_witness_inputs(admitted);
        let private_allocations = self.private_allocation_conditions(admitted, &private);

        for value in values {
            referents.insert(value.clone());
        }

        let stable_contents = |value: &ExecutionCondition| {
            // Stable address bits do not certify unchanged pointee contents.
            stable_values.preserves(value) && (matches!(value, ExecutionCondition::Borrowed(_))
                || !address_slots.contains(value.owner_identity()))
        };

        let revokes = |condition: &ExecutionCondition| {
            let predicate = match condition {
                ExecutionCondition::Trusted(predicate) => predicate.as_ref(),
                condition => condition,
            };

            !preserved.contains(condition)
                && !condition.observes_only(&stable_contents)
                && !matches!(predicate, ExecutionCondition::Predicate(symbol, _, _) if stable_predicates.contains(symbol))
                && condition
                    .observations_except(|observation| {
                        // A borrow of an address slot observes the descriptor itself.
                        // Pointee mutation still revokes claims on the unborrowed address.
                        matches!(observation, ExecutionCondition::Borrowed(value)
                        if address_slots.contains(value.owner_identity()))
                    })
                    .any(|observation| referents.overlaps(observation))
        };

        // A referent write changes observations through borrows and callable or
        // predicate contracts. A by-value guard, including a nullable tag, still
        // describes the same live owner. Physical replacement retires its observations separately.
        let revokes_observation = |condition: &ExecutionCondition| {
            if preserved.contains(condition) || condition.observes_only(&stable_contents) {
                return false;
            }

            condition.observations_except(|observation| {
                preserved.contains(observation)
                    || matches!(observation, ExecutionCondition::Predicate(symbol, _, _) if stable_predicates.contains(symbol))
            }).any(|observation| {
                matches!(
                    observation,
                    ExecutionCondition::Borrowed(_)
                        | ExecutionCondition::Predicate(_, _, _)
                        | ExecutionCondition::Call(_, _, _, _, _)
                ) && revokes(observation)
            })
        };

        self.assumptions
            .retain(|(condition, _)| !revokes_observation(condition));

        self.trusted_assumptions
            .retain(|(condition, _)| !revokes_observation(condition));

        self.witnesses.retain(|carrier, witness| {
            if private.contains(carrier) {
                return true;
            }

            witness.retain_conditions(|condition, _| !revokes_observation(condition));

            !witness.conditions.is_empty() || !witness.dependencies.is_empty()
        });

        self.allocation_owners
            .retain(|condition, _| private_allocations.contains(condition) || !revokes_observation(condition));

        for entry in self.pending_entries_mut(&|entry| {
            entry
                .assumptions
                .iter()
                .any(|(condition, _)| revokes_observation(condition))
                || entry
                    .trusted_assumptions
                    .iter()
                    .any(|(condition, _)| revokes_observation(condition))
        }) {
            entry
                .assumptions
                .retain(|(condition, _)| !revokes_observation(condition));

            entry
                .trusted_assumptions
                .retain(|(condition, _)| !revokes_observation(condition));
        }
    }

    pub(super) fn invalidate_contents(
        &mut self,
        places: &BTreeSet<crate::ExecutionPlace>,
        values: impl IntoIterator<Item = ExecutionCondition>,
        stable_predicates: &BTreeSet<bray_symbols::PredicateDefinitionSymbolId>,
        stable_values: &crate::execution_guarantees::ObservationSet,
        admitted: Option<bray_bound_tree::BoundExecutionSite>,
    ) {
        // Writes revoke observed contents. A live carrier's lifetime dependencies
        // are retired only when the physical owner or capability ends.
        self.retire_values(
            values,
            stable_values,
            None,
            stable_predicates,
            (bray_symbols::DependencyMutationKind::Write, places),
            admitted,
        );
    }

    pub(super) fn exclude_live_witnesses(
        &self,
        retired: &mut BTreeSet<(ExecutionCondition, bool)>,
    ) {
        for witness in self.witnesses.values().filter(|witness| witness.available) {
            for condition in witness.conditions.keys() {
                condition
                    .clone()
                    .assume_with(true, &mut |condition, holds| {
                        retired.remove(&(condition, holds));
                    });
            }
        }
    }

    fn retire_witness_roots(
        &mut self,
        roots: &BTreeSet<crate::execution_guarantees::ExecutionInput>,
    ) {
        let mut retired = BTreeSet::new();

        self.witnesses.retain(|carrier, witness| {
            // A scoped view expires without changing the independently held
            // referent. Root-bound address clauses expire at this same boundary.
            if matches!(carrier.ownership_identity(), Some((crate::execution_guarantees::ExecutionObservationKind::Reference, _)))
                && witness.dependencies.iter().any(|owner| roots.contains(&owner.root))
            {
                for condition in witness.conditions.keys() {
                    condition.clone().assume(true, &mut retired);
                }

                return false;
            }

            witness.retain_conditions(|condition, guarantee| {
                if guarantee
                    .addresses
                    .values()
                    .flatten()
                    .any(|owner| roots.contains(&owner.root))
                {
                    condition.clone().assume(true, &mut retired);

                    false
                } else {
                    true
                }
            });

            !witness.conditions.is_empty() || !witness.dependencies.is_empty()
        });

        self.exclude_live_witnesses(&mut retired);

        self.trusted_assumptions
            .retain(|fact| !retired.contains(fact));

        self.allocation_owners
            .retain(|condition, _| !retired.contains(&(condition.clone(), true)));

        for entry in self.pending_entries_mut(&|entry| {
            entry
                .assumptions
                .iter()
                .chain(&entry.trusted_assumptions)
                .any(|fact| retired.contains(fact))
        }) {
            entry.assumptions.retain(|fact| !retired.contains(fact));

            entry
                .trusted_assumptions
                .retain(|fact| !retired.contains(fact));
        }
    }

    pub(super) fn invalidate_trusted_roots(
        &mut self,
        roots: &BTreeSet<crate::execution_guarantees::ExecutionInput>,
    ) {
        self.assumptions.retain(|(condition, _)| !condition.observes_live_where(|value| {
            value.observations().any(|observed| matches!(observed, ExecutionCondition::Input(place) if roots.contains(&place.root)))
        }, |_| false));

        self.retire_witness_roots(roots);

        let dependent = self
            .witnesses
            .iter()
            .filter(|(_, witness)| {
                witness
                    .dependencies
                    .iter()
                    .any(|owner| roots.contains(&owner.root))
            })
            .map(|(carrier, _)| carrier.clone())
            .collect::<Vec<_>>();

        self.invalidate_trusted_values(dependent);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use super::ExecutionState;
    use crate::ExecutionCondition;
    use crate::ExecutionPlace;
    use bray_bound_tree::StorageProjection;
    use bray_symbols::{ConstantProjectionKind, SymbolOrdinal};

    #[test]
    fn retirement_revokes_owners_beyond_the_proof_budget() {
        for count in [1, ExecutionCondition::WORK_LIMIT + 1] {
            let owners = (0..count).map(|index| {
                ExecutionCondition::Input(ExecutionPlace::argument(
                    SymbolOrdinal::new(u32::try_from(index).expect("test owner index fits ordinal")),
                ))
            }).collect::<BTreeSet<_>>();

            let first = owners.first().expect("test has an owner").clone();
            let mut state = ExecutionState::default();

            state.carry_guarantee(first.clone(), first.clone());
            state.trusted_assumptions.insert((first.clone(), true));
            state.retire_trusted_values(owners, &BTreeSet::new());

            assert!(!state.owns_witness(&first), "retirement retained authority with {count} invalidated owners");
        }
    }

    #[test]
    fn retained_parent_identity_does_not_republish_changed_borrowed_data() {
        let root = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let sibling = StorageProjection::TupleElement(SymbolOrdinal::new(1));

        let old = ExecutionCondition::owned_value(root.clone(), ExecutionCondition::constructed(root.clone(), [
            (field, ExecutionCondition::Boolean(true)),
        ].into()));

        let changed = ExecutionCondition::owned_value(root.clone(), ExecutionCondition::constructed(root.clone(), [
            (field, ExecutionCondition::Boolean(false)), (sibling, ExecutionCondition::Boolean(true)),
        ].into()));

        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");

        let condition = ExecutionCondition::Trusted(std::sync::Arc::new(ExecutionCondition::predicate(
            symbol.into(), crate::test_support::empty_substitution(symbol.into()),
            vec![ExecutionCondition::reference_value(root.clone().project(field).borrowed(), old.clone().project(field))],
        )));

        for (returned, preserved) in [
            (old.clone(), true),
            (old.with_storage_component(&[field], ExecutionCondition::scalar_observation(
                ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(2))),
                old.clone().project(field),
            )), true),
            (ExecutionCondition::owned_value(root.clone(), ExecutionCondition::Unknown), false),
            (changed, false),
        ] {
            let mut state = ExecutionState::default();

            state.carry_guarantee(root.clone(), condition.clone());
            condition.clone().assume(true, &mut state.trusted_assumptions);
            state.retire_trusted_values([old.clone()], &BTreeSet::from([returned]));

            assert_eq!(condition.prove_trusted(&state.trusted_assumptions, &state.assumptions) == Some(true), preserved);
        }
    }

    #[test]
    fn copying_or_retiring_a_loan_does_not_own_or_retire_its_source() {
        let owner = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));

        for (condition, retained) in [
            (owner.clone(), BTreeSet::new()),
            (ExecutionCondition::Trusted(std::sync::Arc::new(owner.clone().borrowed())), BTreeSet::from([owner.clone()])),
        ] {
            let mut state = ExecutionState::default();

            state.carry_guarantee(owner.clone(), condition.clone());
            state.trusted_assumptions.insert((condition.clone(), true));

            let copied = ExecutionCondition::scalar_observation(owner.clone(), owner.clone());
            let loan = owner.clone().borrowed();

            state.carry_guarantee(loan.clone(), condition.clone());
            assert!(state.owns_witness(&owner));
            assert!(!state.owns_witness(&copied));
            assert!(!state.owns_witness(&loan));

            state.retire_trusted_values([copied, loan.clone()], &retained);

            assert!(state.owns_witness(&owner), "ending a view cannot retire its independently live source");
            assert!(state.trusted_assumptions.contains(&(condition.clone(), true)));

            let scope = ExecutionPlace::argument(SymbolOrdinal::new(1));
            state.current.insert(scope.clone(), ExecutionCondition::Input(scope.clone()));
            state.carry_guarantee(loan.clone(), condition.clone());
            std::sync::Arc::make_mut(state.witnesses.get_mut(&loan).expect("the scoped view has its own evidence"))
                .dependencies.insert(scope.clone());
            let mut only_view = ExecutionState::default();
            only_view.current = state.current.clone();
            only_view.witnesses.insert(loan.clone(), state.witnesses[&loan].clone());
            only_view.trusted_assumptions.insert((condition.clone(), true));
            only_view.invalidate_trusted_roots(&BTreeSet::from([scope.root.clone()]));
            assert!(!only_view.trusted_assumptions.contains(&(condition.clone(), true)), "an expired sole view cannot retain evidence");

            state.invalidate_trusted_roots(&BTreeSet::from([scope.root]));

            assert!(!state.witnesses.contains_key(&loan));
            assert!(state.owns_witness(&owner), "view expiry preserves independent authority");
            assert!(state.trusted_assumptions.contains(&(condition.clone(), true)));
            state.invalidate_trusted(&owner);
            assert!(!state.trusted_assumptions.contains(&(condition, true)), "retiring the actual owner revokes its evidence");
        }
    }

    #[test]
    fn constructor_fields_retain_actual_owned_temporaries() {
        use bray_bound_tree::{BoundErrorExpression, BoundExpression, BoundUnitId};
        use crate::test_support::{error_type, expression_unit, push_expression};

        let unit = BoundUnitId::new(41);

        let (_, expressions) = expression_unit(unit, |tree, origin| (0..2).map(|_| {
            push_expression(tree, BoundExpression::Error(BoundErrorExpression::new(origin, error_type())))
        }).collect::<Vec<_>>());

        let result = ExecutionCondition::Expression(expressions[0]);
        let child = ExecutionCondition::Expression(expressions[1]);
        let field = StorageProjection::ElementFromStart(SymbolOrdinal::new(0));
        let mut state = ExecutionState::default();

        state.carry_guarantee(child.clone(), child.clone());
        state.trusted_assumptions.insert((child.clone(), true));

        let aggregate = ExecutionCondition::constructed(result, [(field, child)].into());

        assert!(state.owns_witness(&aggregate));
        assert!(state.owns_witness(&aggregate.project(field)));
    }

    #[test]
    fn retained_aggregate_identity_does_not_retain_its_replaced_child() {
        let root = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
        let field = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
        let left = root.clone().component(field);
        let right = root.clone().component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)));
        let replacement = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
        let mut state = ExecutionState::default();

        for value in [&root, &left, &right] {
            state.carry_guarantee(value.clone(), value.clone());
            state.trusted_assumptions.insert((value.clone(), true));
        }

        let mut unchanged = state.clone();

        unchanged.retire_trusted_values([root.clone()], &BTreeSet::from([root.clone()]));

        assert_eq!(unchanged.witnesses, state.witnesses);
        assert_eq!(unchanged.trusted_assumptions, state.trusted_assumptions);

        let retained = ExecutionCondition::owned_value(root.clone(), ExecutionCondition::constructed(root.clone(), [(
            StorageProjection::TupleElement(SymbolOrdinal::new(0)), replacement,
        )].into()));

        state.retire_trusted_values([root.clone()], &BTreeSet::from([retained]));

        assert_eq!(state.witnesses.keys().cloned().collect::<BTreeSet<_>>(), BTreeSet::from([root.clone(), right.clone()]));
        assert_eq!(state.trusted_assumptions, BTreeSet::from([(root, true), (right, true)]));
        assert!(!state.witnesses.contains_key(&left));
    }

    #[test]
    fn retained_owner_and_retained_projected_contents_are_distinct() {
        let root = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
        let field = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
        let child = root.clone().component(field);
        let before = ExecutionCondition::owned_value(child.clone(), ExecutionCondition::Boolean(true));
        let after = ExecutionCondition::owned_value(child, ExecutionCondition::Boolean(false));

        let retained = BTreeSet::from([ExecutionCondition::constructed(root.clone(), [(
            StorageProjection::TupleElement(SymbolOrdinal::new(0)), after,
        )].into())]);

        assert!(crate::execution_guarantees::ObservationSet::retains_owner(&retained, &root));
        assert!(crate::execution_guarantees::ObservationSet::retains_owner(&retained, &before));
        assert!(!crate::execution_guarantees::ObservationSet::retains_contents(&retained, &before));
    }
}
