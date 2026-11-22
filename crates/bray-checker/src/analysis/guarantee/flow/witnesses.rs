use crate::execution_guarantees::ExecutionAssumptions;
use crate::execution_guarantees::{ExecutionSubstitution, SnapshotSubstitution};
use crate::execution_guarantees::{ExecutionObservationKind as ObservationKind, ExecutionProof};
use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::BoundOperator;

use super::super::state::ExecutionState;
use crate::{ExecutionCondition, ExecutionPlace};

/// Clauses retain one joined owner and its predecessor availability.
#[derive(Debug)]
pub(super) struct JoinedWitness {
    pub(super) owner: ExecutionCondition,
    pub(super) available: bool,
    pub(super) clauses: Vec<(ExecutionCondition, ExecutionCondition, bool)>,
}

/// Propose value-contract facts and prove each against both original predecessors.
pub(super) fn conditional_facts<'value>(
    left: (
        &ExecutionState,
        &ExecutionSubstitution<'_>,
    ),
    right: (
        &ExecutionState,
        &ExecutionSubstitution<'_>,
    ),
    current: &BTreeMap<ExecutionPlace, ExecutionCondition>,
    results: impl IntoIterator<Item = ([Option<&'value ExecutionCondition>; 2], &'value ExecutionCondition)>,
    availability: &BTreeSet<(ExecutionCondition, ExecutionCondition)>,
    selector: Option<&ExecutionCondition>,
    facts: &mut Vec<(ExecutionCondition, bool)>,
) -> Vec<JoinedWitness> {
    let mut guards = availability.clone();
    let mut inputs = [BTreeMap::new(), BTreeMap::new()];
    let absent = [left.0, right.0].map(absent_observations);

    for (place, joined) in current {
        let source = [left.0, right.0].map(|state| {
            (super::merge::resolved_availability(state, place) != super::super::current::ValueAvailability::Absent)
                .then(|| place.value_in(&state.current).unwrap_or_else(ExecutionCondition::unknown))
        });

        collect_observations(
            [source[0].as_ref(), source[1].as_ref()], joined, &mut inputs, &mut guards,
            [&absent[0], &absent[1]], &ExecutionCondition::boolean(true),
            &mut { ExecutionCondition::WORK_LIMIT },
        );
    }

    for (source, joined) in results {
        collect_observations(
            source, joined, &mut inputs, &mut guards,
            [&absent[0], &absent[1]], &ExecutionCondition::boolean(true),
            &mut { ExecutionCondition::WORK_LIMIT },
        );
    }

    if std::env::var_os("BRAY_BRA694_TRACE").is_some()
        && current.values().any(|value| format!("{:?}", value.kind()).contains("Joined(Block(BoundUnitId(2848653), 162)"))
    {
        for (side, input) in inputs.iter().enumerate() {
            let selected = input.iter().filter(|(joined, _)| {
                let key = format!("{:?}", joined.kind());
                key.contains("SymbolId(5257)") && key.contains("Joined(Block(BoundUnitId(2848653), 162)")
            }).collect::<Vec<_>>();

            eprintln!("BRA694 SNAPSHOT MAP side={side} total={} selected={}", input.len(), selected.len());

            for (joined, source) in selected {
                eprintln!("BRA694 SNAPSHOT PAIR side={side} joined={:?} source={:?}", joined.kind(), source.kind());
            }
        }
    }

    let proposals = if std::env::var_os("BRAY_BRA694_DISABLE_PROPOSALS").is_some() {
        [BTreeMap::new(), BTreeMap::new()]
    } else {
        inputs.each_ref().map(joined_snapshot_proposals)
    };
    let no_owners = BTreeMap::new();
    let [left_proposals, right_proposals] = proposals.each_ref().map(|values| ExecutionSubstitution::new(values, &no_owners));
    let [left_inputs, right_inputs] = inputs.each_ref().map(|inputs| SnapshotSubstitution::new(inputs));

    // Immutable branch selectors can qualify a moved carrier even when the selector
    // is stored outside it. Each proposed clause still needs both predecessor proofs.
    let path_guards = if let Some(selector) = selector {
        // A structured join owns its selector. Derived consequences are facts, not
        // additional execution choices to multiply against every carrier clause.
        [left, right].into_iter().flat_map(|(_, substitution)| {
            let selector = substitution.value(selector);

            [selector.clone(), ExecutionCondition::operation(BoundOperator::LogicalNot, vec![selector])]
        }).collect::<BTreeSet<_>>()
    } else {
        let [first, second] = [left, right].map(|(state, substitution)| {
            state.assumptions.iter()
                .filter(|(condition, _)| !condition.contains_trusted() && !condition.observes_live(None))
                .map(|(condition, holds)| (substitution.value(condition), *holds))
                .collect::<BTreeSet<_>>()
        });

        first.iter()
            .filter(|(condition, holds)| second.contains(&(condition.clone(), !holds)))
            .flat_map(|(condition, _)| {
                [condition.clone(), ExecutionCondition::operation(BoundOperator::LogicalNot, vec![condition.clone()])]
            }).collect::<BTreeSet<_>>()
    };

    let ordinary_proofs = [left.0, right.0].map(|state| state.ordinary_proof());
    let all_guards = guards.iter().map(|(_, guard)| guard).chain(&path_guards).collect::<BTreeSet<_>>();
    let inactive = inactive_guards(&all_guards, [&left_inputs, &right_inputs], &ordinary_proofs, facts);

    conditional_aliases([left, right], [&left_inputs, &right_inputs], &all_guards, &inactive, &ordinary_proofs, facts);

    if left.0.witnesses.is_empty() && right.0.witnesses.is_empty() {
        return Vec::new();
    }

    let candidates = [(left, &left_proposals), (right, &right_proposals)]
        .into_iter()
        .flat_map(|((state, substitution), proposals)| {
            state
                .witnesses
                .iter()
                .flat_map(move |(owner, witness)| {
                    let mut witness = std::sync::Arc::clone(witness);
                    let mut trusted = std::borrow::Cow::Borrowed(&state.trusted_assumptions);
                    let mut ordinary = std::borrow::Cow::Borrowed(&state.assumptions);

                    if witness.has_guards() {
                        witness.refine(trusted.to_mut(), ordinary.to_mut(), &state.proofs);
                    }

                    let proof = state.proofs.proof(&trusted, &ordinary);

                    witness.conditions.keys().filter_map(|condition| {
                        (!witness.available || proof.prove_mixed(condition,
                            &mut { ExecutionCondition::WORK_LIMIT }) == Some(true))
                        .then(|| substitution.value(condition))
                    }).flat_map(|condition| {
                        // Repeated joins may qualify the same underlying claim with
                        // different path guards. Consider that claim directly, then
                        // retain it only if both actual predecessors prove it. A joined
                        // fallback is another proposal, never a rewrite of the source fact.
                        let proposed = proposals.value(&condition);

                        std::iter::once(condition.clone())
                            .chain((proposed != condition && !proposed.contains_unknown()).then_some(proposed))
                            .flat_map(|condition| {
                                let claims = condition.observations()
                                    .filter(|candidate| matches!(candidate.kind(), crate::ExecutionConditionKind::Trusted(_)))
                                    .cloned().collect::<Vec<_>>();

                                std::iter::once(condition).chain(claims)
                            })
                            .map(|condition| (substitution.owner(owner), condition))
                            .collect::<Vec<_>>()
                    }).collect::<Vec<_>>()
                })
        })
        .fold(BTreeMap::<_, BTreeSet<_>>::new(), |mut owners, (owner, condition)| {
            owners.entry(owner).or_default().insert(condition);

            owners
        });

    let mut retained = Vec::new();

    let source_proofs = [left.0, right.0].map(|state| {
        state.proof()
    });

    for (owner, conditions) in candidates {
        if owner == ExecutionCondition::unknown() {
            continue;
        }

        // Each complete carrier supplies its own authority. An ordinary inactive
        // variant needs no trusted evidence, but another carrier cannot lend it.
        let predecessors =
            [(0, (left.0, &left_inputs)), (1, (right.0, &right_inputs))].map(|(index, (state, inputs))| {
                let source = inputs.apply(&owner);
                let mut evidence = ExecutionAssumptions::default();
                let mut owning = ExecutionAssumptions::default();
                let mut available = true;

                for (_, witness) in state.owned_witnesses([source]) {
                    available &= witness.available;

                    for (condition, guarantee) in witness.conditions.iter().filter(|(condition, _)| {
                        !witness.available || source_proofs[index].prove_trusted(condition) == Some(true)
                    }) {
                        condition.clone().assume(true, &mut evidence);

                        if guarantee.witness {
                            condition.clone().assume(true, &mut owning);
                        }
                    }
                }

                (state, inputs, evidence, owning, available)
            });

        let proofs = predecessors.each_ref().map(|(state, _, evidence, _, _)| {
            state.proofs.proof(evidence, &state.assumptions)
        });

        let owning_proofs = predecessors.each_ref().map(|(state, _, _, evidence, _)| {
            state.proofs.proof(evidence, &state.assumptions)
        });

        let proved = |clause: &ExecutionCondition| {
            predecessors.iter().zip(&proofs).all(|((_, inputs, _, _, _), proof)| {
                proof.prove_trusted(&inputs.apply(clause)) == Some(true)
            })
        };

        let owned = |clause: &ExecutionCondition| {
            predecessors.iter().zip(&owning_proofs).all(|((_, inputs, _, _, _), proof)| {
                proof.prove_trusted(&inputs.apply(clause)) == Some(true)
            })
        };

        // A join preserves validity while the child owns execution. It must not
        // make any unavailable predecessor's authority usable in the caller.
        let available = predecessors.iter().all(|(_, _, _, _, available)| *available);
        let mut clauses = Vec::new();

        for condition in conditions {
            if matches!(condition.kind(), crate::ExecutionConditionKind::Unknown | crate::ExecutionConditionKind::Boolean(true)) { continue; }

            if std::env::var_os("BRAY_BRA694_TRACE").is_some()
                && [("BoundUnitId(2848653)", "SymbolId(5264)"), ("BoundUnitId(1046592)", "SymbolId(2073)")]
                    .iter().any(|(unit, predicate)|
                        format!("{:?}", owner.kind()).contains(unit)
                            && format!("{:?}", condition.kind()).contains(predicate))
            {
                let compact = |value: &ExecutionCondition| format!("{:?}", value.kind()).chars().take(800).collect::<String>();
                eprintln!("BRA694 JOIN CANDIDATE owner={} condition={} proved={} owning={} predecessors={:?}",
                    compact(&owner), compact(&condition), proved(&condition), owned(&condition),
                    predecessors.iter().zip(&proofs).map(|((_, inputs, evidence, owning, available), proof)|
                        (compact(&inputs.apply(&condition)), proof.prove_trusted(&inputs.apply(&condition)),
                            evidence.len(), owning.len(), available)).collect::<Vec<_>>());
                if !proved(&condition) && matches!(condition.kind(), crate::ExecutionConditionKind::Trusted(_)) {
                    for (index, (state, inputs, _, _, _)) in predecessors.iter().enumerate() {
                        let source = inputs.apply(&owner);
                        let target = inputs.apply(&condition);
                        let proof = state.proof();
                        let witnesses = state.owned_witnesses([source.clone()]).map(|(carrier, witness)| {
                            let matching = witness.conditions.keys().filter(|claim| {
                                [("SymbolId(5264)", "BoundUnitId(2848653)"), ("SymbolId(2073)", "BoundUnitId(1046592)")]
                                    .iter().any(|(predicate, unit)| format!("{:?}", claim.kind()).contains(predicate)
                                        && format!("{:?}", carrier.kind()).contains(unit))
                            }).map(|claim| (compact(claim), proof.prove_trusted(claim))).collect::<Vec<_>>();
                            (compact(carrier), witness.available, matching)
                        }).collect::<Vec<_>>();
                        eprintln!("BRA694 JOIN EVIDENCE side={index} source={} direct={:?} raw={} uncached={:?} target={} witnesses={witnesses:?}",
                            compact(&source), proof.prove_trusted(&target), state.trusted_assumptions.contains(&(target.clone(), true)),
                            ExecutionProof::new(&state.trusted_assumptions, &state.assumptions).prove_trusted(&target),
                            compact(&target));
                    }
                }
            }

            // An existing guarded claim can already hold on both paths without
            // discovering a new discriminant or adding a redundant outer guard.
            if proved(&condition) {
                let owning = owned(&condition);

                clauses.push((condition.clone(), condition, owning));
                continue;
            }

            for guard in guards
                .iter()
                .filter(|(parent, _)| {
                    owner.is_owned_projection_of(parent) || parent.is_owned_projection_of(&owner)
                })
                .map(|(_, guard)| guard)
                .chain(&path_guards)
            {
                if inactive.contains(guard) { continue; }

                let clause = ExecutionCondition::operation(
                    BoundOperator::LogicalOr,
                    vec![
                        ExecutionCondition::operation(BoundOperator::LogicalNot, vec![guard.clone()]),
                        condition.clone(),
                    ],
                );

                // These are actual predecessor snapshots of moved owners, not ordinary
                // equal-content substitutions for borrowed observations.
                if !matches!(clause.kind(), crate::ExecutionConditionKind::Boolean(true)) && proved(&clause) {
                    let owning = owned(&clause);

                    clauses.push((condition.clone(), clause, owning));
                }
            }
        }

        if !clauses.is_empty() { retained.push(JoinedWitness { owner, available, clauses }); }
    }

    retained
}

