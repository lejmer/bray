use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;

use super::state::ExecutionState;
use crate::ExecutionCondition;
use crate::execution_guarantees::{RetainedSnapshots, memoize_condition, memoize_observations};

/// Lazy answers share one condition lookup within an immutable retirement context.
#[derive(Default)]
struct RetirementQueries {
    borrowed: std::sync::OnceLock<bool>,
    contents: std::sync::OnceLock<bool>,
    revoked: std::sync::OnceLock<bool>,
}

impl ExecutionState {
    pub(in crate::analysis) fn retire_captured_storage(&mut self,
        retirements: &[(bray_bound_tree::StorageTarget, bray_bound_tree::StorageValueRetirement)],
    ) {
        use bray_bound_tree::StorageValueRetirement;

        if retirements.iter().any(|(_, scope)| *scope != StorageValueRetirement::Backing) {
            self.update_stored_values(ExecutionCondition::retiring_storage(retirements));
        }

        if retirements.iter().any(|(_, scope)| *scope == StorageValueRetirement::Backing) {
            self.retire_witness_dependencies(&|place| {
                let crate::execution_guarantees::ExecutionInput::Pointee { target, .. } = &place.root else { return false; };

                retirements.iter().any(|(expired, scope)| scope.ends_backing(expired, target))
            });
        }
    }

    #[cfg(test)]
    pub(super) fn invalidate_trusted(&mut self, value: &ExecutionCondition) {
        self.invalidate_trusted_values([value.clone()]);
    }

    pub(super) fn invalidate_trusted_values(
        &mut self,
        values: impl IntoIterator<Item = ExecutionCondition>,
    ) {
        self.retire_trusted_values(values, RetainedSnapshots::default());
    }

    pub(super) fn retire_trusted_values(
        &mut self,
        values: impl IntoIterator<Item = ExecutionCondition>,
        retained: impl Into<RetainedSnapshots>,
    ) {
        self.retire_values(
            values,
            &crate::execution_guarantees::ObservationSet::default(),
            Some(retained.into()),
            &BTreeSet::new(),
            (bray_symbols::DependencyMutationKind::Retire, &BTreeSet::new()),
        );
    }

