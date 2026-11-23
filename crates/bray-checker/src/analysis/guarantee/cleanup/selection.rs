use super::super::current::ValueAvailability;
use super::super::flow::{ExecutionFlowDomain, ExecutionJoin, merge_execution};
use super::super::state::ExecutionState;
use super::plan::Selection;
use crate::{CheckerRequestContext, ExecutionCondition, ExecutionPlace};
use std::borrow::Cow;

pub(super) type ReplacementPath<'a> = (&'a bray_bound_tree::StoragePath, Option<&'a bray_bound_tree::StoragePath>);

pub(super) fn cleanup_path<'a>(
    path: &'a bray_bound_tree::StoragePath,
    replacement: Option<ReplacementPath<'_>>,
) -> Option<Cow<'a, bray_bound_tree::StoragePath>> {
    let Some((source, reached)) = replacement else { return Some(Cow::Borrowed(path)); };

    let reached = reached?;

    if source == reached { return Some(Cow::Borrowed(path)); }

    path.rebase(source, reached).map(Cow::Owned)
}

pub(super) fn visit_cleanup_parts<E>(
    values: &bray_symbols::SemanticValueStore,
    parts: &[crate::asynchronous::ResolvedCleanupPart],
    mut visit: impl FnMut(bray_symbols::TypeId, Option<&bray_bound_tree::StorageCleanupPart>, &[bray_symbols::SymbolOrdinal], Option<usize>) -> Result<(), E>,
) -> Result<(), E> {
    visit_cleanup_group(values, parts, 0, ExecutionCondition::WORK_LIMIT, &mut Vec::new(), &mut visit)
}

fn visit_cleanup_group<E>(
    values: &bray_symbols::SemanticValueStore,
    mut parts: &[crate::asynchronous::ResolvedCleanupPart],
    depth: usize,
    budget: usize,
    indices: &mut Vec<bray_symbols::SymbolOrdinal>,
    visit: &mut impl FnMut(bray_symbols::TypeId, Option<&bray_bound_tree::StorageCleanupPart>, &[bray_symbols::SymbolOrdinal], Option<usize>) -> Result<(), E>,
) -> Result<(), E> {
    use bray_bound_tree::{StorageCleanupPart, StorageCleanupProjectionKind};
    use bray_symbols::SymbolOrdinal;

    while let Some((ty, part)) = parts.first() {
        let projection = |part: Option<&StorageCleanupPart>| part.and_then(|part| part.projections().get(depth).copied());

        let Some(first) = projection(part.as_ref()) else {
            visit(*ty, part.as_ref(), indices, None)?;
            parts = &parts[1..];
            continue;
        };

        let count = parts.iter().take_while(|(_, part)| projection(part.as_ref()) == Some(first)).count();

        let (group, remaining) = parts.split_at(count);

        if let StorageCleanupProjectionKind::ArrayElements(_) = first.projection() {
            let extent = crate::constant::known_array_length(values, first.source_type())
                .filter(|length| length.checked_mul(group.len()).is_some_and(|work| work <= budget));

            if let Some(extent) = extent {
                for index in (0..extent).rev() {
                    indices.push(SymbolOrdinal::new(u32::try_from(index)
                        .expect("bounded array cleanup index fits an ordinal")));

                    visit_cleanup_group(values, group, depth + 1, budget / extent, indices, visit)?;
                    indices.pop();
                }
            } else {
                for (index, (ty, part)) in group.iter().enumerate() {
                    let repeat = (index + 1 == group.len()).then_some(group.len());

                    visit(*ty, part.as_ref(), indices, repeat)?;
                }
            }
        } else {
            visit_cleanup_group(values, group, depth + 1, budget, indices, visit)?;
        }

        parts = remaining;
    }

    Ok(())
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(super) fn cleanup_part_condition(
        &self,
        state: &ExecutionState,
        selection: &Selection,
        definitely_absent: &impl Fn(&bray_bound_tree::StoragePath) -> bool,
        replacement: Option<ReplacementPath<'_>>,
    ) -> ExecutionCondition {
        let owner = selection.owner.as_ref().and_then(|path| cleanup_path(path, replacement));

        if owner.as_deref().is_some_and(definitely_absent) {
            return ExecutionCondition::boolean(false);
        }

        let place = owner.as_deref().and_then(|path| ExecutionPlace::storage_path(self.storage, path));
        let available = place.as_ref().map(|place| state.current.availability(place));

        let initialized = match available {
            Some(ValueAvailability::Absent) if selection.route.is_some()
                || place.is_some_and(|place| state.current.is_subtree_absent(&place)) => {
                return ExecutionCondition::boolean(false);
            }
            Some(ValueAvailability::Guarded(guard)) => guard,
            Some(ValueAvailability::Present) if selection.route.is_some() => ExecutionCondition::boolean(true),
            _ => ExecutionCondition::unknown(),
        };

        let initialized = if selection.route.is_some() {
            initialized
        } else {
            ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalAnd, vec![initialized, ExecutionCondition::unknown()])
        };

        selection.guards.iter().fold(initialized, |condition, (path, operator, expected)| {
            let guard = cleanup_path(path, replacement).map_or_else(ExecutionCondition::unknown, |path| {
                ExecutionCondition::pattern(self.storage_path_value(state, &path), *operator, expected.clone())
            });

            ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalAnd, vec![condition, guard])
        })
    }

}

