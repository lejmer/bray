use bray_ir::{
    MirAbandonmentAction, MirGeneratedLifecycleRole, MirOperationKind, MirUnit, MirUnitBuilder,
    MirUnitKey,
};

use crate::SyntheticLoweringError;

/// Retains a checked destructor's body, guards, and source under its abandonment specialization.
pub fn specialize_destructor_body(
    unit: MirUnit,
    key: MirUnitKey,
) -> Result<MirUnit, SyntheticLoweringError> {
    if !matches!(&key, MirUnitKey::GeneratedLifecycle(key)
        if key.role() == MirGeneratedLifecycleRole::Abandon(MirAbandonmentAction::Destructor))
        || unit.kind().protected_frame().is_some()
    {
        return Err(SyntheticLoweringError::UnsupportedLifecycleRole(
            MirGeneratedLifecycleRole::Abandon(MirAbandonmentAction::Destructor),
        ));
    }

    // Retain the Arc-backed owner after the builder consumes the validated unit.
    let owner = unit.source().clone();

    let invalid = |cause| SyntheticLoweringError::LifecycleMir {
        owner: owner.clone(),
        cause,
    };

    let entry = unit.entry();

    let kind = unit.kind().clone();

    MirUnitBuilder::for_specialization(unit, key, kind)
        .finish(entry)
        .map_err(invalid)
}

