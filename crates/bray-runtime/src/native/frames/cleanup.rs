use bray_runtime_abi::{NativeFrameMetadata, NativeRuntimeStatus};

use super::registry::{admit, registry, release};

/// Process-local identity includes every field required to use the prepared task records.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
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

/// Keeps partially prepared bundles unavailable to cleanup until admission fully succeeds.
struct PreparedFrames(Vec<usize>);

impl Drop for PreparedFrames {
    fn drop(&mut self) {
        for address in self.0.drain(..) {
            release(address);
        }
    }
}

pub(in crate::native) fn admit_cleanup(
    metadata: impl ExactSizeIterator<Item = NativeFrameMetadata>,
) -> Result<(), NativeRuntimeStatus> {
    let mut prepared = PreparedFrames(Vec::new());

    crate::allocation::reserve_vec_entries(&mut prepared.0, metadata.len())
        .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

    for frame in metadata {
        prepared.0.push(admit(frame)?);
    }

    let mut state = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    crate::allocation::reserve_map_entries(&mut state.available, prepared.0.len())
        .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

    // Every fallible step is complete. Publish the entire bundle under one registry lock.
    for address in prepared.0.drain(..) {
        let shape = FrameShape::of(&state.frames[&address].metadata);
        let next = state.available.insert(shape, address);

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

    let address = state
        .available
        .get(&shape)
        .copied()
        .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

    let storage = state.frames.get_mut(&address).unwrap_or_else(|| {
        unreachable!("available cleanup frames retain their registry ownership")
    });

    let FrameAvailability::Reserved { next } = storage.availability else {
        unreachable!("available cleanup frame links only contain reserved storage")
    };

    storage.availability = FrameAvailability::Owned;

    match next {
        Some(next) => {
            state.available.insert(shape, next);
        }
        None => {
            state.available.remove(&shape);
        }
    }

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
        super::admit_cleanup([first, second, first].into_iter()).unwrap();

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
        });

        let state = registry().lock().unwrap();
        assert!(state.frames.is_empty());
        assert!(state.available.is_empty());
    }

    #[test]
    fn incomplete_bundle_failure_never_publishes_partial_capacity() {
        let _isolation = test_runtime_isolation();
        let existing = metadata(182);
        super::admit_cleanup([existing].into_iter()).unwrap();
        let mut completed = false;

        for allowed in 0..256 {
            let result = with_allocation_failure_after(allowed, || {
                super::admit_cleanup([metadata(183), metadata(184)].into_iter())
            });

            match result {
                Ok(()) => {
                    for frame in [metadata(183), metadata(184)] {
                        with_allocation_failure(|| {
                            release(super::activate_cleanup(&frame).unwrap())
                        });
                    }

                    completed = true;
                }
                Err(status) => {
                    assert_eq!(status, NativeRuntimeStatus::ALLOCATION_FAILURE);
                    let state = registry().lock().unwrap();
                    assert_eq!(state.frames.len(), 1);
                    assert_eq!(state.available.len(), 1);
                }
            }

            if completed {
                break;
            }
        }

        assert!(completed);
        with_allocation_failure(|| release(super::activate_cleanup(&existing).unwrap()));
        let state = registry().lock().unwrap();
        assert!(state.frames.is_empty());
        assert!(state.available.is_empty());
    }

    #[test]
    fn ordinary_entry_points_cannot_consume_or_release_unactivated_cleanup_storage() {
        let _isolation = test_runtime_isolation();
        let frame = metadata(185);
        super::admit_cleanup([frame].into_iter()).unwrap();
        let address = *registry().lock().unwrap().frames.keys().next().unwrap();

        with_allocation_failure(|| {
            assert!(matches!(
                claim(address, NativeFrameEntry::Body, &frame),
                Err(NativeRuntimeStatus::INVALID_ARGUMENT)
            ));

            release(address);
            assert_eq!(super::activate_cleanup(&frame), Ok(address));
            release(address);
        });

        assert!(registry().lock().unwrap().frames.is_empty());
    }
}
