use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::BoundExpressionId;

use crate::ExecutionCondition;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExecutionState {
    pub(super) trust_boundaries: BTreeSet<BoundExpressionId>,
    pub(super) assumptions: BTreeSet<(ExecutionCondition, bool)>,
    pub(super) witness_carriers: BTreeMap<ExecutionCondition, BTreeSet<ExecutionCondition>>,
    pub(super) witness_dependencies: BTreeMap<ExecutionCondition, BTreeSet<crate::ExecutionPlace>>,
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
            witness_carriers: BTreeMap::new(),
            witness_dependencies: BTreeMap::new(),
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
        for (carrier, conditions) in returned.carriers {
            for condition in &conditions {
                condition
                    .clone()
                    .assume(true, &mut self.trusted_assumptions);
            }

            self.witness_carriers
                .entry(carrier)
                .or_default()
                .extend(conditions);
        }

        self.witness_dependencies.extend(returned.dependencies);

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
        self.witness_carriers
            .entry(carrier.owner_identity().clone())
            .or_default()
            .insert(condition);
    }

    pub(super) fn directly_carries_witness(&self, value: &ExecutionCondition) -> bool {
        self.witness_carriers
            .get(value.owner_identity())
            .is_some_and(|conditions| {
                conditions.iter().any(|condition| {
                    (self
                        .trusted_assumptions
                        .contains(&(condition.clone(), true))
                        || condition.prove_trusted(&self.trusted_assumptions, &self.assumptions)
                            == Some(true))
                        && condition
                            .prove(&self.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
                            != Some(true)
                })
            })
    }

    pub(super) fn is_witness(&self, value: &ExecutionCondition) -> bool {
        self.witness_carriers.keys().any(|carrier| {
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

        self.witness_carriers.retain(|value, conditions| {
            conditions.retain(|condition| !stale(condition));

            !stale(value) && !conditions.is_empty()
        });

        self.witness_dependencies.retain(|value, _| !stale(value));

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
                .carriers
                .iter()
                .any(|(carrier, conditions)| stale(carrier) || conditions.iter().any(stale))
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

        self.witness_carriers.retain(|carrier, conditions| {
            if !stable_owners
                .iter()
                .any(|owner| carrier.is_owned_projection_of(owner))
            {
                return false;
            }

            conditions.retain(|condition| condition.observes_only(stable_values));

            for condition in conditions.iter() {
                condition.clone().assume(true, &mut retained);
            }

            !conditions.is_empty()
        });

        self.witness_dependencies
            .retain(|carrier, _| self.witness_carriers.contains_key(carrier));

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

        let mut carriers = BTreeMap::<ExecutionCondition, BTreeSet<ExecutionCondition>>::new();

        for (carrier, conditions) in &self.witness_carriers {
            let carrier = rename_identity(carrier);

            if carrier != ExecutionCondition::Unknown {
                carriers.entry(carrier).or_default().extend(
                    conditions
                        .iter()
                        .map(rename)
                        .filter(|condition| *condition != ExecutionCondition::Unknown),
                );
            }
        }

        self.witness_carriers = carriers;

        self.allocation_owners = self
            .allocation_owners
            .iter()
            .map(|(condition, owner)| (rename(condition), rename_identity(owner)))
            .filter(|(condition, _)| *condition != ExecutionCondition::Unknown)
            .collect();

        let mut dependencies =
            BTreeMap::<ExecutionCondition, BTreeSet<crate::ExecutionPlace>>::new();

        for (carrier, owners) in &self.witness_dependencies {
            let carrier = rename_identity(carrier);

            if carrier != ExecutionCondition::Unknown {
                dependencies
                    .entry(carrier)
                    .or_default()
                    .extend(owners.iter().cloned());
            }
        }

        self.witness_dependencies = dependencies;

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

        let mut dependents = BTreeMap::<ExecutionCondition, BTreeSet<ExecutionCondition>>::new();

        let mut input_dependents =
            BTreeMap::<crate::ExecutionPlace, BTreeSet<ExecutionCondition>>::new();

        // Index current dependencies once, then visit only carriers reached by the worklist.
        for (carrier, dependencies) in self
            .witness_dependencies
            .iter()
            .filter(|_| !kind.is_write())
        {
            for dependency in dependencies
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

        let revokes = |condition: &ExecutionCondition| observes(condition) && !stable(condition);

        self.assumptions
            .retain(|(condition, _)| !condition.observes_borrowed_where(observes));

        let mut retired_conditions = BTreeSet::new();

        self.witness_carriers.retain(|carrier, conditions| {
            let retired = invalidated_observations.contains(carrier)
                && !retained_observations.contains(carrier);

            conditions.retain(|condition| {
                if (retired && (directly_changed.contains(carrier) || !stable(condition)))
                    || revokes(condition)
                {
                    condition.clone().assume(true, &mut retired_conditions);

                    false
                } else {
                    true
                }
            });

            !conditions.is_empty()
        });

        // Another live carrier may independently establish the same condition.
        for conditions in self.witness_carriers.values() {
            for condition in conditions {
                condition
                    .clone()
                    .assume_with(true, &mut |condition, holds| {
                        retired_conditions.remove(&(condition, holds));
                    });
            }
        }

        self.trusted_assumptions
            .retain(|fact| !revokes(&fact.0) && !retired_conditions.contains(fact));

        self.allocation_owners
            .retain(|condition, owner| !observes(condition) && !invalidated.contains(owner));

        // A content write may revoke all old facts on an intermediate address.
        // Keep its lifetime edge so newly established facts still expire with the owner.
        self.witness_dependencies.retain(|carrier, _| {
            kind.is_write()
                || !invalidated_observations.contains(carrier)
                || retained_observations.contains(carrier)
                || self.witness_carriers.contains_key(carrier)
                || !stable_predicates.is_empty() && !directly_changed.contains(carrier)
        });

        for entry in self
            .entries
            .values_mut()
            .filter(|entry| entry.pending_execution)
        {
            entry
                .assumptions
                .retain(|fact| !revokes(&fact.0) && !retired_conditions.contains(fact));

            entry
                .trusted_assumptions
                .retain(|fact| !revokes(&fact.0) && !retired_conditions.contains(fact));
        }
    }

    pub(super) fn invalidate_referents(
        &mut self,
        values: &[ExecutionCondition],
        stable_predicates: &BTreeSet<bray_symbols::PredicateDefinitionSymbolId>,
    ) {
        if values.is_empty() {
            return;
        }

        let revokes = |condition: &ExecutionCondition| {
            let predicate = match condition {
                ExecutionCondition::Trusted(predicate) => predicate.as_ref(),
                condition => condition,
            };

            !matches!(predicate, ExecutionCondition::Predicate(symbol, _, _) if stable_predicates.contains(symbol))
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

        self.witness_carriers.retain(|_, conditions| {
            conditions.retain(|condition| !revokes(condition));

            !conditions.is_empty()
        });

        self.allocation_owners
            .retain(|condition, _| !revokes(condition));

        for entry in self
            .entries
            .values_mut()
            .filter(|entry| entry.pending_execution)
        {
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
            !condition.observes_borrowed_where(|value| {
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

    pub(super) fn invalidate_trusted_roots(
        &mut self,
        roots: &BTreeSet<crate::execution_guarantees::ExecutionInput>,
    ) {
        self.assumptions.retain(|(condition, _)| !condition.observes_borrowed_where(|value| {
            value.observations().any(|observed| matches!(observed, ExecutionCondition::Input(place) if roots.contains(&place.root)))
        }));

        let dependent = self
            .witness_dependencies
            .iter()
            .filter(|(_, owners)| owners.iter().any(|owner| roots.contains(&owner.root)))
            .map(|(carrier, _)| carrier.clone())
            .collect::<Vec<_>>();

        self.invalidate_trusted_values(dependent);
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
        assert!(!state.witness_carriers[&input(0)].contains(&predicate));
        assert!(state.witness_carriers[&input(1)].contains(&predicate));

        state.assumptions = [(guard, false)].into();
        state.refine_witnesses();
        assert!(state.witness_carriers[&input(0)].contains(&predicate));
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
            .witness_dependencies
            .insert(carrier.clone(), BTreeSet::from([owner.clone()]));

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

        assert!(state.witness_dependencies.contains_key(&carrier));

        let mut ended = state.clone();

        ended.invalidate_trusted_roots(&BTreeSet::from([owner.root]));
        assert!(ended.trusted_assumptions.is_empty());
        assert!(ended.witness_dependencies.is_empty());

        // A carrier can establish a condition about a separate pointer. Overwriting
        // that carrier must revoke the condition even though the pointer is unchanged.
        let external = input(2);

        state.witness_carriers.clear();
        state.witness_dependencies.clear();
        state.carry_guarantee(external.clone(), spatial.clone());
        state.invalidate_contents(&BTreeSet::new(), [external], &stable);
        assert!(state.trusted_assumptions.is_empty());
        assert!(state.witness_carriers.is_empty());

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

        state
            .witness_dependencies
            .insert(input(1), BTreeSet::from([owner.clone()]));

        state
            .witness_dependencies
            .insert(carrier.clone(), BTreeSet::from([intermediate]));

        state.carry_guarantee(carrier, spatial.clone());
        state.trusted_assumptions.insert((spatial.clone(), true));

        state.invalidate_contents(
            &BTreeSet::from([owner.clone()]),
            [input(0)],
            &BTreeSet::from([symbol.into()]),
        );

        assert_eq!(state.trusted_assumptions, BTreeSet::from([(spatial, true)]));
        assert!(state.witness_dependencies.contains_key(&input(1)));

        state.invalidate_trusted_roots(&BTreeSet::from([owner.root]));
        assert!(state.trusted_assumptions.is_empty());
        assert!(state.witness_dependencies.is_empty());
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
                .witness_carriers
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>(),
            retained
        );

        assert_eq!(
            moved.trusted_assumptions,
            BTreeSet::from([(child.clone(), true)])
        );

        // Retaining a value cannot extend the storage required by its witness.
        state
            .witness_dependencies
            .insert(child, BTreeSet::from([root.clone()]));

        state.invalidate_trusted_roots(&BTreeSet::from([root.root.clone()]));
        state.retire_trusted_values([ExecutionCondition::Input(root)], &retained);

        assert!(state.witness_carriers.is_empty());
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
                .witness_dependencies
                .insert(carrier.clone(), BTreeSet::from([dependency]));

            state.carry_guarantee(carrier.clone(), carrier.clone());
            state.trusted_assumptions.insert((carrier, true));
        }

        let cycle = ExecutionPlace::argument(SymbolOrdinal::new(514));
        let independent = input(1000);

        state.current.insert(cycle.clone(), input(513));

        state
            .witness_dependencies
            .insert(input(1), BTreeSet::from([cycle]));

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
                assert_eq!(state.witness_dependencies.len(), 513);

                state.invalidate_trusted_roots(&BTreeSet::from([ExecutionPlace::argument(
                    SymbolOrdinal::new(1),
                )
                .root]));
            } else {
                state.invalidate_trusted(&input(1));
            }

            assert_eq!(
                state
                    .witness_carriers
                    .keys()
                    .cloned()
                    .collect::<BTreeSet<_>>(),
                BTreeSet::from([independent.clone()])
            );

            assert_eq!(
                state.trusted_assumptions,
                BTreeSet::from([(independent.clone(), true)])
            );

            assert!(state.witness_dependencies.is_empty());
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
                .witness_dependencies
                .insert(carrier.clone(), BTreeSet::from([dependency]));

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
                .witness_carriers
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([input(22)])
        );

        assert_eq!(
            state.trusted_assumptions,
            BTreeSet::from([(captured, true), (input(22), true)])
        );

        assert_eq!(state.assumptions, BTreeSet::from([(changed, true)]));
        assert_eq!(state.witness_dependencies.len(), 1);
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

        state.witness_carriers.clear();
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
        assert!(!state.witness_carriers.contains_key(&owner));
        assert!(state.is_witness(&child));
        assert!(!state.trusted_assumptions.contains(&(owner_condition, true)));
        assert!(state.trusted_assumptions.contains(&(child_condition, true)));
    }
}
