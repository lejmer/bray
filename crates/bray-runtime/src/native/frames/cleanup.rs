use bray_runtime_abi::{NativeFrameMetadata, NativeRuntimeStatus};

use super::registry::{FrameRegistry, admit, registry, release};

/// Concrete frame identity and native layouts select one compatible cleanup capacity group.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(in crate::native) struct FrameShape {
    identity: [u8; 32],
    state_count: u32,
    size: usize,
    alignment: usize,
    completion_size: usize,
    completion_alignment: usize,
}

impl FrameShape {
    pub(in crate::native) fn of(metadata: &NativeFrameMetadata) -> Self {
        Self {
            identity: metadata.identity(),
            state_count: metadata.state_count(),
            size: metadata.size(),
            alignment: metadata.alignment(),
            completion_size: metadata.completion_size(),
            completion_alignment: metadata.completion_alignment(),
        }
    }
}

pub(super) enum FrameAvailability {
    Owned,
    Reserved { next: Option<usize> },
}

/// Credits outlive activation so an owner can retire its whole bundle even when phases are omitted.
pub(super) struct CleanupCapacity {
    descriptor: bray_runtime_model::ProtectedFrameDescriptor,
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
        let Some((shape, address)) = group.first() else {
            continue;
        };

        let descriptor = &state
            .frames
            .get(address)
            .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?
            .descriptor;

        if state
            .cleanup_capacity
            .get(shape)
            .is_some_and(|capacity| capacity.descriptor != *descriptor)
            || group.iter().any(|(_, address)| {
                state
                    .frames
                    .get(address)
                    .is_none_or(|storage| storage.descriptor != *descriptor)
            })
        {
            return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
        }

        state
            .cleanup_capacity
            .get(shape)
            .map_or(0, |capacity| capacity.credits)
            .checked_add(group.len())
            .ok_or(NativeRuntimeStatus::ALLOCATION_FAILURE)?;
    }

    // Every fallible step is complete. Publish the entire bundle under one registry lock.
    for (shape, address) in prepared.0.drain(..) {
        // The capacity group shares the state table already owned by its prepared frame.
        let descriptor = state
            .frames
            .get(&address)
            .unwrap_or_else(|| {
                unreachable!("prepared cleanup frames retain their checked descriptor")
            })
            .descriptor
            .clone();

        let capacity = state
            .cleanup_capacity
            .entry(shape)
            .or_insert(CleanupCapacity {
                descriptor,
                credits: 0,
                spent: 0,
                available: None,
            });

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
    with_matching_capacity(metadata, |state, shape| {
        let address = take_available(state, shape)?;

        // Removing an available frame preserves credits = spent + available frame count.
        state
            .cleanup_capacity
            .get_mut(&shape)
            .unwrap_or_else(|| {
                unreachable!("activating cleanup storage retains its capacity credit")
            })
            .spent += 1;

        Ok(address)
    })
}

/// Retires one credit when its owner's obligation is resolved, whether or not its phase ran.
pub(in crate::native) fn discharge_cleanup(
    metadata: &NativeFrameMetadata,
) -> Result<(), NativeRuntimeStatus> {
    let address = with_matching_capacity(metadata, |state, shape| {
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
            Some(take_available(state, shape)?)
        };

        let capacity = state.cleanup_capacity.get_mut(&shape).unwrap_or_else(|| {
            unreachable!("retiring cleanup capacity retains its entry until the last credit")
        });

        capacity.credits -= 1;

        if capacity.credits == 0 {
            state.cleanup_capacity.remove(&shape);
        }

        Ok(address)
    })?;

    // Releasing task reservations may drop runtime-owned state. Do it outside the registry lock.
    if let Some(address) = address {
        release(address);
    }

    Ok(())
}