/// Effects, completion facts and retirement belong to the same selected part.
pub(super) fn select_cleanup(
    state: &mut ExecutionState,
    condition: ExecutionCondition,
    action: impl FnOnce(&mut ExecutionState, bool) -> bool,
) -> bool {
    match condition.prove_ordinary(&state.assumptions, Some(&state.proofs)) {
        Some(false) => true,
        Some(true) => action(state, false),
        None => {
            let mut skipped = state.clone();

            condition.clone().assume(false, &mut skipped.assumptions);
            condition.clone().assume(true, &mut state.assumptions);
            skipped.refine_witnesses();
            state.refine_witnesses();

            if action(state, true) {
                let mut joined = Some(std::mem::take(state));

                merge_execution(&mut joined, &Some(skipped), ExecutionJoin::Selection(&condition));
                *state = joined.expect("conditional cleanup retains its normal continuation");
            } else {
                *state = skipped;
            }

            true
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{ExecutionCondition, ExecutionPlace, ExecutionState, ValueAvailability, select_cleanup};

    #[test]
    fn unknown_cleanup_cannot_publish_unconditional_completion_or_retirement() {
        let place = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0));
        let value = ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone()));
        let fact = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(1))));

        for condition in [ExecutionCondition::boolean(false), ExecutionCondition::boolean(true), ExecutionCondition::unknown()] {
            let mut state = ExecutionState::default();

            state.assign(place.clone(), value.clone(), None);

            let mut calls = 0;

            select_cleanup(&mut state, condition.clone(), |state, conditional| {
                calls += 1;
                assert_eq!(conditional, condition == ExecutionCondition::unknown());
                fact.clone().assume(true, &mut state.assumptions);
                state.current.retire(&place);

                true
            });

            let definite = condition == ExecutionCondition::boolean(true);

            assert_eq!(calls, usize::from(condition != ExecutionCondition::boolean(false)));
            assert_eq!(fact.prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT }), definite.then_some(true));
            assert_eq!(state.current.availability(&place) == ValueAvailability::Absent, definite);
        }
    }

    #[test]
    fn established_guard_alias_selects_cleanup_without_forking() {
        let place = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0));
        let prior = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(1))));
        let current = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(2))));

        for active in [false, true] {
            let mut state = ExecutionState::default();
            let mut calls = 0;

            state.assign(place.clone(), ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone())), None);
            prior.clone().assume(active, &mut state.assumptions);
            ExecutionCondition::value_alias(prior.clone(), current.clone()).assume(true, &mut state.assumptions);

            select_cleanup(&mut state, current.clone(), |state, conditional| {
                assert!(!conditional, "known copied guards must not create another control-flow choice");
                calls += 1;
                state.current.retire(&place);

                true
            });

            assert_eq!(calls, usize::from(active));
            assert_eq!(state.current.availability(&place) == ValueAvailability::Absent, active);
        }
    }

    #[test]
    fn selected_cleanup_keeps_one_choice_and_its_carrier_authority() {
        let input = |index| ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(index))));
        let place = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0));
        let owner = input(0);
        let selector = input(1);
        let claim = ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(input(2))));
        let unrelated = ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(input(3))));
        let mut state = ExecutionState::default();

        state.assign(place.clone(), owner.clone(), None);
        state.carry_guarantee(owner.clone(), claim.clone());
        claim.clone().assume(true, &mut state.trusted_assumptions);

        for index in 10..60 {
            ExecutionCondition::value_alias(selector.clone(), input(index)).assume(true, &mut state.assumptions);
        }

        select_cleanup(&mut state, selector.clone(), |state, conditional| {
            assert!(conditional);
            state.current.retire(&place);
            state.witnesses.remove(&owner);
            state.trusted_assumptions.clear();

            true
        });

        assert!(state.witnesses[&owner].conditions.len() <= 4,
            "derived copies of one selector must not multiply carrier clauses");

        assert_ne!(claim.prove_trusted(&state.trusted_assumptions, &state.assumptions), Some(true));

        for selected in [false, true] {
            let mut branch = state.clone();

            selector.clone().assume(selected, &mut branch.assumptions);
            branch.refine_witnesses();

            assert_eq!(claim.prove_trusted(&branch.trusted_assumptions, &branch.assumptions) == Some(true), !selected);
            assert_ne!(unrelated.prove_trusted(&branch.trusted_assumptions, &branch.assumptions), Some(true));
        }
    }
}