fn joined_snapshot_proposals(
    inputs: &BTreeMap<ExecutionCondition, ExecutionCondition>,
) -> BTreeMap<ExecutionCondition, ExecutionCondition> {
    let mut proposals = BTreeMap::new();

    for (joined, source) in inputs {
        let Some(source) = corresponding_joined_source(joined, source) else { continue; };

        proposals.entry(source.clone()).and_modify(|previous| {
            if previous != joined { *previous = ExecutionCondition::unknown(); }
        }).or_insert_with(|| joined.clone());
    }

    proposals
}

fn corresponding_joined_source<'source>(
    joined: &ExecutionCondition,
    source: &'source ExecutionCondition,
) -> Option<&'source ExecutionCondition> {
    use crate::ExecutionConditionKind as Kind;

    let mut joined = joined;
    let original_source = source.computed_value();
    let mut source = original_source;
    let mut projected = false;

    loop {
        match (joined.kind(), source.kind()) {
            (Kind::Projection(joined_field, joined_base), Kind::Projection(source_field, source_base))
                if joined_field == source_field => {
                    joined = joined_base;
                    source = source_base;
                    projected = true;
                }
            (Kind::Joined(..), Kind::Constructed(base, _))
                if matches!(base.kind(), Kind::PostState(..) | Kind::Joined(..)) => {
                    return Some(if projected { original_source } else { base });
                }
            (Kind::Joined(..), Kind::PostState(..) | Kind::Joined(..)) => {
                return Some(original_source);
            }
            _ => return None,
        }
    }
}

