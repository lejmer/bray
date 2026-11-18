use crate::execution_guarantees::ExecutionSubstitution;
use std::collections::BTreeSet;
use std::sync::Arc;

use crate::ExecutionCondition;

type Selection = BTreeSet<(ExecutionCondition, bool)>;

pub(in crate::analysis) use crate::execution_guarantees::ExecutionDeferredValue as DeferredValue;
pub(super) use crate::execution_guarantees::ExecutionDeferredInvocation as DeferredInvocation;
pub(super) use crate::execution_guarantees::{deferred_selection as selection, deferred_possible as possible};

impl DeferredValue {
    pub(super) fn join_values(
        &mut self,
        substitution: &ExecutionSubstitution<'_>,
        current: &super::current::CurrentValues,
        context: Option<&Selection>,
    ) {
        let current_fact = |condition: &ExecutionCondition| {
            !condition.observations().any(|observed| matches!(observed.kind(), crate::ExecutionConditionKind::ScalarValue(_))
                && observed.selector_place().is_some_and(|place| current.live_value(&place)
                    .is_none_or(|value| value.copied_value() != observed)))
        };

        let context = context.map(|facts| facts.iter().filter(|(condition, _)| current_fact(condition))
            .cloned().collect::<Selection>());

        for guard in self.unknown.iter_mut().map(|candidate| &mut candidate.selection).chain(
            self.producers
                .values_mut()
                .map(|candidate| &mut candidate.selection),
        ) {
            guard.retain(|(condition, _)| current_fact(condition));

            if let Some(context) = &context {
                guard.extend(selection(context));
            }

            if !substitution.has_changes() {
                continue;
            }

            *guard = guard
                .iter()
                .flat_map(|(condition, holds)| {
                    let mut facts = BTreeSet::new();

                    substitution.value(condition)
                        .assume(*holds, &mut facts);

                    facts
                })
                .collect();
        }

        for candidate in self.producers.values_mut() {
            let captured = context.as_ref().map(|context| candidate.entry.captured_facts(context)).unwrap_or_default();

            let stale = candidate.entry.copy_sources.values().any(|source| !current_fact(source))
                || candidate.entry.assumptions.iter().chain(&candidate.entry.trusted_assumptions)
                    .any(|(condition, _)| !current_fact(condition));

            if stale || !captured.is_subset(&candidate.entry.assumptions) {
                let entry = Arc::make_mut(&mut candidate.entry);

                entry.copy_sources.retain(|_, source| current_fact(source));
                entry.assumptions.retain(|(condition, _)| current_fact(condition));
                entry.trusted_assumptions.retain(|(condition, _)| current_fact(condition));
                entry.assumptions.extend(captured);
            }

            if let Some(entry) = candidate.entry.joined_values(substitution) {
                candidate.entry = Arc::new(entry);
            }
        }
    }

}

#[cfg(test)]
mod tests {
    use super::{DeferredInvocation, DeferredValue};
    use crate::analysis::guarantee::state::ExecutionState;
    use crate::test_support::{error_type, expression_unit, push_expression};
    use crate::{ExecutionCallEvidence, ExecutionCondition, ExecutionPlace};
    use bray_bound_tree::{BoundErrorExpression, BoundExpression, BoundUnitId};
    use bray_symbols::SymbolOrdinal;
    use std::sync::Arc;

    fn carried(state: &ExecutionState) -> &DeferredValue {
        state.current.values().find_map(|value| value.deferred_value().map(Arc::as_ref))
            .expect("test state retains its actual frame value")
    }