pub(super) fn resolve_remainder(unit: MirUnit) -> Result<MirUnit, SyntheticLoweringError> {
    let abandoned = matches!(unit.key(), MirUnitKey::GeneratedLifecycle(key)
        if key.role() == MirGeneratedLifecycleRole::Abandon(MirAbandonmentAction::Destructor));

    let mut replacements = Vec::new();

    for (id, operation) in unit.operations_with_ids() {
        let MirOperationKind::DestructorRemainder { role, place } = operation.kind() else {
            continue;
        };

        let operation =
            match role {
                MirGeneratedLifecycleRole::Destroy
                | MirGeneratedLifecycleRole::Cleanup(
                    bray_ir::MirCleanupPhase::LifecycleResolution,
                ) if abandoned => MirOperationKind::Abandon {
                    action: MirAbandonmentAction::Destroy,
                    place: place.clone(),
                },
                MirGeneratedLifecycleRole::Destroy => MirOperationKind::Destroy(place.clone()),
                MirGeneratedLifecycleRole::Cleanup(
                    bray_ir::MirCleanupPhase::LifecycleResolution,
                ) => MirOperationKind::Cleanup {
                    phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                    place: place.clone(),
                },
                _ => return Err(SyntheticLoweringError::UnsupportedLifecycleRole(*role)),
            };

        replacements.push((id, operation));
    }

    if replacements.is_empty() {
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

    for (id, operation) in replacements {
        builder
            .replace_effect(id, operation, None)
            .map_err(invalid)?;
    }

    builder.finish(entry).map_err(invalid)
}

#[cfg(test)]
mod tests {
    use bray_ir::{
        MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge, MirGeneratedLifecycleKey,
        MirOperand, MirPlace, MirSourceAnchor, MirStorageKind, MirTerminatorKind, MirUnitKind,
    };

    use super::{resolve_remainder, specialize_destructor_body};
    use bray_ir::{
        MirAbandonmentAction, MirGeneratedLifecycleRole, MirOperationKind, MirUnitBuilder,
        MirUnitKey,
    };

    #[test]
    fn specialization_retains_guards_and_changes_only_implicit_receiver_cleanup() {
        let bound = bray_testing::test_bound_unit(21);
        let source = MirSourceAnchor::from(bound.key().source());

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::Synchronous,
            bray_testing::test_mir_target(),
        );

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap();

        let broadcast = builder
            .push_block(source.clone(), MirBlockKind::CleanupBroadcast)
            .unwrap();

        let guarded = builder
            .push_block(source.clone(), MirBlockKind::LifecycleResolution)
            .unwrap();

        let cleanup = builder
            .push_block(source.clone(), MirBlockKind::LifecycleResolution)
            .unwrap();

        let exit = builder
            .push_block(source.clone(), MirBlockKind::LifecycleResolution)
            .unwrap();

        let ty = bray_testing::test_mir_type();

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, ty)
            .unwrap();

        let place = MirPlace::new(storage, [], ty);

        let guard = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, ty)
            .unwrap();

        builder
            .set_terminator(
                entry,
                source.clone(),
                MirTerminatorKind::BeginCleanup(MirCleanupEdge::new(
                    MirCleanupPhase::TaskCancellation,
                    MirEdge::new(broadcast, []),
                )),
            )
            .unwrap();

        builder
            .set_terminator(
                broadcast,
                source.clone(),
                MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                    MirCleanupPhase::LifecycleResolution,
                    MirEdge::new(guarded, []),
                )),
            )
            .unwrap();

        builder
            .set_terminator(
                guarded,
                source.clone(),
                MirTerminatorKind::Branch {
                    condition: MirOperand::Copy(MirPlace::new(guard, [], ty)),
                    then_edge: MirEdge::new(cleanup, []),
                    else_edge: MirEdge::new(exit, []),
                },
            )
            .unwrap();

        let kinds = [
            MirOperationKind::Finalize(place.clone()),
            MirOperationKind::Destroy(place.clone()),
            MirOperationKind::DestructorRemainder {
                role: MirGeneratedLifecycleRole::Destroy,
                place: place.clone(),
            },
            MirOperationKind::DestructorRemainder {
                role: MirGeneratedLifecycleRole::Cleanup(MirCleanupPhase::LifecycleResolution),
                place: place.clone(),
            },
        ];

        let operations = kinds
            .iter()
            .map(|kind| {
                builder
                    .push_operation(cleanup, source.clone(), kind.clone(), None)
                    .unwrap()
                    .operation()
            })
            .collect::<Vec<_>>();

        builder
            .set_terminator(
                cleanup,
                source.clone(),
                MirTerminatorKind::Goto(MirEdge::new(exit, [])),
            )
            .unwrap();

        builder
            .set_terminator(exit, source, MirTerminatorKind::Return(None))
            .unwrap();

        let original = builder.finish(entry).unwrap();

        let key = MirUnitKey::GeneratedLifecycle(MirGeneratedLifecycleKey::new(
            MirGeneratedLifecycleRole::Abandon(MirAbandonmentAction::Destructor),
            [21; 32],
        ));

        let specialized = specialize_destructor_body(original.clone(), key.clone()).unwrap();
        let abandoned = resolve_remainder(specialized).unwrap();
        let ordinary = resolve_remainder(original.clone()).unwrap();

        assert_eq!(abandoned.key(), &key);
        assert_eq!(ordinary.key(), original.key());
        assert_eq!(abandoned.source(), original.source());
        assert_eq!(abandoned.target(), original.target());
        assert_eq!(abandoned.storages(), original.storages());

        for (id, block) in original.blocks_with_ids() {
            assert_eq!(
                abandoned.block(id).unwrap().terminator(),
                block.terminator()
            );

            assert_eq!(
                abandoned.block(id).unwrap().operations(),
                block.operations()
            );
        }

        for (index, operation) in operations.into_iter().enumerate() {
            let abandoned = abandoned.operation(operation).unwrap();
            let ordinary = ordinary.operation(operation).unwrap();

            assert_eq!(
                abandoned.source(),
                original.operation(operation).unwrap().source()
            );

            if index < 2 {
                assert_eq!(abandoned.kind(), &kinds[index]);
                assert_eq!(ordinary.kind(), &kinds[index]);
            } else {
                assert_eq!(
                    abandoned.kind(),
                    &MirOperationKind::Abandon {
                        action: MirAbandonmentAction::Destroy,
                        place: place.clone(),
                    }
                );

                let expected = if index == 2 {
                    MirOperationKind::Destroy(place.clone())
                } else {
                    MirOperationKind::Cleanup {
                        phase: MirCleanupPhase::LifecycleResolution,
                        place: place.clone(),
                    }
                };

                assert_eq!(ordinary.kind(), &expected);
            }
        }
    }
}
