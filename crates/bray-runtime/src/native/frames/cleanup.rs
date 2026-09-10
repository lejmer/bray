use bray_runtime_abi::{NativeFrameMetadata, NativeRuntimeStatus};

use super::registry::{FrameRegistry, admit, registry, release};

/// Process-local identity includes every field required to use the prepared task records.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct FrameShape {
    identity: [u8; 32],
    state_count: u32,
    size: usize,
    alignment: usize,
    completion_size: usize,
    completion_alignment: usize,
    state: usize,
}

impl FrameShape {
    pub(super) fn of(metadata: &NativeFrameMetadata) -> Self {
        Self {
            identity: metadata.identity(),
            state_count: metadata.state_count(),
            size: metadata.size(),
            alignment: metadata.alignment(),
            completion_size: metadata.completion_size(),
            completion_alignment: metadata.completion_alignment(),
            // Function identity is local to this runtime registry, never a persisted contract.
            state: metadata.state() as usize,
        }
    }
}

pub(super) enum FrameAvailability {
    Owned,
    Reserved { next: Option<usize> },
}

/// Credits outlive activation so an owner can retire its whole bundle even when phases are omitted.
#[derive(Default)]
pub(super) struct CleanupCapacity {
    credits: usize,
    spent: usize,
    available: Option<usize>,
}

/// Keeps partially prepared bundles unavailable to cleanup until admission fully succeeds.
struct PreparedFrames(Vec<(FrameShape, usize)>);

impl Drop for PreparedFrames {
    fn drop(&mut self) {
        for (_, address) in self.0.drain(..) {
            release(address);
        }
    }
}

pub(in crate::native) fn admit_cleanup(
    metadata: impl ExactSizeIterator<Item = Result<NativeFrameMetadata, NativeRuntimeStatus>>,
) -> Result<(), NativeRuntimeStatus> {
    let mut prepared = PreparedFrames(Vec::new());

    crate::allocation::reserve_vec_entries(&mut prepared.0, metadata.len())
        .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

    for frame in metadata {
        let frame = frame?;
        prepared.0.push((FrameShape::of(&frame), admit(frame)?));
    }

    prepared.0.sort_unstable_by_key(|(shape, _)| *shape);

    let mut state = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    crate::allocation::reserve_map_entries(&mut state.cleanup_capacity, prepared.0.len())
        .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

    for group in prepared.0.chunk_by(|left, right| left.0 == right.0) {
        let Some((shape, _)) = group.first() else {
            continue;
        };

        state
            .cleanup_capacity
            .get(shape)
            .map_or(0, |capacity| capacity.credits)
            .checked_add(group.len())
            .ok_or(NativeRuntimeStatus::ALLOCATION_FAILURE)?;
    }

    // Every fallible step is complete. Publish the entire bundle under one registry lock.
    for (shape, address) in prepared.0.drain(..) {
        let capacity = state.cleanup_capacity.entry(shape).or_default();
        capacity.credits += 1;
        let next = capacity.available.replace(address);

        state
            .frames
            .get_mut(&address)
            .unwrap_or_else(|| {
                unreachable!("prepared cleanup frames retain their registry ownership")
            })
            .availability = FrameAvailability::Reserved { next };
    }

    Ok(())
}

pub(in crate::native) fn activate_cleanup(
    metadata: &NativeFrameMetadata,
) -> Result<usize, NativeRuntimeStatus> {
    let shape = FrameShape::of(metadata);

    let mut state = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let address = take_available(&mut state, shape)?;

    // Removing an available frame preserves credits = spent + available frame count.
    state
        .cleanup_capacity
        .get_mut(&shape)
        .unwrap_or_else(|| unreachable!("activating cleanup storage retains its capacity credit"))
        .spent += 1;

    Ok(address)
}

/// Retires one credit when its owner's obligation is resolved, whether or not its phase ran.
pub(in crate::native) fn discharge_cleanup(
    metadata: &NativeFrameMetadata,
) -> Result<(), NativeRuntimeStatus> {
    let shape = FrameShape::of(metadata);

    let address = {
        let mut state = registry()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let capacity = state
            .cleanup_capacity
            .get_mut(&shape)
            .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

        // Credits are fungible. Retiring a spent credit first can leave excess unused storage,
        // but cannot take capacity needed by another live owner whose phase has not run.
        let address = if capacity.spent != 0 {
            capacity.spent -= 1;

            None
        } else {
            Some(take_available(&mut state, shape)?)
        };

        let capacity = state.cleanup_capacity.get_mut(&shape).unwrap_or_else(|| {
            unreachable!("retiring cleanup capacity retains its entry until the last credit")
        });

        capacity.credits -= 1;

        if capacity.credits == 0 {
            state.cleanup_capacity.remove(&shape);
        }

        address
    };

    // Releasing task reservations may drop runtime-owned state. Do it outside the registry lock.
    if let Some(address) = address {
        release(address);
    }

    Ok(())
}