fn inactive_guards(
    guards: &BTreeSet<&ExecutionCondition>,
    inputs: [&SnapshotSubstitution<'_>; 2],
    proofs: &[ExecutionProof<'_>; 2],
    facts: &mut Vec<(ExecutionCondition, bool)>,
) -> BTreeSet<ExecutionCondition> {
    let mut inactive = BTreeSet::new();

    for &guard in guards {
        let negated = ExecutionCondition::operation(BoundOperator::LogicalNot, vec![guard.clone()]);

        if proofs.iter().zip(inputs).all(|(proof, inputs)| {
            proof.prove_trusted(&inputs.apply(&negated)) == Some(true)
        }) {
            // Both actual predecessors establish the ordinary fact, independently of any owner.
            // Keeping it once avoids manufacturing every vacuously guarded carrier promise.
            inactive.insert(guard.clone());
            facts.push((guard.clone(), false));
        }
    }

    inactive
}

fn conditional_aliases(
    sources: [(
        &ExecutionState,
        &ExecutionSubstitution<'_>,
    ); 2],
    inputs: [&SnapshotSubstitution<'_>; 2],
    guards: &BTreeSet<&ExecutionCondition>,
    inactive: &BTreeSet<ExecutionCondition>,
    proofs: &[ExecutionProof<'_>; 2],
    facts: &mut Vec<(ExecutionCondition, bool)>,
) {
    let (stable, _) = super::merge::stable_observations(sources[0].0, sources[1].0);

    let mut aliases = BTreeSet::new();

    let proved = |condition: &ExecutionCondition| proofs.iter().zip(inputs).all(|(proof, inputs)| {
        proof.prove_trusted(&inputs.apply(condition)) == Some(true)
    });

    for value in stable.into_iter().take(ExecutionCondition::WORK_LIMIT) {
        let Some((_, copied)) = value.storage_contents() else { continue; };

        if copied.is_reference() || copied.observes_live(None) { continue; }

        for (_, substitution) in sources {
            let target = substitution.value(copied);

            if target == *copied || target.is_reference() { continue; }

            let alias = ExecutionCondition::value_alias(copied.clone(), target);

            if matches!(alias.kind(), crate::ExecutionConditionKind::Boolean(_) | crate::ExecutionConditionKind::Unknown) { continue; }

            if proved(&alias) {
                aliases.insert(alias);
                continue;
            }

            for guard in guards.iter().take(ExecutionCondition::WORK_LIMIT) {
                if inactive.contains(*guard) { continue; }

                let clause = ExecutionCondition::operation(BoundOperator::LogicalOr, vec![
                    ExecutionCondition::operation(BoundOperator::LogicalNot, vec![(*guard).clone()]),
                    alias.clone(),
                ]);

                if proved(&clause) { aliases.insert(clause); }
            }
        }
    }

    facts.extend(aliases.into_iter().map(|condition| (condition, true)));
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    use bray_bound_tree::BoundOperator;
    use bray_symbols::SymbolOrdinal;

    use super::conditional_facts;
    use crate::analysis::guarantee::state::ExecutionState;
    use crate::{ExecutionCondition, ExecutionPlace};

    #[test]
    fn join_publication_retains_carrier_edges_but_reads_current_evidence() {
        let input = |ordinal| ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal))));
        let owner = input(0);
        let address = input(1);
        let claim = ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(address.clone())));
        let first = ExecutionCondition::operation(BoundOperator::LogicalOr, vec![input(2), claim.clone()]);
        let last = ExecutionCondition::operation(BoundOperator::LogicalOr, vec![input(3), first.clone()]);
        let dependency = ExecutionPlace::argument(SymbolOrdinal::new(1));
        let unrelated = ExecutionPlace::argument(SymbolOrdinal::new(9));

        for available in [false, true] {
            let mut left = ExecutionState::default();
            let mut right = ExecutionState::default();

            left.carry_guarantee(owner.clone(), claim.clone());

            let source = Arc::make_mut(left.witnesses.get_mut(&owner).expect("owner retains its guarantee"));

            source.available = available;
            source.dependencies.insert(dependency.clone());

            source.conditions.get_mut(&claim).expect("carried clause exists")
                .addresses.insert(address.clone(), BTreeSet::from([dependency.clone()]));

            for state in [&mut left, &mut right] {
                state.carry_guarantee(input(9), claim.clone());

                let outside = Arc::make_mut(state.witnesses.get_mut(&input(9)).expect("independent carrier exists"));

                outside.dependencies.insert(unrelated.clone());

                outside.conditions.get_mut(&claim).expect("independent clause exists")
                    .addresses.insert(address.clone(), BTreeSet::from([unrelated.clone()]));
            }

            super::retain_conditional_guarantees(&mut left, &mut right, &[super::JoinedWitness {
                owner: owner.clone(), available,
                clauses: vec![(claim.clone(), first.clone(), true), (first.clone(), last.clone(), true)],
            }]);

            for state in [&left, &right] {
                let witness = &state.witnesses[&owner];

                assert_eq!(witness.available, available);
                assert!(witness.conditions[&last].witness, "the earlier clause supplies current evidence on the new owner");
                assert_eq!(witness.dependencies, BTreeSet::from([dependency.clone()]));
                assert_eq!(witness.conditions[&last].addresses[&address], BTreeSet::from([dependency.clone()]));
                assert_eq!(state.trusted_assumptions.contains(&(last.clone(), true)), available);
            }
        }
    }

    #[test]
    fn opposite_join_guards_do_not_manufacture_carrier_promises() {
        let input = |ordinal| ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal))));
        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let owner = input(0);
        let guard = input(1);
        let negated = ExecutionCondition::operation(BoundOperator::LogicalNot, vec![guard.clone()]);

        let claim = ExecutionCondition::operation(BoundOperator::LogicalOr, vec![
            negated.clone(), ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(input(2)))),
        ]);

        let mut left = ExecutionState::default();
        let mut right = ExecutionState::default();

        for state in [&mut left, &mut right] {
            state.assign(place.clone(), owner.clone(), None);
        }

        guard.clone().assume(false, &mut left.assumptions);
        guard.clone().assume(true, &mut right.assumptions);
        left.carry_guarantee(owner.clone(), claim.clone());
        claim.assume(true, &mut left.trusted_assumptions);

        let availability = BTreeSet::from([(owner.clone(), guard), (owner, negated)]);
        let empty = BTreeMap::new();
        let substitution = crate::execution_guarantees::ExecutionSubstitution::new(&empty, &empty);
        let mut facts = Vec::new();

        let retained = conditional_facts((&left, &substitution), (&right, &substitution),
            &BTreeMap::new(), [], &availability, None, &mut facts);

        assert!(retained.is_empty(), "an ordinary tautology establishes no carrier authority");
    }

    #[test]
    fn opposite_join_guards_recover_a_claim_proved_by_both_carriers() {
        let input = |ordinal| ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal))));
        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let owner = input(0);
        let guard = input(1);
        let trusted = ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(input(2))));

        let left_clause = ExecutionCondition::operation(BoundOperator::LogicalOr, vec![
            ExecutionCondition::operation(BoundOperator::LogicalNot, vec![guard.clone()]),
            trusted.clone(),
        ]);

        let right_clause = ExecutionCondition::operation(BoundOperator::LogicalOr, vec![
            guard.clone(), trusted.clone(),
        ]);

        let mut left = ExecutionState::default();
        let mut right = ExecutionState::default();

        for state in [&mut left, &mut right] {
            state.assign(place.clone(), owner.clone(), None);
        }

        guard.clone().assume(true, &mut left.assumptions);
        guard.clone().assume(false, &mut right.assumptions);
        left.carry_guarantee(owner.clone(), left_clause.clone());
        right.carry_guarantee(owner.clone(), right_clause.clone());
        left_clause.assume(true, &mut left.trusted_assumptions);
        right_clause.assume(true, &mut right.trusted_assumptions);

        let empty = BTreeMap::new();
        let substitution = crate::execution_guarantees::ExecutionSubstitution::new(&empty, &empty);
        let mut facts = Vec::new();

        let retained = conditional_facts((&left, &substitution), (&right, &substitution),
            &BTreeMap::new(), [], &BTreeSet::new(), None, &mut facts);

        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].owner, owner);
        assert!(retained[0].clauses.contains(&(trusted.clone(), trusted.clone(), true)));

        super::retain_conditional_guarantees(&mut left, &mut right, &retained);

        for state in [&left, &right] {
            assert!(state.witnesses[&owner].conditions[&trusted].witness);
        }
    }

    #[test]
    fn common_inactive_guards_publish_truth_without_phantom_carrier_promises() {
        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let owner = ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone()));
        let guard = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(1))));
        let claim = ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(2)))))));
        let mut left = ExecutionState::default();
        let mut right = ExecutionState::default();

        for state in [&mut left, &mut right] {
            state.assign(place.clone(), owner.clone(), None);
            guard.clone().assume(false, &mut state.assumptions);
        }

        left.carry_guarantee(owner.clone(), claim.clone());
        claim.assume(true, &mut left.trusted_assumptions);

        let availability = BTreeSet::from([
            (owner.clone(), guard.clone()),
            (owner, ExecutionCondition::operation(BoundOperator::LogicalNot, vec![guard.clone()])),
        ]);

        let empty = BTreeMap::new();
        let mut facts = Vec::new();

        let retained = conditional_facts((&left, &crate::execution_guarantees::ExecutionSubstitution::new(&empty, &empty)), (&right, &crate::execution_guarantees::ExecutionSubstitution::new(&empty, &empty)),
            &BTreeMap::new(), [], &availability, None, &mut facts);

        assert!(retained.is_empty());
        assert!(facts.contains(&(guard, false)));
    }
}

