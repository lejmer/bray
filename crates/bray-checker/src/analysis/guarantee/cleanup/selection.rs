use super::super::current::ValueAvailability;
use super::super::flow::{ExecutionFlowDomain, ExecutionJoin, merge_execution};
use super::super::state::ExecutionState;
use super::plan::Step;
use crate::{CheckerRequestContext, ExecutionCondition, ExecutionPlace};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    /// Cancellation visits only represented parts that exist on this execution path.
    pub(super) fn cancellation_condition(
        &self,
        state: &ExecutionState,
        access: bray_bound_tree::StorageAccessId,
    ) -> ExecutionCondition {
        self.disposal.plan.steps.get(&access)
            .expect("selected cancellation has prepared cleanup parts")
            .iter()
            .map(|step| self.cleanup_part_condition(state, step))
            .fold(ExecutionCondition::Boolean(false), |condition, part| {
                ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalOr, vec![condition, part])
            })
    }

    pub(super) fn cleanup_part_condition(
        &self,
        state: &ExecutionState,
        step: &Step,
    ) -> ExecutionCondition {
        let (path, guards) = match step {
            Step::Opaque { path, guards, .. } | Step::Lifecycle { path, guards, .. } => (path, guards),
        };

        let available = path.as_ref().and_then(|path| ExecutionPlace::storage_path(self.storage, path))
            .map(|place| state.current.availability(&place));

        let initialized = match available {
            Some(ValueAvailability::Absent) => ExecutionCondition::Boolean(false),
            Some(ValueAvailability::Guarded(guard)) => guard,
            Some(ValueAvailability::Present) => ExecutionCondition::Boolean(true),
            Some(ValueAvailability::Unknown) | None => ExecutionCondition::Unknown,
        };

        guards.iter().fold(initialized, |condition, (path, operator, expected)| {
            let guard = ExecutionCondition::pattern(self.storage_path_value(state, path), *operator, expected.clone());

            ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalAnd, vec![condition, guard])
        })
    }

}

/// Effects, completion facts and retirement belong to the same selected part.
pub(super) fn select_cleanup(
        state: &mut ExecutionState,
        condition: ExecutionCondition,
        action: impl FnOnce(&mut ExecutionState, bool),
    ) {
        match condition.prove_ordinary(&state.assumptions, Some(&state.proofs)) {
            Some(false) => {}
            Some(true) => action(state, false),
            None => {
                let mut skipped = state.clone();

                condition.clone().assume(false, &mut skipped.assumptions);
                condition.clone().assume(true, &mut state.assumptions);
                skipped.refine_witnesses();
                state.refine_witnesses();
                action(state, true);

                let mut joined = Some(std::mem::take(state));

                merge_execution(&mut joined, &Some(skipped), ExecutionJoin::Selection(&condition));
                *state = joined.expect("conditional cleanup retains its normal continuation");
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
        let value = ExecutionCondition::Input(place.clone());
        let fact = ExecutionCondition::Input(ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(1)));

        for condition in [ExecutionCondition::Boolean(false), ExecutionCondition::Boolean(true), ExecutionCondition::Unknown] {
            let mut state = ExecutionState::default();

            state.assign(place.clone(), value.clone(), None);

            let mut calls = 0;

            select_cleanup(&mut state, condition.clone(), |state, conditional| {
                calls += 1;
                assert_eq!(conditional, condition == ExecutionCondition::Unknown);
                fact.clone().assume(true, &mut state.assumptions);
                state.current.retire(&place);
            });

            let definite = condition == ExecutionCondition::Boolean(true);

            assert_eq!(calls, usize::from(condition != ExecutionCondition::Boolean(false)));
            assert_eq!(fact.prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT }), definite.then_some(true));
            assert_eq!(state.current.availability(&place) == ValueAvailability::Absent, definite);
        }
    }

    #[test]
    fn established_guard_alias_selects_cleanup_without_forking() {
        let place = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0));
        let prior = ExecutionCondition::Input(ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(1)));
        let current = ExecutionCondition::Input(ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(2)));

        for active in [false, true] {
            let mut state = ExecutionState::default();
            let mut calls = 0;

            state.assign(place.clone(), ExecutionCondition::Input(place.clone()), None);
            prior.clone().assume(active, &mut state.assumptions);
            ExecutionCondition::value_alias(prior.clone(), current.clone()).assume(true, &mut state.assumptions);
            select_cleanup(&mut state, current.clone(), |state, conditional| {
                assert!(!conditional, "known copied guards must not create another control-flow choice");
                calls += 1;
                state.current.retire(&place);
            });

            assert_eq!(calls, usize::from(active));
            assert_eq!(state.current.availability(&place) == ValueAvailability::Absent, active);
        }
    }

    #[test]
    fn selected_cleanup_keeps_one_choice_and_its_carrier_authority() {
        let input = |index| ExecutionCondition::Input(ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(index)));
        let place = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0));
        let owner = input(0);
        let selector = input(1);
        let claim = ExecutionCondition::Trusted(Arc::new(input(2)));
        let unrelated = ExecutionCondition::Trusted(Arc::new(input(3)));
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
