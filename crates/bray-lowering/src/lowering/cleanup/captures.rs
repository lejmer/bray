use std::sync::Arc;

use bray_bound_tree::{AnyBoundNodeId, StorageAccessId};
use bray_ir::{
    MirBlockKind, MirFrameRetention, MirFrameState, MirFrameStateId, MirOperationKind,
    MirSourceAnchor,
};

use super::control::CleanupDestination;
use crate::lowering::LoweringError;
use crate::lowering::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(in crate::lowering) fn initial_frame_retention(&self) -> MirFrameRetention {
        let initialized = self.storages.iter().filter_map(|(identity, storage)| {
            self.input
                .storage_plan()
                .identity(*identity)
                .expect("lowered capture retains its checked identity")
                .is_initialized_at_entry()
                .then_some(*storage)
        });

        MirFrameRetention::new(
            self.initial_broadcast
                .expect("protected frame retains its capture broadcast"),
            self.execution_lane_requirements(),
            initialized,
        )
        .with_affinity(self.frame_affinity())
    }

    pub(in crate::lowering) fn frame_capture_cleanup(
        &mut self,
        source: &MirSourceAnchor,
        entry: AnyBoundNodeId,
    ) -> Result<Option<(MirFrameStateId, MirFrameStateId)>, LoweringError> {
        let plans = self.cleanup_plans(0, entry);

        let destruction = plans
            .iter()
            .flat_map(|plan| plan.lifecycle_resolution().iter().copied())
            .collect::<Vec<_>>();

        if destruction.is_empty() {
            return Ok(None);
        }

        let quiescence = plans
            .iter()
            .flat_map(|plan| plan.cancellation_broadcast().iter().copied())
            .collect::<Vec<_>>();

        let retention = Arc::new(self.initial_frame_retention());
        let quiescence = self.capture_cleanup_entry(source, &quiescence, false, &retention)?;
        let destruction = self.capture_cleanup_entry(source, &destruction, true, &retention)?;

        Ok(Some((quiescence, destruction)))
    }

    fn capture_cleanup_entry(
        &mut self,
        source: &MirSourceAnchor,
        accesses: &[StorageAccessId],
        destroying: bool,
        retention: &Arc<MirFrameRetention>,
    ) -> Result<MirFrameStateId, LoweringError> {
        let entry = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let state = self.next_frame_state()?;

        // Both entries retain the same capture identities established before publication.
        self.frame_states
            .push(MirFrameState::new(state, entry, retention.as_ref().clone()));

        let outcome = self.create_cleanup_outcome(entry, source)?;
        let outcome = outcome.with_retention((!destroying).then(|| Arc::clone(retention)));

        outcome.initialize_cancellation(&mut self.builder, entry, source, None)?;

        let mut block = entry;

        for access in accesses {
            let place = self.place_for_access(*access, false)?;

            let operation = if destroying {
                MirOperationKind::Destroy {
                    place,
                    abandoned: Some(bray_ir::MirOperand::Immediate {
                        ty: self.representation_type(
                            bray_compiler_known::RepresentationRole::ScalarBool,
                        )?,
                        value: bray_ir::MirImmediateValue::Boolean(true),
                    }),
                    collection: None,
                }
            } else {
                MirOperationKind::Quiesce {
                    place,
                    collection: None,
                }
            };

            self.push_operation(block, Self::retained_source(source), operation, None)?;
            block = outcome.check(&mut self.builder, block, source, false)?;
        }

        let panicked = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let cancelled = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let completed = outcome.dispatch(
            &mut self.builder,
            block,
            source,
            panicked,
            cancelled,
            &mut None,
        )?;

        self.set_destination(completed, source, CleanupDestination::Goto(cancelled), None)?;

        self.set_destination(
            panicked,
            source,
            CleanupDestination::PropagatePanic,
            Some(outcome.report()),
        )?;

        self.set_destination(
            cancelled,
            source,
            CleanupDestination::PropagateCancellation,
            Some(outcome.report()),
        )?;

        Ok(state)
    }
}
