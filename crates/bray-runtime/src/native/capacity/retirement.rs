use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use bray_runtime_abi::{
    NativeProviderRetention, NativeProviderRetentionCallbacks, NativeProviderRetirement,
    NativeProviderRetirementCallbacks, NativeProviderRetirementObservation, NativeRuntimeStatus,
};

use super::domain::CleanupCapacityDomain;

static PROVIDERS: OnceLock<Mutex<HashMap<usize, Provider>>> = OnceLock::new();
static NEXT: AtomicUsize = AtomicUsize::new(1);
static REGISTRATION: NativeProviderRetirementCallbacks =
    NativeProviderRetirementCallbacks::new(acquire, begin, observe, release_registration);
static RETENTION: NativeProviderRetentionCallbacks =
    NativeProviderRetentionCallbacks::new(retain, release);

struct Provider {
    _domain: triomphe::Arc<CleanupCapacityDomain>,
    context: usize,
    teardown: extern "C" fn(usize) -> NativeRuntimeStatus,
    references: usize,
    registered: bool,
    requested: bool,
    running: bool,
    outcome: Option<NativeRuntimeStatus>,
}

impl Provider {
    fn start(&mut self) -> Option<(usize, extern "C" fn(usize) -> NativeRuntimeStatus)> {
        if !self.requested || self.references != 0 || self.running || self.outcome.is_some() {
            return None;
        }

        self.running = true;

        Some((self.context, self.teardown))
    }

    fn releasable(&self) -> bool {
        !self.registered
            && self.references == 0
            && !self.running
            && (!self.requested || self.outcome == Some(NativeRuntimeStatus::SUCCESS))
    }
}

fn providers() -> &'static Mutex<HashMap<usize, Provider>> {
    PROVIDERS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn register(
    domain: triomphe::Arc<CleanupCapacityDomain>,
    context: usize,
    teardown: extern "C" fn(usize) -> NativeRuntimeStatus,
    destination: &mut NativeProviderRetirement,
) -> Result<(), NativeRuntimeStatus> {
    if !destination.is_empty() {
        return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
    }

    let mut entries = providers()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    crate::allocation::reserve_map_entries(&mut entries, 1)
        .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

    let identity = NEXT
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .ok()
        .and_then(NonZeroUsize::new)
        .ok_or(NativeRuntimeStatus::ALLOCATION_FAILURE)?;

    entries.insert(
        identity.get(),
        Provider {
            _domain: domain,
            context,
            teardown,
            references: 0,
            registered: true,
            requested: false,
            running: false,
            outcome: None,
        },
    );

    drop(entries);
    *destination = NativeProviderRetirement::new(identity, &REGISTRATION);

    Ok(())
}

extern "C" fn acquire(
    context: usize,
    destination: &mut NativeProviderRetention,
) -> NativeRuntimeStatus {
    if !destination.is_empty() {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    }

    let Some(identity) = NonZeroUsize::new(context) else {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    };

    let mut entries = providers()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let Some(provider) = entries.get_mut(&context).filter(|provider| {
        provider.registered
            && !provider.running
            && provider.outcome.is_none()
            && (!provider.requested || provider.references > 0)
    }) else {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    };

    let references = provider
        .references
        .checked_add(1)
        .unwrap_or_else(|| std::process::abort());

    provider.references = references;
    *destination = NativeProviderRetention::new(identity, &RETENTION);

    NativeRuntimeStatus::SUCCESS
}

extern "C" fn retain(context: usize) {
    let mut entries = providers()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    // The source retention keeps this record live and contributes one reference.
    let provider = entries
        .get_mut(&context)
        .expect("live retention owns its provider record");

    assert_ne!(provider.references, 0);

    provider.references = provider
        .references
        .checked_add(1)
        .unwrap_or_else(|| std::process::abort());
}

extern "C" fn release(context: usize) {
    let (action, removed) = {
        let mut entries = providers()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Each callback releases one reference acquired from this resident record.
        let provider = entries
            .get_mut(&context)
            .expect("live retention owns its provider record");

        provider.references = provider
            .references
            .checked_sub(1)
            .expect("retention releases once");

        let action = provider.start();

        let removed = provider
            .releasable()
            .then(|| entries.remove(&context))
            .flatten();

        (action, removed)
    };

    drop(removed);

    if let Some(action) = action {
        complete(context, action);
    }
}

// Retirement permits extending existing ownership, but reaching zero starts teardown
// atomically with release and permanently closes acquisition.
extern "C" fn begin(context: usize) -> NativeRuntimeStatus {
    let action = {
        let mut entries = providers()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(provider) = entries
            .get_mut(&context)
            .filter(|provider| provider.registered)
        else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        provider.requested = true;
        let action = provider.start();

        if action.is_none() {
            return provider.outcome.unwrap_or(NativeRuntimeStatus::SUCCESS);
        }

        action
    };

    action.map_or(NativeRuntimeStatus::SUCCESS, |action| {
        complete(context, action)
    })
}

fn complete(
    context: usize,
    (provider_context, teardown): (usize, extern "C" fn(usize) -> NativeRuntimeStatus),
) -> NativeRuntimeStatus {
    let status = teardown(provider_context);

    let removed = {
        let mut entries = providers()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // An executing teardown pins its record even if the host drops its registration.
        let provider = entries
            .get_mut(&context)
            .expect("running teardown owns its provider record");

        provider.running = false;
        provider.outcome = Some(status);

        provider
            .releasable()
            .then(|| entries.remove(&context))
            .flatten()
    };

    drop(removed);

    status
}