fn with_matching_capacity<T>(
    metadata: &NativeFrameMetadata,
    operation: impl FnOnce(&mut FrameRegistry, FrameShape) -> Result<T, NativeRuntimeStatus>,
) -> Result<T, NativeRuntimeStatus> {
    let shape = FrameShape::of(metadata);

    let descriptor = {
        let state = registry()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // The descriptor shares already admitted state storage, so this clone cannot allocate.
        state
            .cleanup_capacity
            .get(&shape)
            .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?
            .descriptor
            .clone()
    };

    // Generated callbacks run outside the registry mutex, including during mandatory cleanup.
    if !super::super::frame::NativeFrame::matches_metadata_states(&descriptor, metadata) {
        return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
    }

    let mut state = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    if state
        .cleanup_capacity
        .get(&shape)
        .is_none_or(|capacity| capacity.descriptor != descriptor)
    {
        return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
    }

    operation(&mut state, shape)
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

    static CALLBACK_LOCKED: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);
    static REPLACE_CAPACITY: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);
    static REPLACEMENT_FAILED: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);

    #[inline(never)]
    extern "C" fn equivalent_state(state: u32) -> bray_runtime_abi::NativeFrameState {
        std::hint::black_box(state);

        if registry().try_lock().is_err() {
            CALLBACK_LOCKED.store(true, std::sync::atomic::Ordering::Relaxed);
        }

        native_origin_frame_state(state)
    }

    extern "C" fn different_state(state: u32) -> bray_runtime_abi::NativeFrameState {
        bray_runtime_abi::NativeFrameState::new(
            bray_runtime_abi::NativeFrameAffinity::ORIGIN_THREAD,
            if state == 0 {
                bray_runtime_abi::NativeLaneRequirements::COMPUTE
            } else {
                bray_runtime_abi::NativeLaneRequirements::NONE
            },
        )
    }

    fn with_state(
        identity: u8,
        state: bray_runtime_abi::NativeFrameStateCallback,
    ) -> NativeFrameMetadata {
        NativeFrameMetadata::new([identity; 32], 2, 256, 64, 0, 1, state)
    }

    #[test]
    fn equivalent_callbacks_share_capacity_and_claims_without_allocation_or_locked_callbacks() {
        let _isolation = test_runtime_isolation();
        CALLBACK_LOCKED.store(false, std::sync::atomic::Ordering::Relaxed);
        let original = metadata(194);
        let equivalent = with_state(194, equivalent_state);
        assert_ne!(original.state() as usize, equivalent.state() as usize);
        super::admit_cleanup([original, equivalent].into_iter().map(Ok)).unwrap();
        let ordinary = admit(original).unwrap();

        with_allocation_failure(|| {
            for incoming in [equivalent, original] {
                let address = super::activate_cleanup(&incoming).unwrap();

                assert!(
                    claim(address, NativeFrameEntry::Body, &equivalent)
                        .unwrap()
                        .is_some()
                );

                release(address);
                super::discharge_cleanup(&equivalent).unwrap();
            }

            assert!(
                claim(ordinary, NativeFrameEntry::Body, &equivalent)
                    .unwrap()
                    .is_some()
            );

            release(ordinary);
        });

        assert!(!CALLBACK_LOCKED.load(std::sync::atomic::Ordering::Relaxed));
        let state = registry().lock().unwrap();
        assert!(state.frames.is_empty());
        assert!(state.cleanup_capacity.is_empty());
    }

    #[test]
    fn conflicting_state_requirements_roll_back_bundles_and_preserve_existing_capacity() {
        let _isolation = test_runtime_isolation();
        let original = metadata(195);
        let incompatible = with_state(195, different_state);

        assert_eq!(
            super::admit_cleanup([original, incompatible].into_iter().map(Ok)),
            Err(NativeRuntimeStatus::INVALID_ARGUMENT)
        );

        assert!(registry().lock().unwrap().frames.is_empty());
        super::admit_cleanup([original].into_iter().map(Ok)).unwrap();

        assert_eq!(
            super::admit_cleanup([metadata(196), incompatible].into_iter().map(Ok)),
            Err(NativeRuntimeStatus::INVALID_ARGUMENT)
        );

        with_allocation_failure(|| {
            assert_eq!(
                super::activate_cleanup(&incompatible),
                Err(NativeRuntimeStatus::INVALID_ARGUMENT)
            );

            assert_eq!(
                super::discharge_cleanup(&incompatible),
                Err(NativeRuntimeStatus::INVALID_ARGUMENT)
            );

            let address = super::activate_cleanup(&original).unwrap();

            assert!(matches!(
                claim(address, NativeFrameEntry::Body, &incompatible),
                Err(NativeRuntimeStatus::INVALID_ARGUMENT)
            ));

            assert!(
                claim(address, NativeFrameEntry::Body, &original)
                    .unwrap()
                    .is_some()
            );

            release(address);
            super::discharge_cleanup(&original).unwrap();
        });

        let state = registry().lock().unwrap();
        assert!(state.frames.is_empty());
        assert!(state.cleanup_capacity.is_empty());
    }

    extern "C" fn replacing_state(state: u32) -> bray_runtime_abi::NativeFrameState {
        if REPLACE_CAPACITY.swap(false, std::sync::atomic::Ordering::Relaxed) {
            let retired = super::discharge_cleanup(&metadata(197));

            let admitted =
                super::admit_cleanup([with_state(197, different_state)].into_iter().map(Ok));

            REPLACEMENT_FAILED.store(
                retired.is_err() || admitted.is_err(),
                std::sync::atomic::Ordering::Relaxed,
            );
        }

        native_origin_frame_state(state)
    }

    #[test]
    fn capacity_changed_during_metadata_validation_is_rechecked_before_activation() {
        let _isolation = test_runtime_isolation();
        super::admit_cleanup([metadata(197)].into_iter().map(Ok)).unwrap();
        REPLACEMENT_FAILED.store(false, std::sync::atomic::Ordering::Relaxed);
        REPLACE_CAPACITY.store(true, std::sync::atomic::Ordering::Relaxed);

        assert_eq!(
            super::activate_cleanup(&with_state(197, replacing_state)),
            Err(NativeRuntimeStatus::INVALID_ARGUMENT)
        );

        assert!(!REPLACEMENT_FAILED.load(std::sync::atomic::Ordering::Relaxed));
        let replacement = with_state(197, different_state);

        with_allocation_failure(|| {
            release(super::activate_cleanup(&replacement).unwrap());
            super::discharge_cleanup(&replacement).unwrap();
        });

        assert!(registry().lock().unwrap().frames.is_empty());
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