    fn retire_values(
        &mut self,
        values: impl IntoIterator<Item = ExecutionCondition>,
        retained_observations: &crate::execution_guarantees::ObservationSet,
        retained_snapshots: Option<RetainedSnapshots>,
        stable_predicates: &BTreeSet<bray_symbols::PredicateDefinitionSymbolId>,
        effect: (bray_symbols::DependencyMutationKind, &BTreeSet<crate::ExecutionPlace>),
    ) {
        let (kind, places) = effect;

        let pending = values.into_iter().collect::<Vec<_>>();

        if pending.is_empty() && places.is_empty() {
            return;
        }

        let retiring_snapshots = retained_snapshots.as_ref().map(|_| pending.iter().cloned().collect::<RetainedSnapshots>());

        let changed_contents = retiring_snapshots.as_ref().zip(retained_snapshots.as_ref()).map(|(before, after)| {
            let observations = pending.iter().flat_map(|value| value.observations().cloned())
                .collect::<crate::execution_guarantees::ObservationSet>();

            memoize_condition(move |value| observations.overlaps(value) && !before.retains_contents_between(after, value))
        });

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

        self.refine_deferred_witnesses();

        let retained_owner = |value: &ExecutionCondition| retained_snapshots.as_ref().map_or_else(
            || retained_observations.contains(value),
            |snapshots| snapshots.retains_owner(value),
        );

        let retained_data = |value: &ExecutionCondition| retained_snapshots.as_ref().map_or_else(
            || retained_observations.preserves(value),
            |snapshots| snapshots.retains_contents(value),
        );

        let invalidated = self.retired_owners(pending, kind, retained_snapshots.is_some(), &retained_owner);

        let invalidated_observations = invalidated.iter().cloned()
            .collect::<crate::execution_guarantees::ObservationSet>();

        let invalidated_owners = invalidated.iter()
            .filter_map(ExecutionCondition::ownership_identity)
            .collect::<BTreeSet<_>>();

        // Address support follows a retired place. Its current contents can
        // hold data from a separately retired owner.
        let retired_places = if kind.is_write() {
            BTreeSet::new()
        } else {
            invalidated_owners.iter().filter_map(|(_, owner)| match owner.kind() {
                crate::ExecutionConditionKind::Input(place) => Some(place),
                _ => None,
            }).collect()
        };

        let retired_owner = |value: &ExecutionCondition| if retained_snapshots.is_some() {
            value.ownership_identity().is_some_and(|owner| invalidated_owners.contains(&owner))
        } else {
            invalidated_observations.contains(value)
        };

        let observes = memoize_observations(|observed| invalidated_observations.overlaps(observed), &retained_data);

        // A stored copy can retain data after its source stops selecting a frame.
        let observes_changed_selector = memoize_observations(|observed| {
            observed.selector_place().is_some_and(|place| place.overlaps_any(places))
        }, |_| false);

        let observes_selection = |condition: &ExecutionCondition| {
            observes(condition) || !places.is_empty() && observes_changed_selector(condition)
        };

        let borrowed_slot_changed = memoize_observations(|observed| {
            let crate::ExecutionConditionKind::Borrowed(value) = observed.kind() else {
                return false;
            };

            if retained_data(value) {
                return false;
            }

            let owner = value.owner_identity();

            match owner.kind() {
                crate::ExecutionConditionKind::Input(place) => place.overlaps_any(places),
                _ => owner.selector_place().is_some_and(|place| place.overlaps_any(places)),
            }
        }, &retained_data);

        let preserved_observation = memoize_observations(|value| retained_observations.preserves(value), |_| false);
        let queries = memoize_condition(|_| std::sync::Arc::new(RetirementQueries::default()));

        let borrowed_changed = |condition: &ExecutionCondition, memo: &RetirementQueries| *memo.borrowed.get_or_init(|| condition.observes_live_where(|value| {
            observes(value)
                || changed_contents.as_ref().is_some_and(|changed| changed(value))
        }, &retained_data) || borrowed_slot_changed(condition));

        let retained_contents = |condition: &ExecutionCondition, memo: &RetirementQueries| *memo.contents.get_or_init(|| {
            kind.is_write()
                && !borrowed_slot_changed(condition)
                && condition.observes_only(|value| retained_observations.preserves(value))
                && preserved_observation(condition)
        });

        let stable = |condition: &ExecutionCondition, memo: &RetirementQueries| {
            let predicate = match condition.kind() {
                crate::ExecutionConditionKind::Trusted(value) => value.as_ref(),
                _ => condition,
            };

            // Spatial facts describe the captured pointer value and its allocation, not
            // the slot that supplied that value. Contents writes neither free nor resize
            // allocations. A changed slot receives a new observation below its caller.
            matches!(predicate.kind(), crate::ExecutionConditionKind::Predicate(symbol, _, _) if stable_predicates.contains(symbol))
                || retained_contents(condition, memo)
        };

        // A surviving carrier keeps facts about immutable captured contents after
        // their former container ends. Borrowed observations and physical bounds
        // still expire. Writes also invalidate affected by-value observations.
        let revokes = |condition: &ExecutionCondition, memo: &RetirementQueries| *memo.revoked.get_or_init(|| {
            (observes(condition) || borrowed_changed(condition, memo)) && !stable(condition, memo)
        });

        let revokes_guarantee = |condition: &ExecutionCondition, memo: &RetirementQueries| {
            revokes(condition, memo) && (kind.is_write() || borrowed_changed(condition, memo))
        };

        let availability_expired = memoize_observations(|observed| {
            matches!(observed.kind(), crate::ExecutionConditionKind::InitializedSnapshot(_, _, owner)
                if retired_owner(owner) && !retained_owner(owner))
        }, |_| kind.is_write());

        retain_ordinary_facts(&mut self.assumptions, &self.proofs, &availability_expired, |condition| {
            let memo = queries(condition);

            retained_contents(condition, &memo) || !borrowed_changed(condition, &memo)
        });

        let mut retired_conditions = BTreeSet::new();

        self.witnesses.retain(|carrier, witness| {
            let retired = retired_owner(carrier)
                && !retained_owner(carrier);

            witness.retain_conditions(|condition, guarantee| {
                let memo = &queries(condition);

                let address_expired = !retired_places.is_empty()
                    && guarantee.addresses.values().flatten()
                        .any(|owner| retired_places.iter().any(|expired| expired.contains(owner)));

                // Explicitly preserved components still belong to this live
                // carrier. Only replacement or retirement can end their owner.
                let loses_owner = retired
                    && !retained_contents(condition, memo)
                    && (directly_changed.contains(carrier.owner_identity()) || !stable(condition, memo));

                if loses_owner || revokes_guarantee(condition, memo) || address_expired || availability_expired(condition) {
                    if std::env::var_os("BRA_694_TRACE_FRAME").is_some()
                        && matches!(carrier.kind(), crate::ExecutionConditionKind::Expression(expression)
                            if expression.unit().raw() == 18981 && expression.ordinal() == 11)
                    {
                        eprintln!("frame carrier retired kind={kind:?} owner={loses_owner} observations={} availability={} retained_owner={} retained_data={}",
                            revokes_guarantee(condition, memo), availability_expired(condition), retained_owner(carrier), retained_data(carrier));
                    }

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
            .retain(|fact| !revokes_guarantee(&fact.0, &queries(&fact.0)) && !retired_conditions.contains(fact));

        self.allocation_owners.retain(|condition, owner| {
            !revokes_guarantee(condition, &queries(condition)) && !invalidated.contains(owner)
        });

        self.retire_entry_context(&|condition| revokes(condition, &queries(condition)),
            &retired_conditions, &|owner, _| retired_owner(owner) && !retained_owner(owner));

        // Another iteration can already own a different frame from this evaluation
        // site. Retire the actual inactive frame whose owner the change reached.
        let retired_frames = invalidated.iter()
            .filter(|value| !value.is_reference())
            .filter_map(ExecutionCondition::deferred_value)
            .filter(|frame| frame.producers.values().all(|candidate| candidate.entry.pending_execution))
            .map(Arc::as_ptr)
            .collect::<HashSet<_>>();

        self.update_stored_values(ExecutionCondition::mapping_frames(
            |value| value.deferred_value().is_none_or(|frame| !retired_frames.contains(&Arc::as_ptr(frame))),
            |_, frame| Some(frame.retire_selection(&observes_selection)),
        ));

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

        let (dependents, input_dependents) = if kind.is_write() { Default::default() } else { self.retirement_dependents() };

        let mut visited = BTreeSet::new();
        let mut invalidated = BTreeSet::new();

        while let Some(value) = pending.pop() {
            // A write changes storage observations, not an independently held constant.
            if kind.is_write() && matches!(value.kind(),
                crate::ExecutionConditionKind::Literal(_) | crate::ExecutionConditionKind::Boolean(_) | crate::ExecutionConditionKind::Variant(_))
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

            if let crate::ExecutionConditionKind::Input(place) = value.kind() {
                if input_dependents.is_empty() { continue; }

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
    ) -> (
        BTreeMap<ExecutionCondition, BTreeSet<ExecutionCondition>>,
        BTreeMap<crate::ExecutionPlace, BTreeSet<ExecutionCondition>>,
    ) {
        let mut dependents = BTreeMap::<ExecutionCondition, BTreeSet<ExecutionCondition>>::new();

        let mut input_dependents =
            BTreeMap::<crate::ExecutionPlace, BTreeSet<ExecutionCondition>>::new();

        // Index current dependencies once, then visit only carriers reached by the worklist.
        for (carrier, witness) in &self.witnesses {
            for dependency in witness
                .dependencies
                .iter()
                .filter_map(|place| place.value_in(&self.current))
            {
                // A support's lifetime is independent of its current owned children.
                let observed = dependency.referent_owner_identity();

                if let crate::ExecutionConditionKind::Input(place) = observed.kind() {
                    input_dependents.entry(place.clone()).or_default().insert(carrier.clone());
                } else {
                    let mut owner = &observed;

                    loop {
                        dependents.entry(owner.owner_identity().clone()).or_default().insert(carrier.clone());

                        let crate::ExecutionConditionKind::Projection(_, parent) = owner.kind() else { break; };

                        owner = parent;
                    }
                }
            }
        }

        (dependents, input_dependents)
    }

    pub(super) fn retire_entry_context(
        &mut self,
        revokes: &impl Fn(&ExecutionCondition) -> bool,
        retired: &BTreeSet<(ExecutionCondition, bool)>,
        owner_revoked: &impl Fn(&ExecutionCondition, &crate::execution_guarantees::ExecutionWitness) -> bool,
    ) {
        let clause_revoked = |condition: &ExecutionCondition, support: &crate::execution_guarantees::ExecutionGuarantee| {
            revokes(condition) || support.addresses.keys().any(revokes)
        };

        let facts_revoked = |entry: &crate::ExecutionCallEvidence| entry.pending_execution && entry.assumptions
            .iter().chain(&entry.trusted_assumptions).any(|fact| revokes(&fact.0) || retired.contains(fact));

        self.update_entry_snapshots(&|entry| {
            facts_revoked(entry) || entry.witness_support_revoked(owner_revoked, &clause_revoked)
        }, &mut |entry| {
            entry.revoke_witness_support(owner_revoked, &clause_revoked);

            if entry.pending_execution {
                entry.assumptions.retain(|fact| !revokes(&fact.0) && !retired.contains(fact));
                entry.trusted_assumptions.retain(|fact| !revokes(&fact.0) && !retired.contains(fact));
            }
        });
    }

    pub(super) fn invalidate_referents(
        &mut self,
        values: &[ExecutionCondition],
        address_slots: &BTreeSet<ExecutionCondition>,
        stable_predicates: &BTreeSet<bray_symbols::PredicateDefinitionSymbolId>,
        preserved: &BTreeSet<ExecutionCondition>,
        stable_values: &crate::execution_guarantees::ObservationSet,
    ) {
        if values.is_empty() {
            return;
        }

        let mut referents = crate::execution_guarantees::ObservationSet::default();

        for value in values {
            referents.insert(value.clone());
        }

        let stable_contents = |value: &ExecutionCondition| {
            // Stable address bits do not certify unchanged pointee contents.
            stable_values.preserves(value) && (matches!(value.kind(), crate::ExecutionConditionKind::Borrowed(_))
                || !address_slots.contains(value.owner_identity()))
        };

        let observes_only_stable = memoize_condition(|condition| condition.observes_only(&stable_contents));

        let affected_referent = memoize_observations(|observation| referents.overlaps(observation), |observation| {
            // A borrow of an address slot observes the descriptor itself.
            // Pointee mutation still revokes claims on the unborrowed address.
            matches!(observation.kind(), crate::ExecutionConditionKind::Borrowed(value)
                if address_slots.contains(value.owner_identity()))
        });

        // A referent write changes observations through borrows and callable or
        // predicate contracts. A by-value guard, including a nullable tag, still
        // describes the same live owner. Physical replacement retires its observations separately.
        let affected_observation = memoize_observations(|observation| {
            matches!(observation.kind(), crate::ExecutionConditionKind::Borrowed(_)
                | crate::ExecutionConditionKind::Predicate(_, _, _)
                | crate::ExecutionConditionKind::Call(_, _, _, _, _))
                && affected_referent(observation) && !observes_only_stable(observation)
        }, |observation| {
            preserved.contains(observation)
                || matches!(observation.kind(), crate::ExecutionConditionKind::Predicate(symbol, _, _) if stable_predicates.contains(symbol))
        });
        let revokes_observation = |condition: &ExecutionCondition| affected_observation(condition) && !observes_only_stable(condition);

        self.assumptions
            .retain(|(condition, _)| !revokes_observation(condition));

        self.trusted_assumptions
            .retain(|(condition, _)| !revokes_observation(condition));

        self.witnesses.retain(|_, witness| {
            witness.retain_conditions(|condition, _| !revokes_observation(condition));

            !witness.conditions.is_empty() || !witness.dependencies.is_empty()
        });

        self.allocation_owners
            .retain(|condition, _| !revokes_observation(condition));

        self.retire_entry_context(&revokes_observation, &BTreeSet::new(), &|_, _| false);
    }

    pub(super) fn invalidate_contents(
        &mut self,
        places: &BTreeSet<crate::ExecutionPlace>,
        values: impl IntoIterator<Item = ExecutionCondition>,
        stable_predicates: &BTreeSet<bray_symbols::PredicateDefinitionSymbolId>,
        stable_values: &crate::execution_guarantees::ObservationSet,
    ) {
        // Writes revoke observed contents. A live carrier's lifetime dependencies
        // are retired only when the physical owner or capability ends.
        self.retire_values(
            values,
            stable_values,
            None,
            stable_predicates,
            (bray_symbols::DependencyMutationKind::Write, places),
        );
    }

    pub(super) fn exclude_live_witnesses(
        &self,
        retired: &mut BTreeSet<(ExecutionCondition, bool)>,
    ) {
        for witness in self.witnesses.values().filter(|witness| witness.available) {
            if retired.is_empty() {
                break;
            }

            for condition in witness.conditions.keys() {
                condition
                    .clone()
                    .assume_with(true, &mut |condition, holds| {
                        retired.remove(&(condition, holds));
                    });
            }
        }
    }

    fn retire_witness_dependencies(
        &mut self,
        expired: &impl Fn(&crate::ExecutionPlace) -> bool,
    ) {
        let mut retired = BTreeSet::new();

        let address_support_expired = |guarantee: &crate::execution_guarantees::ExecutionGuarantee| {
            guarantee.addresses.values().flatten().any(expired)
        };

        self.witnesses.retain(|carrier, witness| {
            // A scoped view expires without changing the independently held
            // referent. Root-bound address clauses expire at this same boundary.
            if matches!(carrier.ownership_identity(), Some((crate::execution_guarantees::ExecutionObservationKind::Reference, _)))
                && witness.dependencies.iter().any(expired)
            {
                for condition in witness.conditions.keys() {
                    condition.clone().assume(true, &mut retired);
                }

                return false;
            }

            witness.retain_conditions(|condition, guarantee| {
                if address_support_expired(guarantee) {
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

        let owner_revoked = |_: &ExecutionCondition, witness: &crate::execution_guarantees::ExecutionWitness| {
            witness.dependencies.iter().any(expired)
        };

        let clause_revoked = |_: &ExecutionCondition, guarantee: &crate::execution_guarantees::ExecutionGuarantee| {
            address_support_expired(guarantee)
        };

        self.update_entry_snapshots(&|entry| {
            entry.witness_support_revoked(&owner_revoked, &clause_revoked)
                || entry.addresses.values().flat_map(|addresses| addresses.values().flatten()).any(expired)
                || entry.pending_execution && entry.assumptions.iter().chain(&entry.trusted_assumptions).any(|fact| retired.contains(fact))
        }, &mut |entry| {
            entry.revoke_witness_support(&owner_revoked, &clause_revoked);
            entry.addresses.retain(|_, addresses| {
                addresses.retain(|_, places| !places.iter().any(expired));
                !addresses.is_empty()
            });

            if entry.pending_execution {
                entry.assumptions.retain(|fact| !retired.contains(fact));
                entry.trusted_assumptions.retain(|fact| !retired.contains(fact));
            }
        });
    }

    pub(super) fn invalidate_trusted_roots(
        &mut self,
        roots: &BTreeSet<crate::execution_guarantees::ExecutionInput>,
    ) {
        if roots.is_empty() {
            return;
        }

        self.assumptions.retain(|(condition, _)| !condition.observes_live_where(|value| {
            value.observations().any(|observed| matches!(observed.kind(), crate::ExecutionConditionKind::Input(place) if roots.contains(&place.root)))
        }, |_| false));

        self.retire_witness_dependencies(&|place| roots.contains(&place.root));

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

/// Forget retired availability after discharging its known ordinary consequences.
fn retain_ordinary_facts(
    facts: &mut crate::execution_guarantees::ExecutionAssumptions,
    proofs: &crate::execution_guarantees::ExecutionProofCache,
    expired: impl Fn(&ExecutionCondition) -> bool,
    retained: impl Fn(&ExecutionCondition) -> bool,
) {
    let guards = facts.iter().filter(|(condition, _)| !condition.contains_trusted() && expired(condition))
        .flat_map(|(condition, _)| condition.observations())
        .filter(|condition| matches!(condition.kind(), crate::ExecutionConditionKind::InitializedSnapshot(..)) && expired(condition))
        .collect::<BTreeSet<_>>();

    let proof = proofs.ordinary_proof(facts);

    let known = guards.into_iter().filter_map(|guard| {
        proof.prove_mixed(guard, &mut { ExecutionCondition::WORK_LIMIT })
            .map(|holds| (guard, ExecutionCondition::boolean(holds)))
    }).collect::<Vec<_>>();

    if known.is_empty() {
        facts.retain(|(condition, _)| !expired(condition) && retained(condition));

        return;
    }

    let substitution = crate::execution_guarantees::GuardSubstitution::new(known);
    let mut consequences = crate::execution_guarantees::ExecutionAssumptions::default();

    for (condition, holds) in facts.iter().filter(|(condition, _)| !condition.contains_trusted() && expired(condition)) {
        let condition = substitution.apply(condition);

        if !expired(&condition) && retained(&condition) {
            condition.assume(*holds, &mut consequences);
        }
    }

    facts.retain(|(condition, _)| !expired(condition) && retained(condition));
    facts.extend(consequences);
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
    fn retiring_one_frame_keeps_a_distinct_frame_from_the_same_site() {
        use crate::analysis::guarantee::deferred::{DeferredInvocation, DeferredValue};
        use crate::test_support::{error_type, expression_unit, push_expression};
        use bray_bound_tree::{BoundErrorExpression, BoundExpression, BoundUnitId};
        use std::sync::Arc;

        let (_, expressions) = expression_unit(BoundUnitId::new(92), |tree, origin| {
            vec![push_expression(tree, BoundExpression::Error(BoundErrorExpression::new(origin, error_type())))]
        });

        let expression = expressions[0];
        let owner = ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression));
        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));

        let frame = |captured| owner.clone().with_deferred_value(Arc::new(DeferredValue {
            producers: [(expression.into(), DeferredInvocation {
                callee: None,
                transition: None,
                entry: Arc::new(crate::ExecutionCallEvidence {
                    arguments: [(place.clone(), ExecutionCondition::boolean(captured))].into(),
                    pending_execution: true,
                    ..Default::default()
                }),
                completed: None,
                reports: Default::default(),
                selection: Default::default(),
            })].into(),
            unknown: None,
        }));

        let retired = frame(false);
        let live = frame(true);
        let mut state = ExecutionState::default();

        state.expressions.insert(expression, retired.clone());
        state.current.insert(place.clone(), live.clone());
        state.retire_trusted_values([retired], &BTreeSet::new());

        assert!(state.expressions[&expression].deferred_value().is_none());
        assert_eq!(place.value_in(&state.current), Some(live));
    }

    #[test]
    fn writing_a_loop_slot_retires_its_scalar_selector_without_retiring_captures() {
        use crate::analysis::guarantee::deferred::{DeferredInvocation, DeferredValue};
        use crate::test_support::{error_type, expression_unit, push_expression};
        use bray_bound_tree::{BoundErrorExpression, BoundExpression, BoundUnitId};
        use std::sync::Arc;

        let (_, expressions) = expression_unit(BoundUnitId::new(91), |tree, origin| {
            vec![push_expression(tree, BoundExpression::Error(BoundErrorExpression::new(origin, error_type())))]
        });

        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let join = crate::ExecutionJoinSite::Block(BoundUnitId::new(91), 6);
        let selector = ExecutionCondition::opaque_scalar(ExecutionCondition::new(crate::ExecutionConditionKind::Joined(join, place.clone())));

        let unrelated = ExecutionCondition::opaque_scalar(ExecutionCondition::new(crate::ExecutionConditionKind::Joined(join,
            ExecutionPlace::argument(SymbolOrdinal::new(1)))));

        let captured = ExecutionCondition::opaque_scalar(ExecutionCondition::new(
            crate::ExecutionConditionKind::Expression(expressions[0])));

        let mut state = ExecutionState::default();

        state.current.insert(ExecutionPlace::argument(SymbolOrdinal::new(2)), ExecutionCondition::result().with_deferred_value(Arc::new(DeferredValue {
            producers: [(expressions[0].into(), DeferredInvocation { callee: None, transition: None,
                entry: Arc::new(crate::ExecutionCallEvidence {
                    outputs: Default::default(),
                    arguments: [(ExecutionPlace::argument(SymbolOrdinal::new(0)), captured.clone())].into(),
                    assumptions: [(captured.clone(), true)].into(), ..Default::default()
                }), completed: None, reports: Default::default(), selection: Default::default(),
            })].into(),
            unknown: Some(DeferredInvocation { callee: None, transition: None, entry: (), completed: None, reports: Default::default(),
                selection: [(selector.clone(), true), (unrelated.clone(), false), (captured.clone(), true)].into(),
            }), ..Default::default()
        })));

        let field = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
        let sibling = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1));

        let root = ExecutionCondition::post_state(bray_bound_tree::StorageMutation::Operation(expressions[0].into()),
            ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone())));

        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");

        let claim = |projection| ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(
            ExecutionCondition::predicate(symbol.into(), crate::test_support::empty_substitution(symbol.into()),
                vec![root.clone().component(projection).borrowed()]))));

        let field_claim = claim(field);
        let sibling_claim = claim(sibling);

        field_claim.clone().assume(true, &mut state.trusted_assumptions);
        sibling_claim.clone().assume(true, &mut state.trusted_assumptions);
        state.invalidate_contents(&[place.clone().component(field)].into(), [], &Default::default(), &Default::default());

        assert_ne!(field_claim.prove_trusted(&state.trusted_assumptions, &state.assumptions), Some(true));
        assert_eq!(sibling_claim.prove_trusted(&state.trusted_assumptions, &state.assumptions), Some(true));

        let mut retained_data = crate::execution_guarantees::ObservationSet::default();

        retained_data.preserve_copied_data([selector.clone()]);
        state.invalidate_contents(&[place].into(), [], &Default::default(), &retained_data);

        let value = state.current.live_value(&ExecutionPlace::argument(SymbolOrdinal::new(2))).unwrap();
        let frame = value.deferred_value().unwrap();
        let selection = &frame.unknown.as_ref().unwrap().selection;

        assert!(!selection.contains(&(selector, true)));
        assert!(selection.contains(&(unrelated, false)));
        assert!(selection.contains(&(captured, true)));

        let entry = &frame.producers[&expressions[0].into()].entry;

        assert!(!entry.assumptions.is_empty());
    }

    #[test]
    fn retiring_initialization_keeps_only_independent_known_consequences() {
        use bray_bound_tree::BoundOperator::{LogicalAnd, LogicalNot, LogicalOr};
        use std::sync::Arc;

        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let owner = ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone()));

        let initialized = ExecutionCondition::new(crate::ExecutionConditionKind::InitializedSnapshot(
            place, Arc::new(owner.clone()), Arc::new(owner.clone()),
        ));

        let tag = ExecutionCondition::result().matches_variant(
            bray_symbols::UnionVariantSymbolId::from_symbol_id(bray_symbols::SymbolId::new(920)),
        );

        let not = |value| ExecutionCondition::operation(LogicalNot, vec![value]);

        let relation = ExecutionCondition::operation(LogicalOr, vec![
            ExecutionCondition::operation(LogicalAnd, vec![initialized.clone(), tag.clone()]),
            ExecutionCondition::operation(LogicalAnd, vec![not(initialized.clone()), not(tag.clone())]),
        ]);

        let predicate = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");

        let trusted = ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(
            ExecutionCondition::predicate(predicate.into(), crate::test_support::empty_substitution(predicate.into()),
                vec![ExecutionCondition::result().borrowed()]),
        )));

        for known in [Some(true), Some(false), None] {
            let mut state = ExecutionState::default();

            state.assumptions.insert((ExecutionCondition::operation(LogicalOr,
                vec![not(initialized.clone()), trusted.clone()]), true));

            relation.clone().assume(true, &mut state.assumptions);

            if let Some(known) = known {
                initialized.clone().assume(known, &mut state.assumptions);
            }

            state.invalidate_trusted_values([owner.clone()]);

            assert_eq!(tag.prove_ordinary(&state.assumptions, Some(&state.proofs)), known);

            assert_eq!(initialized.prove_ordinary(&state.assumptions, Some(&state.proofs)), None,
                "a surviving consequence cannot revive retired initialization");

            assert_ne!(trusted.prove_trusted(&state.trusted_assumptions, &state.assumptions), Some(true),
                "ordinary guard discharge cannot establish trusted authority");
        }
    }

