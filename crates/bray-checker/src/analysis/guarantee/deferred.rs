use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::{ExecutionCallEvidence, ExecutionCondition};
use bray_bound_tree::BoundExecutionSite;

type Selection = BTreeSet<(ExecutionCondition, bool)>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DeferredInvocation<Entry = Arc<ExecutionCallEvidence>> {
    pub(super) entry: Entry,
    /// Each report owns its formation snapshot, independently of frame captures.
    pub(super) reports: BTreeMap<bray_bound_tree::CallReportOutput, Arc<crate::dependency::ReportDependencies>>,
    pub(super) selection: Selection,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::analysis) struct DeferredValue {
    pub(super) producers: BTreeMap<BoundExecutionSite, DeferredInvocation>,
    pub(super) unknown: Option<DeferredInvocation<()>>,
}

pub(super) fn selection(facts: &Selection) -> Selection {
    facts
        .iter()
        .filter(|(condition, _)| {
            !condition.contains_trusted() && !condition.observes_live(None)
        })
        .cloned()
        .collect()
}

pub(super) fn possible(guard: &Selection, facts: &Selection) -> bool {
    let trusted = BTreeSet::new();
    let proof = crate::execution_guarantees::ExecutionProof::new(&trusted, facts);

    !guard
        .iter()
        .any(|(condition, holds)| proof.prove_trusted(condition) == Some(!holds))
}

impl<Entry> DeferredInvocation<Entry> {
    fn merge_capture(&mut self, incoming: &Self) -> bool {
        let before = self.selection.len();

        self.selection.retain(|fact| incoming.selection.contains(fact));

        let mut changed = before != self.selection.len();

        for (output, incoming) in &incoming.reports {
            if let Some(reports) = self.reports.get_mut(output) {
                changed |= reports.merge(incoming);
            } else {
                self.reports.insert(*output, Arc::clone(incoming));
                changed = true;
            }
        }

        changed
    }
}

impl DeferredValue {
    pub(super) fn unknown(facts: &Selection) -> Self {
        Self {
            unknown: Some(DeferredInvocation {
                entry: (), selection: selection(facts), reports: BTreeMap::new(),
            }),
            ..Self::default()
        }
    }

    pub(super) fn unknown_possible(&self, facts: &Selection) -> bool {
        self.unknown
            .as_ref()
            .is_some_and(|candidate| possible(&candidate.selection, facts))
    }

    pub(super) fn known_sources(&self, facts: &Selection) -> Option<BTreeSet<BoundExecutionSite>> {
        (!self.unknown_possible(facts)).then(|| {
            self.producers
                .iter()
                .filter(|(_, candidate)| possible(&candidate.selection, facts))
                .map(|(source, _)| *source)
                .collect()
        })
    }

    pub(super) fn merge(&mut self, incoming: &Self) -> bool {
        let mut changed = false;

        for (source, candidate) in &incoming.producers {
            if let Some(previous) = self.producers.get_mut(source) {
                if previous != candidate {
                    if previous.entry != candidate.entry {
                        changed |= Arc::make_mut(&mut previous.entry).intersect(&candidate.entry);
                    }

                    changed |= previous.merge_capture(candidate);
                }
            } else {
                // Producers share captured evidence while each flow branch owns its selection facts.
                self.producers.insert(*source, candidate.clone());
                changed = true;
            }
        }

        if let Some(incoming) = &incoming.unknown {
            if let Some(previous) = &mut self.unknown {
                changed |= previous.merge_capture(incoming);
            } else {
                self.unknown = Some(incoming.clone());
                changed = true;
            }
        }

        changed
    }

