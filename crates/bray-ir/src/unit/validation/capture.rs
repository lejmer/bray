use crate::{
    MirAsyncOperation, MirBlockId, MirOperationKind, MirTaskTerminalState, MirTerminatorKind,
    MirUnit, MirUnitBuildError,
};

pub(super) fn validate_capture_entries(unit: &MirUnit) -> Result<(), MirUnitBuildError> {
    let Some(descriptor) = unit.frame_descriptor() else {
        return Ok(());
    };

    let Some((quiescence, destruction)) = descriptor.capture_abandonment() else {
        return Ok(());
    };

    let ordinary =
        unit.reachable_blocks(std::iter::once(unit.entry()).chain(descriptor.inactive_cleanup()));

    let quiescent = unit.reachable_blocks([quiescence]);
    let destroyed = unit.reachable_blocks([destruction]);

    if !ordinary.is_disjoint(&quiescent)
        || !ordinary.is_disjoint(&destroyed)
        || !quiescent.is_disjoint(&destroyed)
    {
        return Err(MirUnitBuildError::ProtectedFrameMismatch);
    }

    for id in ordinary {
        if publishes_completion(unit, id, |state| {
            matches!(state, MirTaskTerminalState::CapturesCompleted)
        })? {
            return Err(MirUnitBuildError::InvalidFrameStateEntry(id));
        }
    }

    for (reachable, synchronous) in [(quiescent, false), (destroyed, true)] {
        for id in reachable {
            let block = unit.block(id).ok_or(MirUnitBuildError::MissingBlock(id))?;

            if publishes_completion(unit, id, |state| {
                matches!(state, MirTaskTerminalState::Completed(_))
            })? || (synchronous
                && matches!(block.terminator().kind(), MirTerminatorKind::Suspend { .. }))
            {
                return Err(MirUnitBuildError::InvalidFrameStateEntry(id));
            }
        }
    }

    Ok(())
}

