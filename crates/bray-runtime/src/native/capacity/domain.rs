use std::collections::HashMap;
use std::sync::Mutex;

use bray_runtime_abi::{NativeCleanupStorage, NativeRuntimeStatus};

/// One shared service owns capacity independently of runtime schedulers and native images.
#[derive(Default)]
pub(in crate::native) struct CleanupCapacityDomain {
    groups: Mutex<HashMap<[u8; 32], Capacity>>,
}

#[derive(Default)]
struct Capacity {
    credits: usize,
    spent: usize,
    available: Vec<NativeCleanupStorage>,
}

impl CleanupCapacityDomain {
    pub(in crate::native) fn new() -> Self {
        Self::default()
    }

    /// Prepared owners remain private until the whole bundle can be published.
    pub(in crate::native) fn admit(
        &self,
        mut prepared: Vec<([u8; 32], NativeCleanupStorage)>,
    ) -> Result<(), NativeRuntimeStatus> {
        if prepared.iter().any(|(_, storage)| storage.is_empty()) {
            return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
        }

        prepared.sort_unstable_by_key(|(identity, _)| *identity);

        let mut groups = self
            .groups
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Err(status) = reserve_bundle(&mut groups, &prepared) {
            // Failed preparation can leave only empty new groups. Removing those invokes
            // no backing callbacks. The prepared owners drop after the mutex guard.
            groups.retain(|_, capacity| capacity.credits != 0);

            return Err(status);
        }

        // Preflight reserves every group and slot before publication begins.
        for (identity, storage) in prepared.drain(..) {
            let capacity = groups
                .get_mut(&identity)
                .expect("preflight reserves every capacity group");

            capacity.credits += 1;
            capacity.available.push(storage);
        }

        Ok(())
    }

    pub(in crate::native) fn activate(
        &self,
        identity: [u8; 32],
    ) -> Result<NativeCleanupStorage, NativeRuntimeStatus> {
        let mut groups = self
            .groups
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let capacity = groups
            .get_mut(&identity)
            .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

        let storage = capacity
            .available
            .pop()
            .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

        // Every available entry already owns a credit, so this increment cannot overflow.
        capacity.spent += 1;

        Ok(storage)
    }

    pub(in crate::native) fn discharge(
        &self,
        identity: [u8; 32],
    ) -> Result<(), NativeRuntimeStatus> {
        let unused = {
            let mut groups = self
                .groups
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            let capacity = groups
                .get_mut(&identity)
                .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

            let unused = if capacity.spent != 0 {
                capacity.spent -= 1;

                None
            } else {
                Some(
                    capacity
                        .available
                        .pop()
                        .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?,
                )
            };

            capacity.credits -= 1;

            if capacity.credits == 0 {
                // A final discharge has detached all owned storage before removing its group.
                groups.remove(&identity);
            }

            unused
        };

        // Releasing backing or its provider may reenter the same domain.
        drop(unused);

        Ok(())
    }
}

fn reserve_bundle(
    groups: &mut HashMap<[u8; 32], Capacity>,
    prepared: &[([u8; 32], NativeCleanupStorage)],
) -> Result<(), NativeRuntimeStatus> {
    crate::allocation::reserve_map_entries(groups, prepared.len())
        .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

    for group in prepared.chunk_by(|left, right| left.0 == right.0) {
        let Some((identity, _)) = group.first() else {
            continue;
        };

        let capacity = groups.entry(*identity).or_default();

        capacity
            .credits
            .checked_add(group.len())
            .ok_or(NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        crate::allocation::reserve_vec_entries(&mut capacity.available, group.len())
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_abi::{NativeCleanupStorage, NativeProviderRetention, NativeRuntimeStatus};

    use super::CleanupCapacityDomain;
    use crate::test_support::{with_allocation_failure, with_allocation_failure_after};

    static DOMAIN: OnceLock<CleanupCapacityDomain> = OnceLock::new();
    static RELEASED: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn release(_: usize, _: usize) {
        let domain = DOMAIN.get().unwrap();

        assert!(
            domain.groups.try_lock().is_ok(),
            "release must permit domain reentry"
        );

        RELEASED.fetch_add(1, Ordering::Relaxed);
    }

    fn backing(address: usize) -> NativeCleanupStorage {
        NativeCleanupStorage::owned(
            NonZeroUsize::new(address).unwrap(),
            0,
            release,
            NativeProviderRetention::empty(),
        )
    }

    #[test]
    fn atomic_bundles_preserve_multiplicity_and_transferred_ownership_without_cleanup_allocation() {
        let domain = DOMAIN.get_or_init(CleanupCapacityDomain::new);

        domain
            .admit(vec![([1; 32], backing(1)), ([1; 32], backing(2))])
            .unwrap();

        let prepared = vec![([2; 32], backing(3)), ([3; 32], backing(4))];

        assert_eq!(
            with_allocation_failure_after(1, || domain.admit(prepared)),
            Err(NativeRuntimeStatus::ALLOCATION_FAILURE),
        );

        assert_eq!(RELEASED.load(Ordering::Relaxed), 2);
        assert_eq!(domain.groups.lock().unwrap().len(), 1);
        assert!(domain.activate([2; 32]).is_err());
        assert!(domain.activate([3; 32]).is_err());

        let transferred = with_allocation_failure(|| {
            let transferred = domain.activate([1; 32]).unwrap();
            domain.discharge([1; 32]).unwrap();
            assert_eq!(RELEASED.load(Ordering::Relaxed), 2);
            domain.discharge([1; 32]).unwrap();
            assert_eq!(RELEASED.load(Ordering::Relaxed), 3);
            assert!(domain.activate([1; 32]).is_err());
            assert!(domain.discharge([1; 32]).is_err());

            transferred
        });

        assert!(domain.groups.lock().unwrap().is_empty());
        drop(transferred);
        assert_eq!(RELEASED.load(Ordering::Relaxed), 4);
    }
}
