use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::{ExecutionCallEvidence, ExecutionCondition};
use bray_bound_tree::BoundExecutionSite;

type Selection = BTreeSet<(ExecutionCondition, bool)>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DeferredInvocation<Entry = Arc<ExecutionCallEvidence>> {
    pub(super) entry: Entry,
    /// Actual frame ownership, captured only when a report can retain inputs.
    pub(super) reports: Option<Arc<crate::dependency::ReportDependencies>>,
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
    !guard
        .iter()
        .any(|(condition, holds)| condition.prove_trusted(&BTreeSet::new(), facts) == Some(!holds))
}

impl<Entry> DeferredInvocation<Entry> {
    fn merge_capture(&mut self, incoming: &Self) -> bool {
        let before = self.selection.len();

        self.selection.retain(|fact| incoming.selection.contains(fact));

        let mut changed = before != self.selection.len();

        if let Some(incoming) = &incoming.reports {
            if let Some(reports) = &mut self.reports {
                changed |= reports.merge(incoming);
            } else {
                self.reports = Some(Arc::clone(incoming));
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
                entry: (), selection: selection(facts), reports: None,
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
        context: Option<&Selection>,
    ) {
        for guard in self.unknown.iter_mut().map(|candidate| &mut candidate.selection).chain(
            self.producers
                .values_mut()
                .map(|candidate| &mut candidate.selection),
        ) {
            if let Some(context) = context {
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
                        reports: None,
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

        state.join_values(
            &[(value.clone(), renamed.clone())].into(),
            &[(value.clone(), renamed.clone())].into(),
        );

        let completed = &state.deferred_invocations[&owner].producers[&source].entry;

        assert!(!Arc::ptr_eq(&entry, completed));
        assert_eq!(entry.arguments[&argument], value);
        assert_eq!(completed.arguments[&argument], renamed);
        assert!(completed.witness_inputs.contains(&renamed));
    }
}
