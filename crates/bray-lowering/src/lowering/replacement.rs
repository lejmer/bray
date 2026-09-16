use bray_bound_tree::{AsyncStorageCleanupRequirement, BoundExpressionId, StorageReplacementPlan};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge, MirOperand,
    MirOperationKind, MirPlace, MirSourceAnchor, MirStorageKind, MirStoreKind, MirTerminatorKind,
};

use super::LoweringError;
use super::initialization::{InitializedPart, part_guard_for_place};
use super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn replace_value(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        destination: MirPlace,
        value: MirOperand,
    ) -> Result<MirBlockId, LoweringError> {
        let plan = self.input.replacement(expression).unwrap_or_else(|| {
            panic!(
                "lowering contract violation: MissingSemanticSelection {value:?}",
                value = expression
            )
        });

        let AsyncStorageCleanupRequirement::Cleanup(phases) = plan.cleanup() else {
            let ty = destination.ty();
            self.install_replacement(block, source, destination, value)?;
            let storage = self.input.storage_plan();

            if storage.resolved_projections(plan.access()) == Some(&[])
                && storage
                    .root_identity(plan.access())
                    .is_some_and(|identity| {
                        bray_bound_tree::storage_identity_is_destructor_receiver(
                            self.input.unit(),
                            storage,
                            identity,
                        )
                    })
            {
                // Reinitializing an already moved receiver does not start another destructor.
                // Its executing destructor retains the original allowance until it returns.
                self.discharge_outgoing_owner(block, source, ty)?;
            }

            return Ok(block);
        };

        let storage = self.builder.push_storage(
            Self::retained_source(source),
            MirStorageKind::Temporary,
            destination.ty(),
        )?;

        let replacement = MirPlace::new(storage, [], destination.ty());

        // Materialize reads and ownership transfers before old-value cleanup can modify their source.
        self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: Self::retained_place(&replacement),
                value,
            },
            None,
        )?;

        let report = self.representation_type(RepresentationRole::PanicReport)?;
        self.cleanup_outcome = Some(self.create_cleanup_outcome(block, source)?);

        let broadcast = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::CleanupBroadcast,
        )?;

        let lifecycle = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        self.set_terminator(
            block,
            Self::retained_source(source),
            MirTerminatorKind::BeginCleanup(MirCleanupEdge::new(
                MirCleanupPhase::TaskCancellation,
                MirEdge::new(broadcast, []),
            )),
        )?;

        let broadcast = if phases.includes_cancellation() {
            self.replacement_cleanup(
                broadcast,
                source,
                MirCleanupPhase::TaskCancellation,
                &destination,
                plan,
            )?
        } else {
            broadcast
        };

        self.set_terminator(
            broadcast,
            Self::retained_source(source),
            MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                MirCleanupPhase::LifecycleResolution,
                MirEdge::new(lifecycle, []),
            )),
        )?;

        let lifecycle = if phases.includes_lifecycle() {
            self.replacement_cleanup(
                lifecycle,
                source,
                MirCleanupPhase::LifecycleResolution,
                &destination,
                plan,
            )?
        } else {
            lifecycle
        };

        self.install_replacement(
            lifecycle,
            source,
            destination,
            MirOperand::Move(replacement),
        )?;

        let installed = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        self.set_terminator(
            lifecycle,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(installed, [])),
        )?;

        let outcome = self.cleanup_outcome.take().unwrap_or_else(|| {
            panic!(
                "lowering contract violation: MissingSemanticSelection {value:?}",
                value = expression
            )
        });

        let panicked = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        let cancelled = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        let completed =
            outcome.dispatch(&mut self.builder, installed, source, panicked, cancelled)?;

        self.finish_panic_to_active_catch(expression, panicked, source, outcome.report(), report)?;
        self.finish_cancellation(cancelled, source, expression.into())?;

        Ok(completed)
    }

    fn install_replacement(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        destination: MirPlace,
        value: MirOperand,
    ) -> Result<(), LoweringError> {
        self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::Store {
                kind: MirStoreKind::Assign,
                destination,
                value,
            },
            None,
        )?;

        Ok(())
    }

    fn replacement_cleanup(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        place: &MirPlace,
        plan: &StorageReplacementPlan,
    ) -> Result<MirBlockId, LoweringError> {
        let owned = self.ownership_place(place);
        let state = self.initialization_guards.get(&owned.storage());

        let guard = state.map(|state| {
            state
                .parts
                .iter()
                .find_map(|part| {
                    (part.plan.projections().len() == owned.projections().len())
                        .then(|| part_guard_for_place(part, state.guard.ty(), &owned))
                        .flatten()
                        .filter(|(_, dimensions)| *dimensions == part.array_types.len())
                        .map(|(guard, _)| guard)
                })
                .unwrap_or_else(|| Self::retained_place(&state.guard))
        });

        if plan.parts().is_none() {
            return self
                .push_guarded_cleanup(
                    block,
                    source,
                    phase,
                    Self::retained_place(place),
                    guard,
                    None,
                    self.input
                        .finalizer_is_complete(plan.expression().into(), plan.access()),
                    None,
                )
                .map(|(block, _)| block);
        }

        // Keep checked part paths borrowed and flag descriptors owned while constructing their cleanup blocks.
        let parts = state
            .map(|state| {
                state
                    .parts
                    .iter()
                    .filter(|part| part_guard_for_place(part, state.guard.ty(), &owned).is_some())
                    .map(|part| InitializedPart {
                        plan: part.plan,
                        guard: Self::retained_place(&part.guard),
                        array_types: std::sync::Arc::clone(&part.array_types),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if parts.is_empty() {
            panic!(
                "lowering contract violation: UnsupportedStorageAccess {value:?}",
                value = plan.access()
            );
        }

        self.push_part_cleanup(
            block,
            source,
            phase,
            plan.access(),
            Self::retained_place(place),
            &parts,
            owned.projections().len(),
            None,
        )
        .map(|(block, _)| block)
    }
}
