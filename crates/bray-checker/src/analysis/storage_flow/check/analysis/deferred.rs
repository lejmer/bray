use bray_bound_tree::BoundExecutionSite;
use bray_symbols::{DependencyMutationKind as Kind, DependencySubjectRoot};

use super::core::StorageFlowCollector;
use crate::analysis::storage_flow::memory::{Effect, Target};
use crate::analysis::storage_flow::model::StorageFlowState;
use crate::CheckerRequestContext;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn capture_deferred_cleanup(
        &self,
        state: &mut StorageFlowState,
        invocation: BoundExecutionSite,
    ) {
        let contracts = &self.dependencies.calls()[&invocation];
        let Some(captures) = contracts.deferred_capture_cleanup() else { return; };
        let result = contracts.deferred_result_cleanup().expect("prepared deferred cleanup retains both phases");
        let roots = self.invocation_roots(state, invocation);
        let output = Target::storage(self.invocation_result(invocation).expect("lazy call retains its completion coordinates"));
        let mut private = vec![output.clone()];

        if let Some(selection) = invocation.expression().and_then(|expression| self.execution.semantics.selections().expression(expression)) {
            private.extend(selection.inputs().filter_map(|input| self.invocation_place(invocation, input.root()).map(Target::storage)));
        }

        let captures = self.capture_opaque_effects(state, invocation, self.call_effects(state, invocation, captures), &roots, &private, &output);
        let result = self.resolve_effects(result, |subject, indirect| {
            if let Some(target) = self.static_location(subject) { return Some(vec![target]); }
            if subject.subject_root() != DependencySubjectRoot::Result { return None; }
            let path = contracts.reference_path(subject)?;
            let location = state.memory.staged_reference_location(invocation, self.request.semantic_values(), &output, path)?;
            if indirect { state.memory.staged_reference_regions(invocation, &output, &location) } else { Some(vec![location]) }
        });
        let result = self.capture_opaque_effects(state, invocation, result, &roots, &private, &output);
        state.memory.capture_deferred_cleanup(invocation, captures, result);
    }

    fn capture_opaque_effects(
        &self,
        state: &StorageFlowState,
        invocation: BoundExecutionSite,
        effects: Vec<Effect>,
        captures: &[Target],
        private: &[Target],
        output: &Target,
    ) -> Vec<Effect> {
        let mut captured = Vec::new();
        for effect in effects {
            match effect {
                Effect::Mutation(target, Kind::Cleanup(ty), _) => {
                    let (mut regions, unknown) = if output.contains(&target) {
                        state.memory.staged_contained_regions(invocation, &target)
                    } else {
                        let mut regions = std::collections::BTreeSet::new();
                        let mut unknown = false;
                        for (_, targets, opaque) in state.memory.observed_references().filter(|(slot, _, _)| target.contains(slot)) {
                            regions.extend(targets.iter().cloned());
                            unknown |= opaque;
                        }
                        (regions, unknown)
                    };
                    let reference_free = self.input.footprint.complete_reference_types.get(&ty) == Some(&true);
                    if unknown || regions.is_empty() && !reference_free {
                        regions.extend(captures.iter().cloned());
                    }
                    if !private.iter().any(|root| root.contains(&target)) {
                        regions.insert(target);
                    }
                    // Region bounds identify captured referents. They do not claim
                    // that each referent has the disposed value's type or layout.
                    captured.extend(regions.into_iter().map(|region| Effect::Mutation(region, Kind::Cleanup(ty), None)));
                    captured.push(Effect::UnboundedMutation { kind: Kind::Cleanup(ty), access_type: None, ambient: true });
                }
                Effect::Unknown | Effect::Escape => {
                    let kind = if effect == Effect::Escape { Kind::Escape } else { Kind::Write };
                    captured.extend(captures.iter().cloned().map(|target| Effect::Mutation(target, kind, None)));
                    captured.push(Effect::Ambient);
                }
                effect => captured.push(effect),
            }
        }
        captured
    }
}