/// Keep the same proven clause on its complete carrier after identity renaming.
pub(super) fn retain_conditional_guarantees(
    left: &mut ExecutionState,
    right: &mut ExecutionState,
    conditions: &[JoinedWitness],
) {
    for JoinedWitness { owner, available, clauses } in conditions {
        // Publication can insert this owner, but cannot change the retained value graph.
        let carriers = [&*left, &*right].map(|state| {
            let roots = state.witness_snapshots([owner.clone()]);

            state.witnesses.keys().chain(std::iter::once(owner))
                .filter(|carrier| roots.retains_owner(carrier))
                .cloned().collect::<BTreeSet<_>>()
        });

        for (original, condition, owning) in clauses {
            let mut guarantee = crate::execution_guarantees::ExecutionGuarantee::default();
            let mut dependencies = BTreeSet::new();

            for (state, carriers) in [&*left, &*right].into_iter().zip(&carriers) {
                // Read current evidence so earlier clauses keep their original publication effects.
                for witness in carriers.iter().filter_map(|carrier| state.witnesses.get(carrier)) {
                    dependencies.extend(witness.dependencies.iter().cloned());

                    guarantee.witness |= witness
                        .conditions
                        .get(original)
                        .is_some_and(|bound| bound.witness);

                    // A predecessor can prove the clause through a differently guarded
                    // premise. Preserve every matching address bound on that carrier.
                    for source in witness.conditions.values() {
                        for (address, owners) in source
                            .addresses
                            .iter()
                            .filter(|(address, _)| condition.observes_trusted(address))
                        {
                            guarantee
                                .addresses
                                .entry(address.clone())
                                .or_default()
                                .extend(owners.iter().cloned());
                        }
                    }
                }
            }

            guarantee.witness |= *owning;

            for state in [&mut *left, &mut *right] {
                let witness = state.witnesses.entry(owner.clone()).or_default();

                if !dependencies.is_subset(&witness.dependencies) {
                    std::sync::Arc::make_mut(witness)
                        .dependencies
                        .extend(dependencies.iter().cloned());
                }

                witness.carry(condition, guarantee.witness, &guarantee.addresses);

                if !available {
                    std::sync::Arc::make_mut(witness).available = false;
                } else {
                    condition
                        .clone()
                        .assume(true, &mut state.trusted_assumptions);
                }
            }
        }
    }
}

