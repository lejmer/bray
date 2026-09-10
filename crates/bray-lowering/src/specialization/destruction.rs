use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockKind, MirEdge, MirGeneratedLifecycleRole, MirImmediateValue, MirOperand,
    MirOperationKind, MirPlace, MirProjectionKind, MirStorageKind, MirTaskTerminalState,
    MirTerminatorKind, MirUnit, MirUnitBuilder, MirUnitKey, MirUnitKind,
};
use bray_symbols::{BorrowKind, CallableExecution, TypeData, TypeId};

use crate::{SyntheticLoweringContext, SyntheticLoweringError};

/// Adapts a consuming destructor template to the borrowed, complete destruction operation.
pub fn specialize_destruction_body<C: SyntheticLoweringContext + ?Sized>(
    context: &C,
    unit: MirUnit,
    key: MirUnitKey,
    ty: TypeId,
) -> Result<MirUnit, C::Error> {
    if !matches!(&key, MirUnitKey::GeneratedLifecycle(key) if key.role() == MirGeneratedLifecycleRole::Destroy)
        || unit.kind().protected_frame().is_some()
    {
        return Err(SyntheticLoweringError::UnsupportedLifecycleRole(
            MirGeneratedLifecycleRole::Destroy,
        )
        .into());
    }

    let cleanup = context.cleanup_type_execution(ty)?;

    let asynchronous = cleanup
        .destruction_execution()
        .ok_or(SyntheticLoweringError::UnresolvedType(ty))?
        == CallableExecution::Asynchronous;

    let frame =
        asynchronous.then(|| crate::identity::protected_frame_identity(&key, unit.target()));

    // The builder consumes the unit while failures retain its Arc-backed provenance.
    let owner = unit.source().clone();

    let invalid = |cause| SyntheticLoweringError::LifecycleMir {
        owner: owner.clone(),
        cause,
    };

    let body = unit.entry();

    let source = unit
        .block(body)
        .ok_or_else(|| invalid(bray_ir::MirUnitBuildError::MissingBlock(body)))?
        .source()
        .clone();

    let receiver = unit
        .storages_with_ids()
        .find(|(_, storage)| storage.kind() == &MirStorageKind::Parameter(0))
        .map(|(id, storage)| (id, storage.ty()));

    let receiver_type = receiver.map_or(ty, |(_, ty)| ty);

    let pointer = context
        .semantic_values()
        .intern_type(TypeData::Borrow {
            kind: BorrowKind::Mutable,
            target: receiver_type,
        })
        .map_err(SyntheticLoweringError::SemanticValue)?;

    let exits = unit
        .blocks_with_ids()
        .filter_map(|(id, block)| {
            matches!(
                block.terminator().kind(),
                MirTerminatorKind::Return(_)
                    | MirTerminatorKind::PropagatePanic { .. }
                    | MirTerminatorKind::PropagateCancellation { .. }
            )
            .then(|| (id, block.terminator().clone()))
        })
        .collect::<Vec<_>>();

    let mut builder = MirUnitBuilder::for_specialization(
        unit,
        key,
        frame.map_or(MirUnitKind::Synchronous, MirUnitKind::ProtectedAsyncFrame),
    );

    let parameter = builder
        .push_storage(source.clone(), MirStorageKind::Parameter(0), pointer)
        .map_err(invalid)?;

    let entry = builder
        .push_block(source.clone(), MirBlockKind::Ordinary)
        .map_err(invalid)?;

    if let Some((receiver, _)) = receiver {
        builder
            .replace_storage_kind(receiver, MirStorageKind::Temporary)
            .map_err(invalid)?;

        let incoming = MirPlace::new(parameter, [], pointer)
            .project(MirProjectionKind::Dereference, receiver_type);

        // Ownership enters the checked body when destruction starts. Its partial-move guards stay intact.
        builder
            .push_operation(
                entry,
                source.clone(),
                MirOperationKind::Store {
                    kind: bray_ir::MirStoreKind::Initialize,
                    destination: MirPlace::new(receiver, [], receiver_type),
                    value: MirOperand::Move(incoming),
                },
                None,
            )
            .map_err(invalid)?;
    }

    builder
        .set_terminator(
            entry,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(body, [])),
        )
        .map_err(invalid)?;

    if let Some(frame) = frame {
        let completion = context.representation_type(RepresentationRole::Unit)?;

        for (block, terminator) in exits {
            publish_destructor_outcome(&mut builder, block, &terminator, completion)
                .map_err(invalid)?;
        }

        crate::synthetic::SyntheticLowerer { context }.attach_lifecycle_frame(
            &mut builder,
            frame,
            entry,
            MirPlace::new(parameter, [], pointer),
            &source,
        )?;
    }

    builder.finish(entry).map_err(|cause| invalid(cause).into())
}

fn publish_destructor_outcome(
    builder: &mut MirUnitBuilder,
    block: bray_ir::MirBlockId,
    terminator: &bray_ir::MirTerminator,
    completion: TypeId,
) -> Result<(), bray_ir::MirUnitBuildError> {
    let state = match terminator.kind() {
        MirTerminatorKind::Return(_) => MirTaskTerminalState::Completed(MirOperand::Immediate {
            value: MirImmediateValue::Unit,
            ty: completion,
        }),
        MirTerminatorKind::PropagatePanic { report, .. } => {
            MirTaskTerminalState::Panicked(report.clone())
        }
        MirTerminatorKind::PropagateCancellation { .. } => MirTaskTerminalState::Cancelled,
        _ => return Ok(()),
    };

    builder.take_terminator(block)?;

    crate::cleanup_await::finish_cleanup_frame(builder, block, terminator.source(), state)
}
