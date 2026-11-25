use bray_bound_tree::{AsyncStorageCleanupRequirement, BoundExpressionId, StorageReplacementDecision};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge, MirOperand,
    MirOperationKind, MirPlace, MirProjection, MirProjectionKind, MirSourceAnchor, MirStorageKind, MirStoreKind, MirTerminatorKind,
};
use bray_symbols::{BorrowKind, TypeData};

use super::LoweringError;
use super::initialization::part_guard_for_place;
use super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn capture_replacement_place(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        place: MirPlace,
    ) -> Result<MirPlace, LoweringError> {
        if !place.projections().iter().any(|projection| projection.kind() == &MirProjectionKind::Dereference) {
            return Ok(place);
        }

        let original = self.ownership_place(&place);
        let ty = place.ty();
        let source = self.expression_source(expression);
        let kind = BorrowKind::Mutable;
        let address_type = self.input.semantic_values().intern_type(TypeData::Borrow { kind, target: ty })?;

        let address = self.push_typed_value_operation(expression, block, Self::retained_source(&source),
            MirOperationKind::Borrow { kind, place }, address_type)?;

        let address = self.materialize_synthetic_place(block, source, address, address_type)?;
        let storage = address.storage();

        self.owned_targets.insert(storage, original);

        Ok(MirPlace::new(storage, [MirProjection::new(MirProjectionKind::Dereference, address_type, ty)], ty))
    }

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

        let previous_occurrence =
            self.cleanup_occurrence
                .replace(bray_bound_tree::BoundExecutionSite::Cleanup(
                    expression.into(),
                ));

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

        self.cleanup_outcome = Some(self.create_cleanup_outcome(block, source)?);

        let broadcast = self.begin_cleanup_broadcast(block, source)?;

        let lifecycle = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
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

        let outcome = self.cleanup_outcome.take().unwrap_or_else(|| {
            panic!(
                "lowering contract violation: MissingSemanticSelection {value:?}",
                value = expression
            )
        });

        let completed = self.finish_replacement_cleanup(expression, lifecycle, source, &outcome)?;

        self.cleanup_occurrence = previous_occurrence;

        Ok(completed)
    }

    pub(super) fn finish_replacement_cleanup(
        &mut self,
        expression: BoundExpressionId,
        lifecycle: MirBlockId,
        source: &MirSourceAnchor,
        outcome: &crate::cleanup_outcome::CleanupOutcome,
    ) -> Result<MirBlockId, LoweringError> {
        let report = self.representation_type(RepresentationRole::PanicReport)?;

        let installed = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        self.set_terminator(
            lifecycle,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(installed, [])),
        )?;

        let panicked = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        let cancelled = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        let completed = outcome.dispatch(
            &mut self.builder,
            installed,
            source,
            panicked,
            cancelled,
            &mut None,
        )?;

        self.finish_panic_to_active_catch(expression, panicked, source, outcome.report(), report)?;
        self.finish_cancellation(cancelled, source, expression.into(), Some(outcome.report()))?;

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
        plan: &StorageReplacementDecision,
    ) -> Result<MirBlockId, LoweringError> {
        let owned = self.ownership_place(place);
        let state = self.initialization_guards.get(&owned.storage());

        if let Some(range) = plan.part_range() {
            let state = state.expect("represented replacement retains its prepared guard partition");

            let cursor = super::arrays::CleanupPartCursor::new(
                plan.access(), Self::retained_place(place), std::sync::Arc::clone(&state.parts), owned.projections().len(),
            ).with_range(range);

            return self.push_part_cleanup(block, source, phase, cursor, None, None).map(|(block, _)| block);
        }

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

        self.push_guarded_cleanup(
            block,
            source,
            phase,
            Self::retained_place(place),
            guard,
            None,
            self.input.finalizer_is_complete(plan.expression().into(), plan.access()),
            None,
        )
        .map(|(block, _)| block)
    }
}
