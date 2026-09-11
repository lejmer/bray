use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCallPanicEdge, MirEdge, MirFrameDescriptor, MirFrameEntry,
    MirFrameState, MirFrameStateId, MirGeneratedLifecycleRole, MirOperand, MirOperationId,
    MirOperationKind, MirPatternPredicate, MirPlace, MirProjectionKind, MirRunResultVariants,
    MirSourceAnchor, MirTerminatorKind, MirUnit, MirUnitBuildError, MirUnitBuilder,
};
use bray_symbols::{BorrowKind, CallableExecution, SymbolOrdinal, TypeData, TypeId};

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

            let (role, place) = match operation.kind() {
                MirOperationKind::Finalize(place) => (MirGeneratedLifecycleRole::Finalize, place),
                MirOperationKind::Destroy(place) => (MirGeneratedLifecycleRole::Destroy, place),
                MirOperationKind::Abandon { action, place } => {
                    (MirGeneratedLifecycleRole::Abandon(*action), place)
                }
                MirOperationKind::Cleanup { phase, place } => {
                    (MirGeneratedLifecycleRole::Cleanup(*phase), place)
                }
                _ => continue,
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

    let entry = unit.entry();
    let mut builder = MirUnitBuilder::from_unit(unit);
    let source = &actions[0].5;

    let first_new_state = builder
        .next_frame_state()
        .map_err(|cause| failure(source, cause))?
        .raw();

    let descriptor = builder
        .take_frame_descriptor()
        .ok_or_else(|| failure(source, MirUnitBuildError::ProtectedFrameMismatch))?;

    // Existing resumptions keep their IDs. New states inherit the checked frame execution context.
    let mut states = descriptor.states().to_vec();

    let lowerer = crate::synthetic::SyntheticLowerer::new(context);

    for (block, operation, role, concrete, place, source, execution) in actions {
        let storage_count = builder.storage_ids().len();
        let next = lowerer.next_lifecycle_state(&builder, &source)?;
        let state = MirFrameStateId::new(next.raw().max(first_new_state));

        builder
            .set_cleanup_execution(operation, None)
            .map_err(|cause| failure(&source, cause))?;

        expand_action(
            context,
            &mut builder,
            (block, operation),
            &source,
            (role, concrete, place),
            state,
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

        states.extend(
            builder
                .suspension_states()
                .filter(|(generated, _)| generated.raw() >= state.raw())
                .map(|(generated, resume)| {
                    MirFrameState::new(generated, resume, execution.clone())
                }),
        );
    }

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
    state: MirFrameStateId,
) -> Result<MirBlockId, C::Error> {
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

    let owner = crate::cleanup_await::CleanupOwner::for_action(
        context.compiler_known_symbols(),
        context.semantic_values(),
        role,
        concrete,
    )
    .map_err(SyntheticLoweringError::SemanticValue)?;

    match owner {
        Some(crate::cleanup_await::CleanupOwner::Future { entry, completion }) => {
            return super::future::expand_future_cleanup(
                context,
                builder,
                location,
                source,
                (entry, place, completion),
                state,
                (completed, *panicked, cancelled),
            );
        }
        Some(crate::cleanup_await::CleanupOwner::Task { completion }) => {
            return super::task::expand_task_cleanup(
                context,
                builder,
                location,
                source,
                (role, place, completion),
                state,
                (completed, *panicked, cancelled),
            );
        }
        None => {}
    }

    let ty = place.ty();

    let receiver = context
        .semantic_values()
        .intern_type(TypeData::Borrow {
            kind: BorrowKind::Mutable,
            target: ty,
        })
        .map_err(SyntheticLoweringError::SemanticValue)?;

    let borrow = builder
        .replace_effect(
            operation,
            MirOperationKind::Borrow {
                kind: BorrowKind::Mutable,
                place,
            },
            Some(receiver),
        )
        .map_err(|cause| failure(source, cause))?;

    let receiver = borrow
        .result()
        .ok_or_else(|| failure(source, MirUnitBuildError::MissingOperationResult(operation)))?;

    let completion = context.representation_type(RepresentationRole::Unit)?;
    let future = unary_type(context, RepresentationRole::Future, completion)?;
    let result = unary_type(context, RepresentationRole::RunResult, completion)?;

    let representation = context
        .compiler_known_symbols()
        .run_result_representation()
        .ok_or(SyntheticLoweringError::MissingRepresentation {
            role: RepresentationRole::RunResult,
            argument: None,
        })?;

    let variants = MirRunResultVariants::new(
        representation.completed_variant(),
        representation.panicked_variant(),
        representation.cancelled_variant(),
    );

    let boolean = context.representation_type(RepresentationRole::ScalarBool)?;

    let (block, rejected, future) = crate::cleanup_await::create_lifecycle_frame(
        builder,
        block,
        source,
        role,
        ty,
        MirOperand::Value(receiver),
        bray_bound_tree::BoundFutureConstruction::new(completion, future),
        boolean,
    )
    .map_err(|cause| failure(source, cause))?;

    let report =
        crate::frame_creation::allocation_panic(builder, rejected, source, panicked.report_type())
            .map_err(|cause| failure(source, cause))?;

    builder
        .set_terminator(
            rejected,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(panicked.target(), [report])),
        )
        .map_err(|cause| failure(source, cause))?;

    let (resume, result) = crate::cleanup_await::await_cleanup(
        builder,
        block,
        source,
        state,
        crate::cleanup_await::CleanupAwait::Frame(MirOperand::Move(future), MirFrameEntry::Body),
        result,
        variants,
    )
    .map_err(|cause| failure(source, cause))?;

    forward_outcome(
        builder,
        resume,
        source,
        result,
        variants,
        (completed, *panicked, cancelled),
    )
    .map_err(|cause| failure(source, cause))?;

    Ok(resume)
}