    #[test]
    fn frame_moves_freeze_copy_guards_and_do_not_import_old_loop_incarnations() {
        let (_, expressions) = expression_unit(BoundUnitId::new(92), |tree, origin| {
            [0, 1].map(|_| push_expression(tree, BoundExpression::Error(BoundErrorExpression::new(origin, error_type())))).to_vec()
        });

        let owner = ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expressions[0]));
        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let source = ExecutionCondition::new(crate::ExecutionConditionKind::ScalarValue(Arc::new(ExecutionCondition::new(crate::ExecutionConditionKind::Joined(BoundUnitId::new(92), 6, place.clone())))));

        let captured = ExecutionCondition::opaque_scalar(ExecutionCondition::new(
            crate::ExecutionConditionKind::Expression(expressions[1])));

        let mut state = ExecutionState::default();

        state.current.insert(place.clone(), source.clone());
        state.assumptions.insert((source.clone(), true));

        state.current.insert(ExecutionPlace::argument(SymbolOrdinal::new(2)), owner.clone().with_deferred_value(Arc::new(DeferredValue {
            producers: [(expressions[0].into(), DeferredInvocation { callee: None, transition: None,
                entry: Arc::new(ExecutionCallEvidence {
                    outputs: Default::default(),
                    arguments: [(place.clone(), captured.clone())].into(),
                    copy_sources: [(place.clone(), source.clone())].into(), pending_execution: true, ..Default::default()
                }), completed: None, reports: Default::default(), selection: Default::default(),
            })].into(),
            unknown: Some(DeferredInvocation { callee: None, transition: None, entry: (), completed: None, reports: Default::default(), selection: [(source.clone(), true)].into() }),
        })));

        let middle = ExecutionCondition::result();

        state.join_values(&crate::execution_guarantees::ExecutionSubstitution::new(&[(owner.clone(), middle.clone())].into(), &[(owner, middle.clone())].into()));
        assert!(carried(&state).producers[&expressions[0].into()].entry.assumptions.contains(&(captured.clone(), true)));
        assert!(carried(&state).producers[&expressions[0].into()].selection.contains(&(source.clone(), true)));

        state.current.insert(place, ExecutionCondition::boolean(false));

        state.join_values(&crate::execution_guarantees::ExecutionSubstitution::new(&[(middle.clone(), middle.clone())].into(), &[(middle.clone(), middle.clone())].into()));

        let unchanged = &carried(&state).producers[&expressions[0].into()];

        assert!(!unchanged.selection.contains(&(source.clone(), true)), "identity joins still prune stale selectors");
        assert!(unchanged.entry.copy_sources.is_empty(), "identity joins still revoke stale copy origins");

        let target = ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expressions[1]));

        state.join_values(&crate::execution_guarantees::ExecutionSubstitution::new(&[(middle.clone(), target.clone())].into(), &[(middle, target.clone())].into()));

        let moved = &carried(&state).producers[&expressions[0].into()];

        assert!(!moved.selection.contains(&(source.clone(), true)));
        assert!(!moved.entry.assumptions.contains(&(source, true)));
        assert!(carried(&state).unknown.as_ref().expect("opaque predecessor remains possible").selection.is_empty());
        assert!(moved.entry.copy_sources.is_empty());
        assert!(moved.entry.assumptions.contains(&(captured, true)));
    }

    #[test]
    fn joining_frame_reports_preserves_output_channels_and_predecessors() {
        use bray_bound_tree::CallReportOutput::{DeferredAbnormal, DeferredCompleted};
        use bray_symbols::{DependencyRequirement, DependencyRequirementKind, DependencySubject, DependencySubjectRoot};

        let requirement = |ordinal| DependencyRequirement::direct(
            DependencySubject::root(DependencySubjectRoot::Parameter(SymbolOrdinal::new(ordinal))),
            DependencyRequirementKind::ValueDependencies);

        let captured = |ordinal| Arc::new(crate::dependency::ReportDependencies {
            requirements: [requirement(ordinal)].into(), ..Default::default()
        });

        let completed = captured(0);
        let abnormal = captured(1);

        let mut first = DeferredInvocation { callee: None, transition: None,
            entry: (), selection: Default::default(), completed: None,
            reports: [(DeferredCompleted, Arc::clone(&completed)), (DeferredAbnormal, Arc::clone(&abnormal))].into(),
        };

        let second = DeferredInvocation { callee: None, transition: None,
            entry: (), selection: Default::default(), completed: None,
            reports: [(DeferredCompleted, captured(2))].into(),
        };

        assert!(first.merge_capture(&second));
        assert_eq!(first.reports[&DeferredCompleted].requirements, [requirement(0), requirement(2)].into());
        assert_eq!(first.reports[&DeferredAbnormal].requirements, [requirement(1)].into());
        assert_eq!(completed.requirements, [requirement(0)].into());
        assert!(Arc::ptr_eq(&first.reports[&DeferredAbnormal], &abnormal));
        assert!(!first.merge_capture(&second));
    }

    #[test]
    fn unrelated_value_joins_share_pending_evidence_without_modifying_predecessors() {
        let (_, source) = expression_unit(BoundUnitId::new(91), |tree, origin| {
            vec![push_expression(
                tree,
                BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
            )]
        });

        let source: crate::execution_guarantees::DeferredProducer = source[0].into();
        let argument = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let value = ExecutionCondition::new(crate::ExecutionConditionKind::Input(argument.clone()));

        let entry = Arc::new(ExecutionCallEvidence {
            outputs: Default::default(),
            arguments: [(argument.clone(), value.clone())].into(),
            witness_inputs: [value.clone()].into(),
            extracted_values: [value.clone()].into(),
            ..ExecutionCallEvidence::default()
        });

        let owner = ExecutionCondition::result();
        let mut state = ExecutionState::default();

        state.current.insert(
            ExecutionPlace::argument(SymbolOrdinal::new(2)),
            owner.clone().with_deferred_value(Arc::new(DeferredValue {
                producers: [(
                    source.clone(),
                    DeferredInvocation { callee: None, transition: None,
                        entry: Arc::clone(&entry),
                        completed: None,
                        reports: Default::default(),
                        selection: Default::default(),
                    },
                )]
                .into(),
                ..DeferredValue::default()
            })),
        );

        let unrelated = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(1))));

        state.join_values(&crate::execution_guarantees::ExecutionSubstitution::new(
            &[(unrelated, ExecutionCondition::boolean(true))].into(),
            &Default::default()));

        assert!(Arc::ptr_eq(
            &entry,
            &carried(&state).producers[&source].entry
        ));

        let renamed = ExecutionCondition::boolean(false);

        let owner = state.current.live_value(&ExecutionPlace::argument(SymbolOrdinal::new(2))).unwrap();

        assert!(!state.owned_values(&owner).any(|owned| owned == value),
            "capturing extraction evidence does not make the future own its borrowed source");

        assert!(state.witness_values(&owner).any(|source| source == value),
            "the future retains the existing source evidence for its checked extraction");

        state.join_values(&crate::execution_guarantees::ExecutionSubstitution::new(
            &[(value.clone(), renamed.clone())].into(),
            &[(value.clone(), renamed.clone())].into()));

        let completed = &carried(&state).producers[&source].entry;

        assert!(!Arc::ptr_eq(&entry, completed));
        assert_eq!(entry.arguments[&argument], value);
        assert_eq!(completed.arguments[&argument], renamed);
        assert!(completed.witness_inputs.contains(&renamed));
        assert_eq!(entry.extracted_values, [value].into());
        assert_eq!(completed.extracted_values, [renamed].into());
    }
}