pub(in crate::analysis::guarantee) fn owned_alternatives(
    left: (&ExecutionState, &ExecutionCondition),
    right: (&ExecutionState, &ExecutionCondition),
    joined: &ExecutionCondition,
) -> bray_symbols::DependencyOwnedResult<bray_bound_tree::StorageProjection, ExecutionCondition> {
    let layouts = [left, right].map(|(state, source)| state.owned_result(source));

    if layouts.iter().all(bray_symbols::DependencyOwnedResult::is_empty) {
        return bray_symbols::DependencyOwnedResult::default();
    }

    let mut guards = BTreeSet::new();
    let mut inputs = [BTreeMap::new(), BTreeMap::new()];
    let absent = [left.0, right.0].map(absent_observations);

    collect_observations(
        [Some(left.1), Some(right.1)], joined, &mut inputs, &mut guards,
        [&absent[0], &absent[1]], &ExecutionCondition::boolean(true),
        &mut { ExecutionCondition::WORK_LIMIT },
    );

    bray_symbols::DependencyOwnedResult::choice([(left.0, right.0), (right.0, left.0)].into_iter().zip(inputs).zip(layouts).map(
        |(((state, other), inputs), mut result)| {
            let snapshots = SnapshotSubstitution::new(&inputs);

            for (condition, holds) in state.assumptions.difference(&other.assumptions) {
                if matches!(condition.kind(), crate::ExecutionConditionKind::Input(place) if place.is_callable_input()) {
                    let condition = if *holds { condition.clone() } else { ExecutionCondition::operation(BoundOperator::LogicalNot, vec![condition.clone()]) };

                    result = bray_symbols::DependencyOwnedResult::case(condition, result);
                }
            }

            for (_, guard) in &guards {
                if let Some(holds) = snapshots.apply(guard)
                    .prove_ordinary(&state.assumptions, Some(&state.proofs))
                {
                    let condition = if holds { guard.clone() } else {
                        ExecutionCondition::operation(BoundOperator::LogicalNot, vec![guard.clone()])
                    };

                    result = bray_symbols::DependencyOwnedResult::case(condition, result);
                }
            }

            result
        },
    ))
}

