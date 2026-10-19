use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge, MirRuntimeReference,
    MirSourceAnchor, MirTerminatorKind, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;

use super::super::{SyntheticLowerer, SyntheticLoweringContext};
use crate::cleanup_outcome::CleanupOutcome;

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(super) fn borrow_collection(
        &self,
        builder: &mut MirUnitBuilder,
        source: &MirSourceAnchor,
        first_parameter: u32,
    ) -> Result<CleanupOutcome, C::Error> {
        let values = self.context.semantic_values();

        let state_type = self
            .context
            .representation_type(RepresentationRole::ScalarU32)?;

        let report_type = self
            .context
            .representation_type(RepresentationRole::PanicReport)?;

        let states = CleanupOutcome::state_constants(values, state_type)
            .map_err(super::super::SyntheticLoweringError::SemanticValue)?;

        let mut parameter = |ordinal, ty| {
            let pointer = values
                .intern_type(bray_symbols::TypeData::Borrow {
                    kind: bray_symbols::BorrowKind::Mutable,
                    target: ty,
                })
                .map_err(super::super::SyntheticLoweringError::SemanticValue)?;

            let storage = builder
                .push_storage(
                    source.clone(),
                    bray_ir::MirStorageKind::Parameter(ordinal),
                    pointer,
                )
                .map_err(|cause| self.capacity_error(cause))?;

            Ok::<_, C::Error>(bray_ir::MirPlace::new(
                storage,
                [bray_ir::MirProjection::new(
                    bray_ir::MirProjectionKind::Dereference,
                    pointer,
                    ty,
                )],
                ty,
            ))
        };

        let collection = bray_ir::MirCleanupCollection::new(
            parameter(first_parameter, state_type)?,
            parameter(first_parameter + 1, report_type)?,
        );

        Ok(CleanupOutcome::from_collection(
            collection,
            states.1,
            self.context
                .representation_type(RepresentationRole::ScalarBool)?,
            self.context.representation_type(RepresentationRole::Unit)?,
            builder.target().runtime_abi(),
        ))
    }

    pub(in crate::synthetic) fn cleanup_outcome(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<CleanupOutcome, C::Error> {
        if let Some(collection) = &self.collection {
            // The returned builder view shares the helper's borrowed storage identities.
            return Ok(collection.clone());
        }

        let boolean = self
            .context
            .representation_type(RepresentationRole::ScalarBool)?;

        let report = self
            .context
            .representation_type(RepresentationRole::PanicReport)?;

        let unit = self.context.representation_type(RepresentationRole::Unit)?;

        let states = CleanupOutcome::state_constants(
            self.context.semantic_values(),
            self.context
                .representation_type(RepresentationRole::ScalarU32)?,
        )
        .map_err(super::super::SyntheticLoweringError::SemanticValue)?;

        CleanupOutcome::new(
            builder,
            block,
            source,
            boolean,
            report,
            unit,
            builder.target().runtime_abi(),
            states,
        )
        .map_err(|cause| self.capacity_error(cause))
    }

    pub(in crate::synthetic) fn finish_cleanup_outcome(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        outcome: &CleanupOutcome,
        value: &mut Option<bray_ir::MirOperand>,
    ) -> Result<MirBlockId, C::Error> {
        if self.collection.is_some() {
            return Ok(block);
        }

        let invalid = |cause| self.capacity_error(cause);
        let kind = builder.block_kind(block);

        // Each completion block independently retains generated-body provenance.
        let panicked = builder.push_block(source.clone(), kind).map_err(invalid)?;
        let cancelled = builder.push_block(source.clone(), kind).map_err(invalid)?;
        let abi = builder.target().runtime_abi();

        let completed = outcome
            .dispatch(builder, block, source, panicked, cancelled, value)
            .map_err(invalid)?;

        self.finish_cleanup_propagation(
            builder,
            panicked,
            source,
            MirTerminatorKind::PropagatePanic {
                report: outcome.report(),
                runtime: MirRuntimeReference::new(RuntimeAbiRole::PanicPropagation, abi),
            },
        )?;

        self.finish_cleanup_propagation(
            builder,
            cancelled,
            source,
            MirTerminatorKind::PropagateCancellation {
                report: outcome.report(),
                runtime: MirRuntimeReference::new(RuntimeAbiRole::PanicPropagation, abi),
            },
        )?;

        Ok(completed)
    }

    pub(super) fn check_lifecycle_value(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        value: bray_ir::MirValueId,
        result: bray_symbols::TypeId,
    ) -> Result<(MirBlockId, bray_ir::MirValueId), C::Error> {
        if let Some(collection) = &self.collection {
            let (completed, result, failed) = collection
                .check_completion(
                    builder,
                    block,
                    source,
                    Some(bray_ir::MirOperand::Value(value)),
                )
                .map_err(|cause| self.capacity_error(cause))?;

            let terminal = match self.continuation {
                Some(continuation) => MirTerminatorKind::Goto(MirEdge::new(continuation, [])),
                None => MirTerminatorKind::Return {
                    value: None,
                    report: None,
                },
            };

            builder.set_terminator(failed, source.clone(), terminal);

            let Some(bray_ir::MirOperand::Value(result)) = result else {
                unreachable!("a completed lifecycle invocation retains its result parameter");
            };

            return Ok((completed, result));
        }

        let invalid = |cause| self.capacity_error(cause);
        let kind = builder.block_kind(block);

        let report_type = self
            .context
            .representation_type(RepresentationRole::PanicReport)?;

        let completed = builder.push_block(source.clone(), kind).map_err(invalid)?;

        let value_parameter = builder
            .push_block_parameter(completed, source.clone(), result)
            .map_err(invalid)?;

        let panicked = builder.push_block(source.clone(), kind).map_err(invalid)?;

        let report_storage = builder
            .push_storage(
                source.clone(),
                bray_ir::MirStorageKind::Temporary,
                report_type,
            )
            .map_err(invalid)?;

        let report = bray_ir::MirPlace::new(report_storage, [], report_type);

        let cancelled = builder.push_block(source.clone(), kind).map_err(invalid)?;

        builder.set_terminator(
            block,
            source.clone(),
            MirTerminatorKind::CheckCallOutcome {
                completed: MirEdge::new(completed, [bray_ir::MirOperand::Value(value)]),
                panicked: bray_ir::MirCallFailureEdge::new(panicked, report.clone()),
                cancelled: bray_ir::MirCallFailureEdge::new(cancelled, report.clone()),
                completed_report: None,
            },
        );

        let abi = builder.target().runtime_abi();

        self.finish_cleanup_propagation(
            builder,
            panicked,
            source,
            MirTerminatorKind::PropagatePanic {
                report: bray_ir::MirOperand::Move(report.clone()),
                runtime: MirRuntimeReference::new(RuntimeAbiRole::PanicPropagation, abi),
            },
        )?;

        self.finish_cleanup_propagation(
            builder,
            cancelled,
            source,
            MirTerminatorKind::PropagateCancellation {
                report: bray_ir::MirOperand::Move(report.clone()),
                runtime: MirRuntimeReference::new(RuntimeAbiRole::PanicPropagation, abi),
            },
        )?;

        Ok((completed, value_parameter))
    }

    fn finish_cleanup_propagation(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        mut terminator: MirTerminatorKind,
    ) -> Result<(), C::Error> {
        let invalid = |cause| self.capacity_error(cause);

        if builder.block_kind(block) != MirBlockKind::CleanupBroadcast {
            builder.set_terminator(block, source.clone(), terminator);

            return Ok(());
        }

        let terminal = builder
            .push_block(source.clone(), MirBlockKind::LifecycleResolution)
            .map_err(invalid)?;

        let argument = if let MirTerminatorKind::PropagatePanic { report, .. }
        | MirTerminatorKind::PropagateCancellation { report, .. } =
            &mut terminator
            && matches!(report, bray_ir::MirOperand::Value(_))
        {
            let report_type = self
                .context
                .representation_type(RepresentationRole::PanicReport)?;

            let parameter = builder
                .push_block_parameter(terminal, source.clone(), report_type)
                .map_err(invalid)?;

            Some(std::mem::replace(
                report,
                bray_ir::MirOperand::Value(parameter),
            ))
        } else {
            None
        };

        builder.set_terminator(
            block,
            source.clone(),
            MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                MirCleanupPhase::LifecycleResolution,
                MirEdge::new(terminal, argument),
            )),
        );

        builder.set_terminator(terminal, source.clone(), terminator);

        Ok(())
    }
}