fn take_available(
    state: &mut FrameRegistry,
    shape: FrameShape,
) -> Result<usize, NativeRuntimeStatus> {
    let capacity = state
        .cleanup_capacity
        .get_mut(&shape)
        .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

    let address = capacity
        .available
        .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

    let storage = state.frames.get_mut(&address).unwrap_or_else(|| {
        unreachable!("available cleanup frames retain their registry ownership")
    });

    let FrameAvailability::Reserved { next } = storage.availability else {
        unreachable!("available cleanup frame links only contain reserved storage")
    };

    storage.availability = FrameAvailability::Owned;
    capacity.available = next;

    Ok(address)
}

#[cfg(test)]
mod tests {
    use super::super::registry::{admit, claim, registry, release};
    use crate::native::state::test_runtime_isolation;
    use crate::test_support::{
        native_origin_frame_state, with_allocation_failure, with_allocation_failure_after,
    };
    use bray_runtime_abi::{NativeFrameEntry, NativeFrameMetadata, NativeRuntimeStatus};

    fn metadata(identity: u8) -> NativeFrameMetadata {
        NativeFrameMetadata::new([identity; 32], 2, 256, 64, 0, 1, native_origin_frame_state)
    }

    #[test]
    fn cleanup_bundles_activate_exact_shapes_without_allocating_or_losing_other_owners() {
        let _isolation = test_runtime_isolation();
        let first = metadata(180);
        let second = metadata(181);
        let ordinary = admit(first).unwrap();
        super::admit_cleanup([first, second, first].into_iter().map(Ok)).unwrap();

        with_allocation_failure(|| {
            let wrong_size =
                NativeFrameMetadata::new([180; 32], 2, 512, 64, 0, 1, native_origin_frame_state);

            assert_eq!(
                super::activate_cleanup(&wrong_size),
                Err(NativeRuntimeStatus::INVALID_ARGUMENT)
            );

            for metadata in [second, first, first] {
                let address = super::activate_cleanup(&metadata).unwrap();
                assert_ne!(address, ordinary);
                assert_eq!(address % 64, 0);

                assert!(
                    claim(address, NativeFrameEntry::Body, &metadata)
                        .unwrap()
                        .is_some()
                );

                release(address);
            }

            assert_eq!(
                super::activate_cleanup(&first),
                Err(NativeRuntimeStatus::INVALID_ARGUMENT)
            );

            assert!(
                claim(ordinary, NativeFrameEntry::Body, &first)
                    .unwrap()
                    .is_some()
            );

            release(ordinary);

            for metadata in [first, second, first] {
                super::discharge_cleanup(&metadata).unwrap();
            }
        });

        let state = registry().lock().unwrap();
        assert!(state.frames.is_empty());
        assert!(state.cleanup_capacity.is_empty());
    }

    #[test]
    fn incomplete_bundle_failure_never_publishes_partial_capacity() {
        let _isolation = test_runtime_isolation();
        let existing = metadata(182);
        super::admit_cleanup([existing].into_iter().map(Ok)).unwrap();
        let mut completed = false;

        for allowed in 0..256 {
            let result = with_allocation_failure_after(allowed, || {
                super::admit_cleanup([metadata(183), metadata(184)].into_iter().map(Ok))
            });

            match result {
                Ok(()) => {
                    for frame in [metadata(183), metadata(184)] {
                        with_allocation_failure(|| {
                            release(super::activate_cleanup(&frame).unwrap());
                            super::discharge_cleanup(&frame).unwrap();
                        });
                    }

                    completed = true;
                }
                Err(status) => {
                    assert_eq!(status, NativeRuntimeStatus::ALLOCATION_FAILURE);
                    let state = registry().lock().unwrap();
                    assert_eq!(state.frames.len(), 1);
                    assert_eq!(state.cleanup_capacity.len(), 1);
                }
            }

            if completed {
                break;
            }
        }

        assert!(completed);

        with_allocation_failure(|| {
            release(super::activate_cleanup(&existing).unwrap());
            super::discharge_cleanup(&existing).unwrap();
        });

        let state = registry().lock().unwrap();
        assert!(state.frames.is_empty());
        assert!(state.cleanup_capacity.is_empty());
    }

    #[test]
    fn ordinary_entry_points_cannot_consume_or_release_unactivated_cleanup_storage() {
        let _isolation = test_runtime_isolation();
        let frame = metadata(185);
        super::admit_cleanup([frame].into_iter().map(Ok)).unwrap();
        let address = *registry().lock().unwrap().frames.keys().next().unwrap();

        with_allocation_failure(|| {
            assert!(matches!(
                claim(address, NativeFrameEntry::Body, &frame),
                Err(NativeRuntimeStatus::INVALID_ARGUMENT)
            ));

            release(address);
            assert_eq!(super::activate_cleanup(&frame), Ok(address));
            release(address);
            super::discharge_cleanup(&frame).unwrap();
        });

        assert!(registry().lock().unwrap().frames.is_empty());
    }