fn absent_observations(state: &ExecutionState) -> BTreeMap<ExecutionCondition, std::sync::Arc<bray_symbols::ConstantValueData>> {
    state.ordinary_proof().ordinary_facts()
        .filter_map(|(condition, holds)| condition.nullable_absence(*holds))
        .map(|(subject, absent)| (subject.clone(), std::sync::Arc::clone(absent)))
        .collect()
}

fn collect_observations(
    sources: [Option<&ExecutionCondition>; 2],
    joined: &ExecutionCondition,
    inputs: &mut [BTreeMap<ExecutionCondition, ExecutionCondition>; 2],
    guards: &mut BTreeSet<(ExecutionCondition, ExecutionCondition)>,
    absent: [&BTreeMap<ExecutionCondition, std::sync::Arc<bray_symbols::ConstantValueData>>; 2],
    active: &ExecutionCondition,
    budget: &mut usize,
) {
    if sources.iter().all(Option::is_none) { return; }

    let Some(remaining) = budget.checked_sub(1) else { return; };

    *budget = remaining;

    if joined == &ExecutionCondition::unknown() { return; }

    if let crate::ExecutionConditionKind::Present(_) = joined.value_inside_capture().kind() {
        for (source, inputs) in sources.into_iter().zip(inputs.iter_mut()) {
            if let Some(source) = source { insert_snapshot(inputs, joined, source); }
        }

        let sources = sources.map(|source| source.map(|source|
            source.clone().project(bray_bound_tree::StorageProjection::NullableValue)));
        let payload = joined.clone().project(bray_bound_tree::StorageProjection::NullableValue);

        collect_observations([sources[0].as_ref(), sources[1].as_ref()], &payload,
            inputs, guards, absent, active, budget);

        return;
    }

    let Some((role, owner)) = joined.ownership_identity() else {
        if joined.observation_kind() == Some(ObservationKind::Scalar) {
            collect_reached_data(sources, joined, inputs, budget);
        }

        return;
    };

    let owner = owner.clone();
    let observed = std::cell::OnceCell::new();

    for (source, inputs) in sources.into_iter().zip(inputs.iter_mut()) {
        let Some(source) = source else { continue; };

        // A retained phi owner can still have different predecessor contents.
        // Proof inserts that existing snapshot once, without rewriting identity.
        if !matches!(owner.kind(), crate::ExecutionConditionKind::Unknown | crate::ExecutionConditionKind::Boolean(_) | crate::ExecutionConditionKind::Literal(_)) {
            insert_snapshot(inputs, &owner, source);
        }

        // Proof needs the complete predecessor value as well as its identity.
        // Rebuilding an opaque value from projected fields changes its snapshot.
        if role != ObservationKind::Reference {
            insert_snapshot(inputs, joined, source);

            if source.ownership_identity().is_some_and(|(role, _)| role == ObservationKind::Owned) {
                let observed = observed.get_or_init(|| joined.clone().borrowed().predicate_observation());

                if !observed.contains_unknown() {
                    let previous = source.clone().borrowed().predicate_observation();

                    insert_snapshot(inputs, observed, &previous);
                }
            }
        }
    }

    if role == ObservationKind::Reference {
        collect_reached_data(sources, joined, inputs, budget);
        return;
    }

    if let crate::ExecutionConditionKind::Observation { contents, kind: ObservationKind::Owned, .. } = joined.value_inside_capture().kind()
        && !matches!(contents.kind(), crate::ExecutionConditionKind::Unknown | crate::ExecutionConditionKind::Literal(_) | crate::ExecutionConditionKind::Boolean(_))
    {
        if std::env::var_os("BRAY_BRA694_TRACE").is_some()
            && format!("{contents:?}").contains("Joined(Block(BoundUnitId(2848653), 162)")
        {
            eprintln!("BRA694 OWNED JOIN contents={} sources={:?}",
                format!("{contents:?}").chars().take(950).collect::<String>(),
                sources.map(|source| source.map(|source| format!("{:?}", source.kind()).chars().take(1050).collect::<String>())));
        }
        // Current contents need no independent owner. Keep this actual carrier
        // while matching its data against the corresponding predecessor data.
        for (source, inputs) in sources.into_iter().zip(inputs.iter_mut()) {
            if let Some(source) = source {
                insert_reached_snapshot(inputs, contents, source.computed_value(), false);
            }
        }
    }

    // Payload authority is conditional on every enclosing active case. A test
    // of an inner tag alone has no meaning on an inactive outer payload.
    let presence = sources.into_iter().zip(absent).find_map(|(source, absent)| {
        let source = source?;

        source.known_absence().or_else(|| absent.get(source))
            .or_else(|| absent.get(source.owner_identity()))
    }).map(|absent| ExecutionCondition::operation(
        BoundOperator::LogicalAnd,
        vec![active.clone(), ExecutionCondition::operation(
            BoundOperator::NotEqual,
            vec![joined.clone(), ExecutionCondition::new(crate::ExecutionConditionKind::Literal(std::sync::Arc::clone(absent)))],
        )],
    ));

    if let Some(presence) = &presence { insert_guard(guards, &owner, presence.clone()); }

    let mut fields = BTreeSet::new();

    for source in sources.into_iter().flatten().map(ExecutionCondition::computed_value) {
        if let Some(variant) = source.known_variant() {
            insert_guard(guards, &owner, ExecutionCondition::operation(
                BoundOperator::LogicalAnd, vec![active.clone(), joined.clone().matches_variant(variant)],
            ));
        }

        match source.kind() {
            crate::ExecutionConditionKind::Constructed(_, components) => {
                fields.extend(components.iter().map(|(field, _)| *field));
            }
            crate::ExecutionConditionKind::Present(_) => { fields.insert(bray_bound_tree::StorageProjection::NullableValue); }
            _ => {}
        }
    }

    for field in fields {
        let active = match field {
            bray_bound_tree::StorageProjection::ActiveUnionPayloadField { variant, .. } => {
                ExecutionCondition::operation(
                    BoundOperator::LogicalAnd, vec![active.clone(), joined.clone().matches_variant(variant)],
                )
            }
            bray_bound_tree::StorageProjection::NullableValue => presence.clone().unwrap_or_else(|| active.clone()),
            _ => active.clone(),
        };

        if matches!(field, bray_bound_tree::StorageProjection::ActiveUnionPayloadField { .. }) {
            insert_guard(guards, &owner, active.clone());
        }

        let values = sources.map(|source| source.map(|source| source.clone().project(field)));
        let selected = joined.clone().project(field);

        if let [Some(left), Some(right)] = values.each_ref().map(Option::as_ref)
            && let (crate::ExecutionConditionKind::Boolean(left), crate::ExecutionConditionKind::Boolean(right)) =
                (left.computed_value().kind(), right.computed_value().kind())
            && left != right
            && !selected.computed_value().contains_unknown()
        {
            let predicate = selected.computed_value().clone();
            let guard = ExecutionCondition::operation(BoundOperator::LogicalAnd, vec![active.clone(), predicate.clone()]);

            insert_guard(guards, &owner, guard);

            if !matches!(active.kind(), crate::ExecutionConditionKind::Boolean(true)) {
                let opposite = ExecutionCondition::operation(BoundOperator::LogicalNot, vec![predicate]);

                insert_guard(guards, &owner, ExecutionCondition::operation(
                    BoundOperator::LogicalAnd, vec![active.clone(), opposite],
                ));
            }
        }

        collect_observations(
            [values[0].as_ref(), values[1].as_ref()], &selected, inputs, guards, absent,
            &active, budget,
        );
    }
}