extern "C" fn observe(context: usize) -> NativeProviderRetirementObservation {
    let entries = providers()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let Some(provider) = entries.get(&context) else {
        return NativeProviderRetirementObservation::new(
            0,
            NativeRuntimeStatus::INVALID_ARGUMENT,
            false,
        );
    };

    NativeProviderRetirementObservation::new(
        provider.references,
        provider.outcome.unwrap_or(NativeRuntimeStatus::SUCCESS),
        provider.outcome == Some(NativeRuntimeStatus::SUCCESS),
    )
}

extern "C" fn release_registration(context: usize) {
    let removed = {
        let mut entries = providers()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // The unique registration remains owned until this callback releases it.
        let provider = entries
            .get_mut(&context)
            .expect("registration owns its provider record");

        provider.registered = false;

        provider
            .releasable()
            .then(|| entries.remove(&context))
            .flatten()
    };

    drop(removed);
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Barrier, OnceLock};

    use bray_runtime_abi::{
        NativeCleanupStorage, NativeProviderRetention, NativeProviderRetirement,
        NativeRuntimeStatus,
    };

    use super::{CleanupCapacityDomain, register};

    static ENTERED: OnceLock<Barrier> = OnceLock::new();
    static RETURN: OnceLock<Barrier> = OnceLock::new();
    static TEARDOWNS: AtomicUsize = AtomicUsize::new(0);
    static ROLLBACK_TEARDOWNS: AtomicUsize = AtomicUsize::new(0);
    static BACKING_RELEASES: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn blocked_teardown(status: usize) -> NativeRuntimeStatus {
        // Taking the same registry lock proves teardown permits resident-service reentry.
        drop(super::providers().lock().unwrap());
        TEARDOWNS.fetch_add(1, Ordering::Relaxed);
        ENTERED.get().unwrap().wait();
        RETURN.get().unwrap().wait();

        if status == 0 {
            NativeRuntimeStatus::SUCCESS
        } else {
            NativeRuntimeStatus::PANICKED
        }
    }

    #[test]
    fn retirement_waits_for_references_and_callback_return_preserving_failure() {
        ENTERED.get_or_init(|| Barrier::new(2));
        RETURN.get_or_init(|| Barrier::new(2));

        for (ordinal, status) in [NativeRuntimeStatus::SUCCESS, NativeRuntimeStatus::PANICKED]
            .into_iter()
            .enumerate()
        {
            let mut registration = NativeProviderRetirement::empty();

            register(
                triomphe::Arc::new(CleanupCapacityDomain::new()),
                ordinal,
                blocked_teardown,
                &mut registration,
            )
            .unwrap();

            let mut retained = NativeProviderRetention::empty();

            let cloned = crate::test_support::with_allocation_failure(|| {
                assert_eq!(
                    registration.acquire(&mut retained),
                    NativeRuntimeStatus::SUCCESS
                );

                retained.clone()
            });

            assert_eq!(registration.observe().references(), 2);
            assert_eq!(registration.begin(), NativeRuntimeStatus::SUCCESS);
            let mut extended = NativeProviderRetention::empty();

            assert_eq!(
                crate::test_support::with_allocation_failure(|| registration.acquire(&mut extended)),
                NativeRuntimeStatus::SUCCESS
            );

            assert_eq!(registration.observe().references(), 3);
            drop(extended);
            drop(retained);
            assert_eq!(registration.observe().references(), 1);
            assert_eq!(TEARDOWNS.load(Ordering::Relaxed), ordinal);

            let worker = std::thread::spawn(move || drop(cloned));
            ENTERED.get().unwrap().wait();
            let running = registration.observe();
            let mut rejected = NativeProviderRetention::empty();
            let during_teardown = registration.acquire(&mut rejected);
            RETURN.get().unwrap().wait();
            worker.join().unwrap();

            assert_eq!(during_teardown, NativeRuntimeStatus::INVALID_ARGUMENT);
            assert!(rejected.is_empty());

            assert_eq!(
                registration.acquire(&mut rejected),
                NativeRuntimeStatus::INVALID_ARGUMENT
            );

            assert!(rejected.is_empty());
            assert_eq!(running.references(), 0);
            assert!(!running.is_complete());
            let finished = registration.observe();
            assert_eq!(finished.status(), status);

            assert_eq!(
                finished.is_complete(),
                status == NativeRuntimeStatus::SUCCESS
            );

            assert_eq!(registration.begin(), status);
            assert_eq!(TEARDOWNS.load(Ordering::Relaxed), ordinal + 1);
        }
    }

    extern "C" fn rollback_teardown(_: usize) -> NativeRuntimeStatus {
        ROLLBACK_TEARDOWNS.fetch_add(1, Ordering::Relaxed);

        NativeRuntimeStatus::SUCCESS
    }

    extern "C" fn release_backing(_: usize, _: usize) {
        // Dropping the final domain owner must not retain the provider registry lock.
        drop(super::providers().lock().unwrap());
        BACKING_RELEASES.fetch_add(1, Ordering::Relaxed);
    }

    #[test]
    fn dropping_unstarted_registration_releases_domain_backing_without_teardown() {
        let domain = triomphe::Arc::new(CleanupCapacityDomain::new());

        domain
            .admit(vec![(
                [91; 32],
                NativeCleanupStorage::owned(
                    NonZeroUsize::new(1).unwrap(),
                    0,
                    release_backing,
                    NativeProviderRetention::empty(),
                ),
            )])
            .unwrap();

        let mut registration = NativeProviderRetirement::empty();
        register(domain, 0, rollback_teardown, &mut registration).unwrap();

        crate::test_support::with_allocation_failure(|| drop(registration));

        assert_eq!(ROLLBACK_TEARDOWNS.load(Ordering::Relaxed), 0);
        assert_eq!(BACKING_RELEASES.load(Ordering::Relaxed), 1);
    }
}