    #[test]
    fn retirement_revokes_owners_beyond_the_proof_budget() {
        for count in [1, ExecutionCondition::WORK_LIMIT + 1] {
            let owners = (0..count).map(|index| {
                ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(
                    SymbolOrdinal::new(u32::try_from(index).expect("test owner index fits ordinal")),
                )))
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
        let root = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let sibling = StorageProjection::TupleElement(SymbolOrdinal::new(1));

        let old = ExecutionCondition::owned_value(root.clone(), ExecutionCondition::constructed(root.clone(), [
            (field, ExecutionCondition::boolean(true)),
        ].into()));

        let changed = ExecutionCondition::owned_value(root.clone(), ExecutionCondition::constructed(root.clone(), [
            (field, ExecutionCondition::boolean(false)), (sibling, ExecutionCondition::boolean(true)),
        ].into()));

        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");

        let condition = ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(std::sync::Arc::new(ExecutionCondition::predicate(
            symbol.into(), crate::test_support::empty_substitution(symbol.into()),
            vec![ExecutionCondition::reference_value(root.clone().project(field).borrowed(), old.clone().project(field))],
        ))));

        for (returned, preserved) in [
            (old.clone(), true),
            (old.with_storage_component(&[field], ExecutionCondition::scalar_observation(
                ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(2)))),
                old.clone().project(field),
            )), true),
            (ExecutionCondition::owned_value(root.clone(), ExecutionCondition::unknown()), false),
            (changed, false),
        ] {
            let mut state = ExecutionState::default();

            state.carry_guarantee(root.clone(), condition.clone());
            condition.clone().assume(true, &mut state.trusted_assumptions);
            state.retire_trusted_values([old.clone()], &BTreeSet::from([returned.clone()]));

            assert_eq!(condition.prove_trusted(&state.trusted_assumptions, &state.assumptions) == Some(true), preserved);

            if returned == old || !preserved {
                let stable = crate::execution_guarantees::ObservationSet::from_iter([
                    returned.clone(), returned.borrowed(),
                ]);

                assert!(stable.contains(&old), "retaining identity is independent of retaining data");
                assert_eq!(stable.preserves(&old), preserved);
            }
        }
    }

    #[test]
    fn copying_or_retiring_a_loan_does_not_own_or_retire_its_source() {
        let owner = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));

        for (condition, retained) in [
            (owner.clone(), BTreeSet::new()),
            (ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(std::sync::Arc::new(owner.clone().borrowed()))), BTreeSet::from([owner.clone()])),
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

            state.current.insert(scope.clone(), ExecutionCondition::new(crate::ExecutionConditionKind::Input(scope.clone())));
            state.carry_guarantee(loan.clone(), condition.clone());

            std::sync::Arc::make_mut(state.witnesses.get_mut(&loan).expect("the scoped view has its own evidence"))
                .dependencies.insert(scope.clone());

            let mut only_view = ExecutionState::default();

            only_view.current = state.current.clone();
            only_view.witnesses.insert(loan.clone(), state.witnesses[&loan].clone());
            only_view.trusted_assumptions.insert((condition.clone(), true));

            let support = ExecutionCondition::new(crate::ExecutionConditionKind::Input(scope.clone()));
            let child = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(2))));

            for (retired, survives) in [(child.clone(), true), (support.clone(), false)] {
                let mut nested = only_view.clone();

                nested.current.insert(scope.clone(), ExecutionCondition::constructed(support.clone(),
                    [(StorageProjection::TupleElement(SymbolOrdinal::new(0)), child.clone())].into()));

                nested.retire_trusted_values([retired], &BTreeSet::new());

                assert_eq!(nested.trusted_assumptions.contains(&(condition.clone(), true)), survives,
                    "retiring a contained value differs from ending the support's lifetime");
            }

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

        let result = ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expressions[0]));
        let child = ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expressions[1]));
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
        let root = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));
        let field = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
        let left = root.clone().component(field);
        let right = root.clone().component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)));
        let replacement = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(1))));
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
        assert_eq!(state.trusted_assumptions, [(root, true), (right, true)].into());
        assert!(!state.witnesses.contains_key(&left));
    }

    #[test]
    fn retained_owner_and_retained_projected_contents_are_distinct() {
        let root = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));
        let field = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
        let child = root.clone().component(field);
        let before = ExecutionCondition::owned_value(child.clone(), ExecutionCondition::boolean(true));
        let after = ExecutionCondition::owned_value(child, ExecutionCondition::boolean(false));

        let retained = crate::execution_guarantees::RetainedSnapshots::from(BTreeSet::from([ExecutionCondition::constructed(root.clone(), [(
            StorageProjection::TupleElement(SymbolOrdinal::new(0)), after.clone(),
        )].into())]));

        assert!(retained.retains_owner(&root));
        assert!(retained.retains_owner(&before));
        assert!(!retained.retains_contents(&before));

        let mut prior = crate::execution_guarantees::RetainedSnapshots::from(BTreeSet::from([before.clone()]));

        assert!(!prior.retains_owner(&root));
        assert!(!prior.retains_contents(&after));

        {
            let unchanged = super::memoize_condition(|value| prior.retains_contents_between(&retained, value));
            let same = super::memoize_condition(|value| retained.retains_contents_between(&retained, value));

            for _ in 0..2 {
                assert!(retained.retains_owner(&before));
                assert!(!retained.retains_contents(&before));
                assert!(retained.retains_contents(&after));
                assert!(!unchanged(&before), "owner retention cannot preserve overwritten data");
                assert!(same(&after), "a different predecessor has its own scoped answer");
            }
        }

        prior.insert(root.clone());
        prior.insert(after.clone());

        assert!(prior.retains_owner(&root));
        assert!(prior.retains_contents(&after));

        let mut stable = crate::execution_guarantees::ObservationSet::from_iter(retained.clone());

        for _ in 0..2 {
            assert!(!stable.preserves(&before), "same owner cannot revive overwritten contents");
            assert!(stable.preserves(&after));
        }

        stable.insert(before.clone());

        assert!(stable.preserves(&before), "a newly retained snapshot invalidates the earlier negative query");

        let borrowed = root.borrowed();

        assert!(!stable.contains(&borrowed));

        stable.insert(borrowed.clone());

        assert!(stable.contains(&borrowed));
    }
}