fn collect_reached_data(
    sources: [Option<&ExecutionCondition>; 2],
    joined: &ExecutionCondition,
    inputs: &mut [BTreeMap<ExecutionCondition, ExecutionCondition>; 2],
    budget: &mut usize,
) {
    if sources.iter().all(Option::is_none) { return; }

    let Some(remaining) = budget.checked_sub(1) else { return; };

    *budget = remaining;

    let joined = joined.computed_value();
    let sources = sources.map(|source| source.map(ExecutionCondition::computed_value));

    if matches!(joined.kind(), crate::ExecutionConditionKind::Unknown | crate::ExecutionConditionKind::Literal(_) | crate::ExecutionConditionKind::Boolean(_)) {
        return;
    }

    // These substitutions instantiate an actual predecessor's reached data
    // during proof. They do not create an ownership mapping or another origin.
    for (source, inputs) in sources.into_iter().zip(inputs.iter_mut()) {
        let Some(source) = source else { continue; };

        insert_reached_snapshot(inputs, joined, source, true);
    }

    if let crate::ExecutionConditionKind::Constructed(_, fields) = joined.kind() {
        for (field, value) in fields.iter() {
            let sources = sources.map(|source| source.map(|source| source.clone().project(*field)));

            collect_reached_data([sources[0].as_ref(), sources[1].as_ref()], value, inputs, budget);
        }
    }
}

