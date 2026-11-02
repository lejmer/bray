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
            retained,
            &BTreeSet::new(),
            bray_symbols::DependencyMutationKind::Retire,
        );
    }

    fn retire_values(
        &mut self,
        values: impl IntoIterator<Item = ExecutionCondition>,
        retained: &BTreeSet<ExecutionCondition>,
        stable_predicates: &BTreeSet<bray_symbols::PredicateDefinitionSymbolId>,
        kind: bray_symbols::DependencyMutationKind,
    ) {
        let mut pending = values.into_iter().collect::<Vec<_>>();
        let mut directly_changed = crate::execution_guarantees::ObservationSet::default();

        if !stable_predicates.is_empty() {
            for value in &pending {
                for component in value.owned_values() {
                    directly_changed.insert(component.clone());
                }
            }
        }

        if pending.is_empty() {
            return;
        }

        self.refine_deferred_witnesses();

        let mut dependents = BTreeMap::<ExecutionCondition, BTreeSet<ExecutionCondition>>::new();

        let mut input_dependents =
            BTreeMap::<crate::ExecutionPlace, BTreeSet<ExecutionCondition>>::new();

        // Index current dependencies once, then visit only carriers reached by the worklist.
        for (carrier, witness) in &self.witnesses {
            // Captured inputs belong to a frame. Ordinary projection identities can
            // survive extraction, so their former containing slot is not a lifetime owner.
            for owner in carrier
                .ownership_ancestors()
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

        let mut retained_observations = crate::execution_guarantees::ObservationSet::default();

        for value in retained {
            retained_observations.insert(value.clone());
        }

        let mut invalidated = BTreeSet::new();
        let mut invalidated_observations = crate::execution_guarantees::ObservationSet::default();

        while let Some(value) = pending.pop() {
            // A write changes a storage observation, never the meaning of a constant
            // also held by an unrelated slot. Borrowed scalars retain their place identity.
            if kind.is_write()
                && matches!(
                    value,
                    ExecutionCondition::Literal(_)
                        | ExecutionCondition::Boolean(_)
                        | ExecutionCondition::Variant(_)
                )
            {
                continue;
            }

            if retained_observations.contains(&value) || !invalidated.insert(value.clone()) {
                continue;
            }

            pending.extend(value.owned_components().cloned());

            if value.owner_identity() != &value {
                pending.push(value.owner_identity().clone());
            }

            invalidated_observations.insert(value.clone());

            if let ExecutionCondition::Input(place) = &value {
                for length in 0..place.projections.len() {
                    let mut prefix = place.clone();

                    prefix.projections = place.projections[..length].into();

                    if let Some(carriers) = input_dependents.get(&prefix) {
                        pending.extend(carriers.iter().cloned());
                    }
                }

                for (_, carriers) in input_dependents
                    .range(place.clone()..)
                    .take_while(|(observed, _)| place.contains(observed))
                {
                    pending.extend(carriers.iter().cloned());
                }
            } else if let Some(carriers) = dependents.get(&value) {
                pending.extend(carriers.iter().cloned());
            }
        }

        let observes = |condition: &ExecutionCondition| {
            condition
                .observations_except(|value| retained_observations.contains(value))
                .any(|observed| invalidated_observations.overlaps(observed))
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
        };

        // A surviving carrier keeps facts about immutable captured contents after
        // their former container ends. Borrowed observations and physical bounds
        // still expire; writes also invalidate affected by-value observations.
        let revokes = |condition: &ExecutionCondition| observes(condition) && !stable(condition);

        let revokes_guarantee = |condition: &ExecutionCondition| {
            revokes(condition) && (kind.is_write() || condition.observes_live_where(observes))
        };

        self.assumptions
            .retain(|(condition, _)| !condition.observes_live_where(observes));

        let mut retired_conditions = BTreeSet::new();

        self.witnesses.retain(|carrier, witness| {
            let retired = invalidated_observations.contains(carrier)
                && !retained_observations.contains(carrier);

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

                if (retired && (directly_changed.contains(carrier) || !stable(condition)))
                    || revokes_guarantee(condition)
                    || address_expired
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
                && (stable_predicates.is_empty() || directly_changed.contains(carrier))
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
            !revokes_guarantee(condition) && !invalidated.contains(owner)
        });

        self.retire_pending_conditions(&revokes, &retired_conditions);

        for entries in self.deferred_invocations.values_mut() {
            entries.retire_selection(observes);
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
        stable_predicates: &BTreeSet<bray_symbols::PredicateDefinitionSymbolId>,
        preserved: &BTreeSet<ExecutionCondition>,
    ) {
        if values.is_empty() {
            return;
        }

        let revokes = |condition: &ExecutionCondition| {
            let predicate = match condition {
                ExecutionCondition::Trusted(predicate) => predicate.as_ref(),
                condition => condition,
            };

            !preserved.contains(condition)
                && !matches!(predicate, ExecutionCondition::Predicate(symbol, _, _) if stable_predicates.contains(symbol))
                && values.iter().any(|value| condition.observes(value))
        };

        let revokes_observation = |condition: &ExecutionCondition| {
            condition.observations().any(|observation| {
                matches!(
                    observation,
                    ExecutionCondition::Borrowed(_) | ExecutionCondition::Predicate(_, _, _)
                ) && revokes(observation)
            })
        };

        self.assumptions
            .retain(|(condition, _)| !revokes_observation(condition));

        self.trusted_assumptions
            .retain(|(condition, _)| !revokes(condition));

        self.witnesses.retain(|_, witness| {
            witness.retain_conditions(|condition, _| !revokes(condition));

            !witness.conditions.is_empty() || !witness.dependencies.is_empty()
        });

        self.allocation_owners
            .retain(|condition, _| !revokes(condition));

        for entry in self.pending_entries_mut(&|entry| {
            entry
                .assumptions
                .iter()
                .any(|(condition, _)| revokes_observation(condition))
                || entry
                    .trusted_assumptions
                    .iter()
                    .any(|(condition, _)| revokes(condition))
        }) {
            entry
                .assumptions
                .retain(|(condition, _)| !revokes_observation(condition));

            entry
                .trusted_assumptions
                .retain(|(condition, _)| !revokes(condition));
        }
    }

    pub(super) fn invalidate_contents(
        &mut self,
        places: &BTreeSet<crate::ExecutionPlace>,
        values: impl IntoIterator<Item = ExecutionCondition>,
        stable_predicates: &BTreeSet<bray_symbols::PredicateDefinitionSymbolId>,
    ) {
        self.assumptions.retain(|(condition, _)| {
            !condition.observes_live_where(|value| {
                value.observations().any(|observed| {
                    matches!(observed,
                ExecutionCondition::Input(place) if place.overlaps_any(places))
                })
            })
        });

        // Writes revoke observed contents. A live carrier's lifetime dependencies
        // are retired only when the physical owner or capability ends.
        self.retire_values(
            values,
            &BTreeSet::new(),
            stable_predicates,
            bray_symbols::DependencyMutationKind::Write,
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

    fn retire_address_roots(
        &mut self,
        roots: &BTreeSet<crate::execution_guarantees::ExecutionInput>,
    ) {
        let mut retired = BTreeSet::new();

        self.witnesses.retain(|_, witness| {
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
        }));

        self.retire_address_roots(roots);

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