    #[test]
    fn omitted_phases_preserve_capacity_across_owner_release_orders() {
        let _isolation = test_runtime_isolation();
        let phases = [metadata(186), metadata(187)];

        let orders = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];

        for order in orders {
            for executed in 0_u8..64 {
                for _ in 0..3 {
                    super::admit_cleanup(phases.into_iter().map(Ok)).unwrap();
                }

                with_allocation_failure(|| {
                    let mut active = if executed & 1 != 0 {
                        Some(super::activate_cleanup(&phases[0]).unwrap())
                    } else {
                        None
                    };

                    for owner in order {
                        for (phase, metadata) in phases.iter().enumerate() {
                            if executed & (1 << (owner * 2 + phase)) != 0 {
                                let address = if owner == 0 && phase == 0 {
                                    active.take().unwrap()
                                } else {
                                    super::activate_cleanup(metadata).unwrap()
                                };

                                assert!(
                                    claim(address, NativeFrameEntry::Body, metadata)
                                        .unwrap()
                                        .is_some()
                                );

                                release(address);
                            }
                        }

                        for metadata in &phases {
                            super::discharge_cleanup(metadata).unwrap();
                        }
                    }
                });

                let state = registry().lock().unwrap();

                assert!(
                    state.frames.is_empty(),
                    "order {order:?}, phases {executed}"
                );

                assert!(
                    state.cleanup_capacity.is_empty(),
                    "order {order:?}, phases {executed}"
                );
            }
        }
    }

    #[test]
    fn late_admission_keeps_unused_capacity_after_another_owner_retires_a_spent_credit() {
        let _isolation = test_runtime_isolation();
        let frame = metadata(188);
        super::admit_cleanup([frame, frame].into_iter().map(Ok)).unwrap();
        let active = super::activate_cleanup(&frame).unwrap();

        with_allocation_failure(|| super::discharge_cleanup(&frame).unwrap());
        super::admit_cleanup([frame].into_iter().map(Ok)).unwrap();

        with_allocation_failure(|| {
            let later = super::activate_cleanup(&frame).unwrap();
            super::discharge_cleanup(&frame).unwrap();
            release(active);
            release(later);
            super::discharge_cleanup(&frame).unwrap();
        });

        let state = registry().lock().unwrap();
        assert!(state.frames.is_empty());
        assert!(state.cleanup_capacity.is_empty());
    }

    #[test]
    fn activated_storage_survives_discharge_of_its_last_credit() {
        let _isolation = test_runtime_isolation();
        let frame = metadata(189);
        super::admit_cleanup([frame].into_iter().map(Ok)).unwrap();
        let active = super::activate_cleanup(&frame).unwrap();

        with_allocation_failure(|| {
            super::discharge_cleanup(&frame).unwrap();
            assert!(registry().lock().unwrap().cleanup_capacity.is_empty());

            assert!(
                claim(active, NativeFrameEntry::Body, &frame)
                    .unwrap()
                    .is_some()
            );

            release(active);

            assert_eq!(
                super::discharge_cleanup(&frame),
                Err(NativeRuntimeStatus::INVALID_ARGUMENT)
            );
        });

        assert!(registry().lock().unwrap().frames.is_empty());
    }

    #[test]
    fn credit_overflow_rolls_back_the_whole_prepared_bundle() {
        let _isolation = test_runtime_isolation();
        let frame = metadata(190);
        let shape = super::FrameShape::of(&frame);
        super::admit_cleanup([frame].into_iter().map(Ok)).unwrap();

        {
            let mut state = registry().lock().unwrap();
            let capacity = state.cleanup_capacity.get_mut(&shape).unwrap();
            capacity.credits = usize::MAX;
            capacity.spent = usize::MAX - 1;
        }

        assert_eq!(
            super::admit_cleanup([metadata(191), frame, frame].into_iter().map(Ok)),
            Err(NativeRuntimeStatus::ALLOCATION_FAILURE)
        );

        {
            let mut state = registry().lock().unwrap();
            assert_eq!(state.frames.len(), 1);
            assert_eq!(state.cleanup_capacity.len(), 1);
            let capacity = state.cleanup_capacity.get_mut(&shape).unwrap();
            assert_eq!(capacity.credits, usize::MAX);
            assert_eq!(capacity.spent, usize::MAX - 1);
            capacity.credits = 1;
            capacity.spent = 0;
        }

        with_allocation_failure(|| super::discharge_cleanup(&frame).unwrap());
        let state = registry().lock().unwrap();
        assert!(state.frames.is_empty());
        assert!(state.cleanup_capacity.is_empty());
    }
}