fn publishes_completion(
    unit: &MirUnit,
    block: MirBlockId,
    accepts: impl Fn(&MirTaskTerminalState) -> bool,
) -> Result<bool, MirUnitBuildError> {
    let block = unit
        .block(block)
        .ok_or(MirUnitBuildError::MissingBlock(block))?;

    for id in block.operations() {
        let operation = unit
            .operation(*id)
            .ok_or(MirUnitBuildError::MissingOperation(*id))?;

        if matches!(operation.kind(), MirOperationKind::Async(MirAsyncOperation::PublishTerminalState { state, .. })
            if accepts(state))
        {
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use crate::{
        MirAsyncOperation, MirOperationKind, MirTaskTerminalState, MirTerminatorKind, MirUnit,
        MirUnitBuildError,
    };
    use crate::{
        MirBlockKind, MirEdge, MirFrameDescriptor, MirFrameState, MirFrameStateId,
        MirImmediateValue, MirOperand, MirRuntimeReference, MirSourceAnchor, MirSuspensionKind,
        MirUnitBuilder, MirUnitKind,
    };
    use bray_runtime_interface::{
        ProtectedAsyncFrameId, ProtectedFrameAbiVersions, RuntimeAbiRole,
    };

    #[derive(Clone, Copy, Debug)]
    enum Scenario {
        Separate,
        BodyEntersQuiescence,
        QuiescenceEntersBodyInterior,
        SharedCaptureContinuation,
        CapturePublishesBodyValue,
        BodyPublishesCaptureCompletion,
        QuiescenceSuspends,
        DestructionSuspends,
    }

    fn capture_unit(scenario: Scenario) -> Result<MirUnit, MirUnitBuildError> {
        let bound = bray_testing::test_bound_unit(1899);
        let source = MirSourceAnchor::from(bound.key().source());
        let target = crate::test_support::test_target();
        let abi = target.runtime_abi();
        let frame = ProtectedAsyncFrameId::new([19; 32]);
        let values = bray_symbols::SemanticValueStore::try_new().unwrap();

        let ty = values
            .intern_type(bray_symbols::TypeData::tuple([]))
            .unwrap();

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::ProtectedAsyncFrame(frame),
            target,
        );

        let body = builder.push_block(source.clone(), MirBlockKind::Ordinary)?;
        let interior = builder.push_block(source.clone(), MirBlockKind::Ordinary)?;
        let quiescence = builder.push_block(source.clone(), MirBlockKind::Ordinary)?;
        let destruction = builder.push_block(source.clone(), MirBlockKind::Ordinary)?;

        let suspends = matches!(
            scenario,
            Scenario::QuiescenceSuspends | Scenario::DestructionSuspends
        );

        let resumed = builder.push_block(
            source.clone(),
            if suspends {
                MirBlockKind::LifecycleResolution
            } else {
                MirBlockKind::Ordinary
            },
        )?;

        let broadcast = if suspends {
            let broadcast = builder.push_block(source.clone(), MirBlockKind::CleanupBroadcast)?;

            let suspension =
                builder.push_block(source.clone(), MirBlockKind::LifecycleResolution)?;

            builder.set_terminator(
                broadcast,
                source.clone(),
                MirTerminatorKind::ContinueCleanup(crate::MirCleanupEdge::new(
                    crate::MirCleanupPhase::LifecycleResolution,
                    MirEdge::new(suspension, []),
                )),
            )?;

            builder.set_terminator(
                suspension,
                source.clone(),
                MirTerminatorKind::Suspend {
                    kind: MirSuspensionKind::Yield,
                    payload: None,
                    resume_state: MirFrameStateId::new(1),
                    resume: MirEdge::new(resumed, []),
                    cancellation: None,
                    registration: MirRuntimeReference::new(
                        RuntimeAbiRole::SuspensionRegistration,
                        abi,
                    ),
                    wake: MirRuntimeReference::new(RuntimeAbiRole::Wake, abi),
                },
            )?;

            Some(broadcast)
        } else {
            None
        };

        let mut states = vec![MirFrameState::new(MirFrameStateId::new(0), body, [], [])];

        for block in [body, interior, quiescence, destruction, resumed] {
            let terminator = match (block, scenario) {
                (block, Scenario::BodyEntersQuiescence) if block == body => {
                    MirTerminatorKind::Goto(MirEdge::new(quiescence, []))
                }
                (block, Scenario::QuiescenceEntersBodyInterior)
                    if block == body || block == quiescence =>
                {
                    MirTerminatorKind::Goto(MirEdge::new(interior, []))
                }
                (block, Scenario::SharedCaptureContinuation)
                    if block == quiescence || block == destruction =>
                {
                    MirTerminatorKind::Goto(MirEdge::new(resumed, []))
                }
                (block, Scenario::QuiescenceSuspends | Scenario::DestructionSuspends)
                    if block
                        == if matches!(scenario, Scenario::QuiescenceSuspends) {
                            quiescence
                        } else {
                            destruction
                        } =>
                {
                    MirTerminatorKind::BeginCleanup(crate::MirCleanupEdge::new(
                        crate::MirCleanupPhase::TaskCancellation,
                        MirEdge::new(broadcast.unwrap(), []),
                    ))
                }
                _ => MirTerminatorKind::Return(None),
            };

            let state = match (block, scenario) {
                (block, Scenario::CapturePublishesBodyValue) if block == quiescence => {
                    Some(MirTaskTerminalState::Completed(MirOperand::Immediate {
                        value: MirImmediateValue::Unit,
                        ty,
                    }))
                }
                (block, Scenario::BodyPublishesCaptureCompletion) if block == body => {
                    Some(MirTaskTerminalState::CapturesCompleted)
                }
                _ => None,
            };

            if let Some(state) = state {
                builder.push_operation(
                    block,
                    source.clone(),
                    MirOperationKind::Async(MirAsyncOperation::PublishTerminalState {
                        state,
                        runtime: MirRuntimeReference::new(RuntimeAbiRole::TerminalPublication, abi),
                    }),
                    None,
                )?;
            }

            builder.set_terminator(block, source.clone(), terminator)?;
        }

        if matches!(
            scenario,
            Scenario::QuiescenceSuspends | Scenario::DestructionSuspends
        ) {
            states.push(MirFrameState::new(MirFrameStateId::new(1), resumed, [], []));
        }

        builder.set_frame_descriptor(
            MirFrameDescriptor::try_new(
                frame,
                abi,
                ProtectedFrameAbiVersions::uniform(abi),
                ty,
                states,
            )
            .unwrap()
            .with_capture_abandonment(quiescence, destruction),
        )?;

        builder.finish(body)
    }

    #[test]
    fn capture_entries_are_isolated_from_each_other_and_from_body_execution() {
        assert!(capture_unit(Scenario::Separate).is_ok());
        let suspended = capture_unit(Scenario::QuiescenceSuspends);

        assert!(suspended.is_ok(), "{suspended:?}");

        for scenario in [
            Scenario::BodyEntersQuiescence,
            Scenario::QuiescenceEntersBodyInterior,
            Scenario::SharedCaptureContinuation,
            Scenario::CapturePublishesBodyValue,
            Scenario::BodyPublishesCaptureCompletion,
            Scenario::DestructionSuspends,
        ] {
            assert!(capture_unit(scenario).is_err(), "{scenario:?}");
        }
    }
}