fn forward_outcome(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    result: MirPlace,
    variants: MirRunResultVariants,
    edges: (&MirEdge, MirCallPanicEdge, &MirEdge),
) -> Result<(), MirUnitBuildError> {
    let (completed, panicked, cancelled) = edges;

    let completed_bridge = builder.push_block(source.clone(), MirBlockKind::LifecycleResolution)?;
    let failed = builder.push_block(source.clone(), MirBlockKind::LifecycleResolution)?;
    let panic_bridge = builder.push_block(source.clone(), MirBlockKind::LifecycleResolution)?;

    let cancellation_bridge =
        builder.push_block(source.clone(), MirBlockKind::LifecycleResolution)?;

    // The original continuations retain their exact ownership arguments and cleanup phases.
    builder.set_terminator(
        completed_bridge,
        source.clone(),
        MirTerminatorKind::Goto(completed.clone()),
    )?;

    builder.set_terminator(
        cancellation_bridge,
        source.clone(),
        MirTerminatorKind::Goto(cancelled.clone()),
    )?;

    let report = result.project(
        MirProjectionKind::ActiveUnionPayloadElement {
            variant: variants.panicked(),
            ordinal: SymbolOrdinal::new(0),
        },
        panicked.report_type(),
    );

    builder.set_terminator(
        panic_bridge,
        source.clone(),
        MirTerminatorKind::Goto(MirEdge::new(panicked.target(), [MirOperand::Move(report)])),
    )?;

    builder.set_terminator(
        block,
        source.clone(),
        MirTerminatorKind::PatternBranch {
            subject: MirOperand::Copy(result.clone()),
            predicate: MirPatternPredicate::ActiveUnionVariant(variants.completed()),
            matched: MirEdge::new(completed_bridge, []),
            unmatched: MirEdge::new(failed, []),
        },
    )?;

    builder.set_terminator(
        failed,
        source.clone(),
        MirTerminatorKind::PatternBranch {
            subject: MirOperand::Copy(result),
            predicate: MirPatternPredicate::ActiveUnionVariant(variants.panicked()),
            matched: MirEdge::new(panic_bridge, []),
            unmatched: MirEdge::new(cancellation_bridge, []),
        },
    )
}

fn unary_type<C: SyntheticLoweringContext + ?Sized>(
    context: &C,
    role: RepresentationRole,
    argument: TypeId,
) -> Result<TypeId, C::Error> {
    context
        .compiler_known_symbols()
        .unary_representation_type(context.semantic_values(), role, argument)
        .map_err(SyntheticLoweringError::SemanticValue)?
        .ok_or_else(|| {
            SyntheticLoweringError::MissingRepresentation {
                role,
                argument: Some(argument),
            }
            .into()
        })
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
