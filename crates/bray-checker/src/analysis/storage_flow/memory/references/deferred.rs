use std::sync::Arc;

use bray_bound_tree::{BoundExecutionSite, StoragePath};

use super::super::model::{Origin, State, Target, Transfer};
use super::super::Effect;

impl State {
    /// Transfers the one prepared latent transition into the actual formed frame.
    pub(in crate::analysis::storage_flow) fn capture_deferred_transition(
        &mut self, source: BoundExecutionSite, output: StoragePath,
    ) -> Arc<bray_bound_tree::StorageDeferredTransition> {
        let transfer = self.pending.remove(&source)
            .expect("formed lazy call retains its prepared storage transition");

        Arc::new(bray_bound_tree::StorageDeferredTransition { output: Some(output), transfer })
    }

    pub(in crate::analysis::storage_flow) fn capture_deferred_cleanup(
        &mut self,
        invocation: BoundExecutionSite,
        captures: Vec<Effect>,
        result: Vec<Effect>,
    ) {
        let transfer = self.pending.get_mut(&invocation)
            .expect("deferred cleanup is captured with its prepared value transition");

        let result = result.into_iter().flat_map(|effect| {
            let Effect::Mutation { footprint: target, kind, access_type, .. } = &effect
            else {
                return vec![effect];
            };

            let Origin::Returned { reference, .. } = &target.origin
            else {
                return vec![effect];
            };

            let Some(bounds) = transfer.returned_inputs.get(reference).filter(|_| !target.previous) else {
                return vec![effect];
            };

            // A pending result has no current referent. Its possible cleanup
            // effects retain physical inputs and independent ambient storage.
            let (origins, unbounded) = Self::effect_origins_from(target, Some(bounds));

            let mut effects = origins.cloned().map(|origin| Effect::bounded(origin, *kind, *access_type))
                .collect::<Vec<_>>();

            if unbounded {
                effects.push(Effect::unbounded(*kind, *access_type, true));
            }

            effects
        }).collect();

        transfer.captured_cleanup = Some(captures.into_iter().collect());
        transfer.result_cleanup = Some(result);
    }

    pub(in crate::analysis::storage_flow) fn prepare_deferred_completion(
        &mut self,
        invocation: BoundExecutionSite,
        result: StoragePath,
        producers: impl IntoIterator<Item = Arc<bray_bound_tree::StorageDeferredTransition>>,
    ) -> bool {
        let result = Target::storage(result);
        let mut completed: Option<Transfer> = None;

        for entry in producers {
            let Some(output) = &entry.output else {
                self.begin_transition(invocation);

                return false;
            };

            let mut transfer = entry.transfer.clone();

            transfer.relocate(&Target::storage(output.clone()), &result);

            if let Some(previous) = &mut completed { previous.merge(&transfer); }
            else { completed = Some(transfer); }
        }

        let Some(completed) = completed else { return false; };

        self.pending.insert(invocation, completed);

        true
    }
}
