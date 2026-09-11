use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirFrameDescriptor, MirFrameState, MirGeneratedLifecycleRole, MirOperationId,
    MirPlace, MirSourceAnchor, MirTerminatorKind, MirUnit, MirUnitBuildError, MirUnitBuilder,
};
use bray_symbols::{CallableExecution, TypeId};

use crate::{SyntheticLoweringContext, SyntheticLoweringError};

/// Expands template lifecycle operations using their closed execution contracts.
/// Synchronous actions retain direct calls. Asynchronous actions require a protected frame and
/// retain the template's completion, panic, and cancellation continuations.
pub fn specialize_lifecycle_execution<C: SyntheticLoweringContext + ?Sized>(
    context: &C,
    unit: MirUnit,
    concrete_type: impl Fn(TypeId) -> Result<TypeId, C::Error>,
) -> Result<MirUnit, C::Error> {
    let unit = super::destructor::resolve_remainder(unit).map_err(C::Error::from)?;
    let mut actions = Vec::new();
    let mut cleanup_types = std::collections::BTreeMap::new();

    for (block_id, block) in unit.blocks_with_ids() {
        for operation_id in block.operations() {
            let operation = unit.operation(*operation_id).ok_or_else(|| {
                failure(
                    block.terminator().source(),
                    MirUnitBuildError::MissingOperation(*operation_id),
                )
            })?;

            let Some((role, place)) = operation.kind().lifecycle_action() else {
                continue;
            };

            let (ty, cleanup) = match cleanup_types.entry(place.ty()) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    let ty = concrete_type(place.ty())?;

                    entry.insert((ty, context.cleanup_type_execution(ty)?))
                }
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            };

            let execution = role
                .execution(cleanup)
                .ok_or(SyntheticLoweringError::UnresolvedType(*ty))?;

            if execution == CallableExecution::Synchronous {
                continue;
            }

            if block.operations().last() != Some(operation_id)
                || !matches!(
                    block.terminator().kind(),
                    MirTerminatorKind::CheckCallOutcome { .. }
                )
            {
                return Err(failure(
                    operation.source(),
                    MirUnitBuildError::InvalidCallPanicCheck(block_id),
                )
                .into());
            }

            // Concrete expansion owns these small identities and shared paths after consuming MIR.
            actions.push((
                block_id,
                *operation_id,
                role,
                *ty,
                place.clone(),
                operation.source().clone(),
                operation.cleanup_execution().cloned().ok_or_else(|| {
                    failure(
                        operation.source(),
                        MirUnitBuildError::InvalidCleanupExecution(*operation_id),
                    )
                })?,
            ));
        }
    }

    if actions.is_empty() {
        return Ok(unit);
    }

    // Retain the Arc-backed owner after the builder consumes the validated unit.
    let owner = unit.source().clone();

    let invalid = |cause| SyntheticLoweringError::LifecycleMir {
        owner: owner.clone(),
        cause,
    };

    let source = actions[0].5.clone();

    if unit.frame_descriptor().is_none() {
        return Err(failure(&source, MirUnitBuildError::ProtectedFrameMismatch).into());
    }

    let entry = unit.entry();
    let mut builder = MirUnitBuilder::from_unit(unit);

    // Keep descriptor-only state IDs visible to all nested expansion until rebuilding it.
    let mut generated_states = Vec::new();

    let lowerer = crate::synthetic::SyntheticLowerer::new(context);

    for (block, operation, role, concrete, place, source, execution) in actions {
        let storage_count = builder.storage_ids().len();
        let state = lowerer.next_lifecycle_state(&builder, &source)?;

        builder
            .set_cleanup_execution(operation, None)
            .map_err(|cause| failure(&source, cause))?;

        expand_action(
            context,
            &mut builder,
            (block, operation),
            &source,
            (role, concrete, place),
        )?;

        // Retain this action's generated slots with its checked lexical dependencies. Some
        // slots hold conditional values whose existing guards still govern initialization.
        let execution = bray_ir::MirFrameExecutionState::new(
            execution.lane_requirements().iter().copied(),
            execution
                .retained_storages()
                .iter()
                .copied()
                .chain(builder.storage_ids().skip(storage_count)),
        )
        .with_affinity(execution.affinity());

        generated_states.extend(
            builder
                .suspension_states()
                .filter(|(generated, _)| generated.raw() >= state.raw())
                .map(|(generated, resume)| {
                    MirFrameState::new(generated, resume, execution.clone())
                }),
        );
    }

    let descriptor = builder
        .take_frame_descriptor()
        .ok_or_else(|| failure(&source, MirUnitBuildError::ProtectedFrameMismatch))?;

    let mut states = descriptor.states().to_vec();
    states.extend(generated_states);
    states.sort_unstable_by_key(MirFrameState::state);

    let mut updated = MirFrameDescriptor::try_new(
        descriptor.frame(),
        descriptor.abi_version(),
        descriptor.frame_abi(),
        descriptor.result_type(),
        states,
    )
    .map_err(SyntheticLoweringError::FrameDescriptor)?;

    if let Some(cleanup) = descriptor.inactive_cleanup() {
        updated = updated.with_inactive_cleanup(cleanup);
    }

    if let Some((quiescence, destruction)) = descriptor.capture_abandonment() {
        updated = updated.with_capture_abandonment(quiescence, destruction);
    }

    builder.set_frame_descriptor(updated).map_err(invalid)?;

    builder.finish(entry).map_err(|cause| invalid(cause).into())
}

fn expand_action<C: SyntheticLoweringContext + ?Sized>(
    context: &C,
    builder: &mut MirUnitBuilder,
    location: (MirBlockId, MirOperationId),
    source: &MirSourceAnchor,
    action: (MirGeneratedLifecycleRole, TypeId, MirPlace),
) -> Result<(), C::Error> {
    let (block, operation) = location;

    let (role, concrete, place) = action;

    let terminator = builder
        .take_terminator(block)
        .map_err(|cause| failure(source, cause))?;

    let MirTerminatorKind::CheckCallOutcome {
        completed,
        panicked,
        cancelled,
    } = terminator.kind()
    else {
        return Err(failure(source, MirUnitBuildError::InvalidCallPanicCheck(block)).into());
    };

    let boolean = context.representation_type(RepresentationRole::ScalarBool)?;
    let unit = context.representation_type(RepresentationRole::Unit)?;

    let outcome = crate::cleanup_outcome::CleanupOutcome::replace_effect(
        builder,
        block,
        operation,
        source,
        boolean,
        panicked.report_type(),
        unit,
        builder.target().runtime_abi(),
    )
    .map_err(|cause| failure(source, cause))?;

    let lowerer = crate::synthetic::SyntheticLowerer::new(context);

    let finished = lowerer.resolve_concrete_lifecycle_action(
        builder, block, source, role, concrete, place, &outcome,
    )?;

    outcome
        .forward(builder, finished, source, (completed, *panicked, cancelled))
        .map_err(|cause| failure(source, cause).into())
}

pub(super) fn failure(
    source: &MirSourceAnchor,
    cause: MirUnitBuildError,
) -> SyntheticLoweringError {
    // Transformation failures retain provenance from source and imported executable templates.
    SyntheticLoweringError::SpecializedMir {
        source: source.clone(),
        cause,
    }
}