fn insert_reached_snapshot(
    inputs: &mut BTreeMap<ExecutionCondition, ExecutionCondition>,
    joined: &ExecutionCondition,
    source: &ExecutionCondition,
    include_owner_base: bool,
) {
    insert_snapshot(inputs, joined, source);

    // An unlisted component projects through the constructed value's base.
    // A joined base is a data identity, so only the actual predecessor snapshot
    // can answer that projection. This map grants no ownership on its own.
    if let crate::ExecutionConditionKind::Constructed(base, _) = joined.kind()
        && (include_owner_base && base.ownership_identity().is_some()
            || matches!(base.kind(), crate::ExecutionConditionKind::Joined(..))
                && std::env::var_os("BRAY_BRA694_DISABLE_JOINED_BASE").is_none())
    {
        if std::env::var_os("BRAY_BRA694_TRACE").is_some()
            && matches!(base.kind(), crate::ExecutionConditionKind::Joined(..))
            && ["BoundUnitId(392598)", "BoundUnitId(2848653)"].iter().any(|unit| format!("{base:?}").contains(unit))
        {
            eprintln!("BRA694 BASE MAP base={base:?} source={:?}", source.kind());
        }
        insert_snapshot(inputs, base, source);
    }
}

fn insert_snapshot(
    inputs: &mut BTreeMap<ExecutionCondition, ExecutionCondition>,
    joined: &ExecutionCondition, source: &ExecutionCondition,
) {
    // Predicates already observe the logical value. Physical capture coordinates
    // belong to storage transport and cannot make two proof operands disagree.
    let joined = joined.without_captured_storage();
    let source = source.without_captured_storage();

    inputs.entry(joined.clone()).and_modify(|previous| {
        if *previous == source { return; }

        // An identity pin contributes no competing contents for the same owner.
        if *previous == joined && source.has_same_owner(&joined) {
            *previous = source.clone();
        } else if source != joined || !previous.has_same_owner(&joined) {
            *previous = ExecutionCondition::unknown();
        }
    }).or_insert(source);
}

fn insert_guard(
    guards: &mut BTreeSet<(ExecutionCondition, ExecutionCondition)>,
    owner: &ExecutionCondition,
    guard: ExecutionCondition,
) {
    if !matches!(guard.kind(), crate::ExecutionConditionKind::Boolean(_) | crate::ExecutionConditionKind::Unknown) {
        guards.insert((owner.clone(), guard.clone()));
        guards.insert((owner.clone(), ExecutionCondition::operation(BoundOperator::LogicalNot, vec![guard])));
    }
}