    pub(super) fn join_values(
        &mut self,
        replacements: &BTreeMap<ExecutionCondition, ExecutionCondition>,
        identities: &BTreeMap<ExecutionCondition, ExecutionCondition>,
        current: &super::current::CurrentValues,
        context: Option<&Selection>,
    ) {
        let current_fact = |condition: &ExecutionCondition| {
            !condition.observations().any(|observed| matches!(observed, ExecutionCondition::ScalarValue(_))
                && observed.selector_place().is_some_and(|place| current.live_value(place)
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

            *guard = guard
                .iter()
                .flat_map(|(condition, holds)| {
                    let mut facts = BTreeSet::new();

                    condition
                        .with_joined_values(replacements, identities)
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

            if let Some(entry) = candidate.entry.joined_values(replacements, identities) {
                candidate.entry = Arc::new(entry);
            }
        }
    }

    pub(super) fn retire_selection(&mut self, stale: impl Fn(&ExecutionCondition) -> bool) {
        for guard in self.unknown.iter_mut().map(|candidate| &mut candidate.selection).chain(
            self.producers
                .values_mut()
                .map(|candidate| &mut candidate.selection),
        ) {
            guard.retain(|(condition, _)| !stale(condition));
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

    #[test]
    fn frame_moves_freeze_copy_guards_and_do_not_import_old_loop_incarnations() {
        let (_, expressions) = expression_unit(BoundUnitId::new(92), |tree, origin| {
            [0, 1].map(|_| push_expression(tree, BoundExpression::Error(BoundErrorExpression::new(origin, error_type())))).to_vec()
        });

        let owner = ExecutionCondition::Expression(expressions[0]);
        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let source = ExecutionCondition::ScalarValue(Arc::new(ExecutionCondition::Joined(BoundUnitId::new(92), 6, place.clone())));

        let captured = |owner| ExecutionCondition::CapturedInput {
            owner: Arc::new(owner), invocation: expressions[0].into(), ordinal: SymbolOrdinal::new(0),
        };

        let mut state = ExecutionState::default();

        state.current.insert(place.clone(), source.clone());
        state.assumptions.insert((source.clone(), true));

        state.deferred_invocations.insert(owner.clone(), DeferredValue {
            producers: [(expressions[0].into(), DeferredInvocation {
                entry: Arc::new(ExecutionCallEvidence {
                    arguments: [(place.clone(), captured(owner.clone()))].into(),
                    copy_sources: [(place.clone(), source.clone())].into(), pending_execution: true, ..Default::default()
                }), reports: Default::default(), selection: Default::default(),
            })].into(),
            unknown: Some(DeferredInvocation { entry: (), reports: Default::default(), selection: [(source.clone(), true)].into() }),
        });

        let middle = ExecutionCondition::Result;

        state.join_values(&[(owner.clone(), middle.clone())].into(), &[(owner, middle.clone())].into());
        assert!(state.deferred_invocations[&middle].producers[&expressions[0].into()].entry.assumptions.contains(&(captured(middle.clone()), true)));
        assert!(state.deferred_invocations[&middle].producers[&expressions[0].into()].selection.contains(&(source.clone(), true)));

        state.current.insert(place, ExecutionCondition::Boolean(false));

        let target = ExecutionCondition::Expression(expressions[1]);

        state.join_values(&[(middle.clone(), target.clone())].into(), &[(middle, target.clone())].into());

        let moved = &state.deferred_invocations[&target].producers[&expressions[0].into()];

        assert!(!moved.selection.contains(&(source.clone(), true)));
        assert!(!moved.entry.assumptions.contains(&(source, true)));
        assert!(state.deferred_invocations[&target].unknown.as_ref().expect("opaque predecessor remains possible").selection.is_empty());
        assert!(moved.entry.copy_sources.is_empty());
        assert!(moved.entry.assumptions.contains(&(captured(target), true)));
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

        let mut first = DeferredInvocation {
            entry: (), selection: Default::default(),
            reports: [(DeferredCompleted, Arc::clone(&completed)), (DeferredAbnormal, Arc::clone(&abnormal))].into(),
        };

        let second = DeferredInvocation {
            entry: (), selection: Default::default(),
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

        let source = source[0].into();
        let argument = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let value = ExecutionCondition::Input(argument.clone());

        let entry = Arc::new(ExecutionCallEvidence {
            arguments: [(argument.clone(), value.clone())].into(),
            witness_inputs: [value.clone()].into(),
            extracted_values: [value.clone()].into(),
            ..ExecutionCallEvidence::default()
        });

        let owner = ExecutionCondition::Result;
        let mut state = ExecutionState::default();

        state.deferred_invocations.insert(
            owner.clone(),
            DeferredValue {
                producers: [(
                    source,
                    DeferredInvocation {
                        entry: Arc::clone(&entry),
                        reports: Default::default(),
                        selection: Default::default(),
                    },
                )]
                .into(),
                ..DeferredValue::default()
            },
        );

        let unrelated = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));

        state.join_values(
            &[(unrelated, ExecutionCondition::Boolean(true))].into(),
            &Default::default(),
        );

        assert!(Arc::ptr_eq(
            &entry,
            &state.deferred_invocations[&owner].producers[&source].entry
        ));

        let renamed = ExecutionCondition::Boolean(false);

        assert!(!state.owned_values(&owner).any(|owned| owned == value),
            "capturing extraction evidence does not make the future own its borrowed source");

        assert!(state.witness_values(&owner).any(|source| source == value),
            "the future retains the existing source evidence for its checked extraction");

        state.join_values(
            &[(value.clone(), renamed.clone())].into(),
            &[(value.clone(), renamed.clone())].into(),
        );

        let completed = &state.deferred_invocations[&owner].producers[&source].entry;

        assert!(!Arc::ptr_eq(&entry, completed));
        assert_eq!(entry.arguments[&argument], value);
        assert_eq!(completed.arguments[&argument], renamed);
        assert!(completed.witness_inputs.contains(&renamed));
        assert_eq!(entry.extracted_values, [value].into());
        assert_eq!(completed.extracted_values, [renamed].into());
    }
}
