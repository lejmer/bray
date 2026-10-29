use bray_runtime_abi::{
    NativeProductHostDescriptor, NativeProductHostObservation, NativeProductHostOperation,
    NativeRuntimeStatus, NativeThreadStaticCleanupRegistration,
};

use super::resident::observe;

pub(super) fn product_key(descriptor: &NativeProductHostDescriptor) -> usize {
    std::ptr::from_ref(descriptor) as usize
}

pub(crate) fn control(
    descriptor: &NativeProductHostDescriptor,
    operation: NativeProductHostOperation,
) -> NativeProductHostObservation {
    observe(product_key(descriptor), operation)
}

#[expect(
    unsafe_code,
    reason = "Bray owns exact-thread membership and TLS cleanup"
)]
unsafe extern "C" {
    safe fn bray_runtime_thread_attachment_identity(
        descriptor: &'static NativeProductHostDescriptor,
    ) -> u64;
    safe fn bray_runtime_thread_static_cleanup_registration(
        registration: &NativeThreadStaticCleanupRegistration,
    ) -> NativeRuntimeStatus;
}

pub(crate) fn thread_attachment_identity(descriptor: &'static NativeProductHostDescriptor) -> u64 {
    bray_runtime_thread_attachment_identity(descriptor)
}

pub(crate) fn register_thread_static(
    registration: &NativeThreadStaticCleanupRegistration,
) -> NativeRuntimeStatus {
    bray_runtime_thread_static_cleanup_registration(registration)
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::sync::Arc;
    use std::sync::Barrier;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_abi::{
        NativeProductHostDescriptor, NativeProductHostOperation, NativeProductHostState,
        NativeProductHostStatus, NativeProductIdentity, NativeStaticDuration,
        NativeStaticFinalizer, NativeStaticFinalizerExecution, NativeStaticFinalizerStartCallback,
        NativeStaticFinalizerStatus, NativeStaticHostEntry, NativeStaticIdentity,
        NativeThreadStaticCleanupRegistration,
    };

    use super::{control, register_thread_static, thread_attachment_identity};

    static CLEANUPS: AtomicUsize = AtomicUsize::new(0);
    static CONTINUING_PRODUCT_CLEANUPS: AtomicUsize = AtomicUsize::new(0);
    static PRODUCT_DESTRUCTIONS: AtomicUsize = AtomicUsize::new(0);
    static PRODUCT_PHASE_ORDER: AtomicUsize = AtomicUsize::new(0);

    thread_local! {
        static THREAD_CLEANUP_ORDER: Cell<usize> = const { Cell::new(0) };
        static PENDING_INCIDENT: RefCell<Option<ProviderIncident>> = const { RefCell::new(None) };
        static INCIDENT_PROVIDER: Cell<Option<&'static NativeProductHostDescriptor>> = const { Cell::new(None) };
        static DEPENDENCY_PRODUCT: Cell<Option<&'static NativeProductHostDescriptor>> = const { Cell::new(None) };
        static DEPENDENCY_ACCESSES: Cell<usize> = const { Cell::new(0) };
        static RELEASED_PROVIDER: Cell<Option<(NativeProductHostState, usize)>> = const { Cell::new(None) };
        static RELEASED_PRODUCT_PHASE: Cell<Option<usize>> = const { Cell::new(None) };
    }

    struct ProviderIncident(&'static NativeProductHostDescriptor);

    impl Drop for ProviderIncident {
        fn drop(&mut self) {
            let provider = control(self.0, NativeProductHostOperation::OBSERVE);

            RELEASED_PROVIDER.set(Some((provider.state(), provider.thread_attachments())));
            RELEASED_PRODUCT_PHASE.set(Some(PRODUCT_PHASE_ORDER.load(Ordering::SeqCst)));
        }
    }

    extern "C" fn release_provider_incident(
        _: usize,
        _: usize,
        _: &mut bray_runtime_abi::NativeRunOutcome,
    ) {
        drop(
            PENDING_INCIDENT
                .take()
                .expect("published incident retains its provider"),
        );
    }

    fn panic_from_provider(
        output: &mut bray_runtime_abi::NativeRunOutcome,
    ) -> NativeStaticFinalizerStatus {
        PENDING_INCIDENT.with_borrow_mut(|slot| {
            assert!(slot.is_none());

            *slot = Some(ProviderIncident(
                INCIDENT_PROVIDER
                    .get()
                    .expect("fixture lookup must contain its requested entry"),
            ));
        });

        *output = bray_runtime_abi::NativeRunOutcome::panicked(crate::frame::native_report(
            bray_runtime_abi::NativePanicPrimary::new(
                bray_runtime_abi::NativePanicCause::MESSAGE,
                bray_runtime_abi::NativeSourceAnchor::unavailable(),
                bray_runtime_abi::NativePanicMessage::new(
                    1,
                    0,
                    None,
                    Some(release_provider_incident),
                ),
            ),
        ));

        NativeStaticFinalizerStatus::SUCCESS
    }

    extern "C" fn access() -> usize {
        1
    }

    extern "C" fn cleanup(
        _: usize,
        _: &mut bray_runtime_abi::NativeRunOutcome,
    ) -> NativeStaticFinalizerStatus {
        CLEANUPS.fetch_add(1, Ordering::SeqCst);

        NativeStaticFinalizerStatus::SUCCESS
    }

    extern "C" fn first_thread_cleanup(
        _: usize,
        _: &mut bray_runtime_abi::NativeRunOutcome,
    ) -> NativeStaticFinalizerStatus {
        append_thread_cleanup(1);

        NativeStaticFinalizerStatus::SUCCESS
    }

    extern "C" fn second_thread_cleanup(
        _: usize,
        _: &mut bray_runtime_abi::NativeRunOutcome,
    ) -> NativeStaticFinalizerStatus {
        append_thread_cleanup(2);

        NativeStaticFinalizerStatus::SUCCESS
    }

    extern "C" fn panicking_thread_cleanup(
        _: usize,
        output: &mut bray_runtime_abi::NativeRunOutcome,
    ) -> NativeStaticFinalizerStatus {
        panic_from_provider(output)
    }

    extern "C" fn panicking_product_cleanup(
        _: usize,
        output: &mut bray_runtime_abi::NativeRunOutcome,
    ) -> NativeStaticFinalizerStatus {
        record_product_phase(1);

        panic_from_provider(output)
    }

    extern "C" fn continuing_product_cleanup(
        _: usize,
        _: &mut bray_runtime_abi::NativeRunOutcome,
    ) -> NativeStaticFinalizerStatus {
        record_product_phase(1);
        CONTINUING_PRODUCT_CLEANUPS.fetch_add(1, Ordering::SeqCst);

        NativeStaticFinalizerStatus::SUCCESS
    }

    extern "C" fn resolve_success(
        _: usize,
        _: usize,
        _: &mut bray_runtime_abi::NativeRunOutcome,
    ) -> NativeStaticFinalizerStatus {
        NativeStaticFinalizerStatus::SUCCESS
    }

    extern "C" fn admit_no_backing(_: u32, _: &mut bray_runtime_abi::NativeRunOutcome) {}

    const fn finalizer(start: NativeStaticFinalizerStartCallback) -> NativeStaticFinalizer {
        NativeStaticFinalizer::new(
            NativeStaticFinalizerExecution::SYNCHRONOUS,
            admit_no_backing,
            start,
            resolve_success,
        )
    }

    extern "C" fn detach_thread_static() {}

    extern "C" fn no_cleanup(_: u32, _: &mut bray_runtime_abi::NativeRunOutcome) {}

    extern "C" fn record_product_destruction(
        abandoned: u32,
        _: &mut bray_runtime_abi::NativeRunOutcome,
    ) {
        let previous = PRODUCT_DESTRUCTIONS.fetch_add(1, Ordering::SeqCst);

        assert_eq!(abandoned, u32::from(previous == 0));
        record_product_phase(2);
    }

    fn record_product_phase(phase: usize) {
        PRODUCT_PHASE_ORDER
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |order| {
                order.checked_mul(10)?.checked_add(phase)
            })
            .unwrap_or_else(|order| panic!("cleanup phase order overflowed from {order}"));
    }

    extern "C" fn no_dependency(_: usize) -> NativeStaticIdentity {
        NativeStaticIdentity::new([0; 32])
    }

    extern "C" fn first_thread_dependency(index: usize) -> NativeStaticIdentity {
        match index {
            0 => NativeStaticIdentity::new([12; 32]),
            _ => NativeStaticIdentity::new([0; 32]),
        }
    }

    static DELAYED_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::PRODUCT,
        NativeStaticIdentity::new([7; 32]),
        0,
        1,
        access,
        detach_thread_static,
        finalizer(cleanup),
        no_cleanup,
        detach_thread_static,
        no_dependency,
        0,
    );

    static PRODUCT_PANICKING_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::PRODUCT,
        NativeStaticIdentity::new([41; 32]),
        0,
        1,
        access,
        detach_thread_static,
        finalizer(panicking_product_cleanup),
        record_product_destruction,
        detach_thread_static,
        no_dependency,
        0,
    );

    static PRODUCT_CONTINUING_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::PRODUCT,
        NativeStaticIdentity::new([42; 32]),
        1,
        2,
        access,
        detach_thread_static,
        finalizer(continuing_product_cleanup),
        record_product_destruction,
        detach_thread_static,
        no_dependency,
        0,
    );

    static THREAD_FIRST_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::EXACT_THREAD,
        NativeStaticIdentity::new([11; 32]),
        0,
        0,
        access,
        detach_thread_static,
        finalizer(first_thread_cleanup),
        no_cleanup,
        detach_thread_static,
        first_thread_dependency,
        1,
    );

    static THREAD_SECOND_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::EXACT_THREAD,
        NativeStaticIdentity::new([12; 32]),
        1,
        0,
        access,
        detach_thread_static,
        finalizer(second_thread_cleanup),
        no_cleanup,
        detach_thread_static,
        no_dependency,
        0,
    );

    static THREAD_PANICKING_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::EXACT_THREAD,
        NativeStaticIdentity::new([21; 32]),
        0,
        0,
        access,
        detach_thread_static,
        finalizer(panicking_thread_cleanup),
        no_cleanup,
        detach_thread_static,
        no_dependency,
        0,
    );

    static THREAD_CONTINUING_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::EXACT_THREAD,
        NativeStaticIdentity::new([22; 32]),
        1,
        0,
        access,
        detach_thread_static,
        finalizer(second_thread_cleanup),
        no_cleanup,
        detach_thread_static,
        no_dependency,
        0,
    );

    extern "C" fn delayed_entry(_: usize) -> NativeStaticHostEntry {
        DELAYED_ENTRY
    }

    extern "C" fn product_incident_entry(index: usize) -> NativeStaticHostEntry {
        match index {
            0 => PRODUCT_PANICKING_ENTRY,
            _ => PRODUCT_CONTINUING_ENTRY,
        }
    }

    extern "C" fn thread_order_entry(index: usize) -> NativeStaticHostEntry {
        match index {
            0 => THREAD_FIRST_ENTRY,
            _ => THREAD_SECOND_ENTRY,
        }
    }

    extern "C" fn dependent_thread_access() -> usize {
        DEPENDENCY_ACCESSES.set(DEPENDENCY_ACCESSES.get() + 1);

        let product = DEPENDENCY_PRODUCT
            .get()
            .expect("dependency accessor must retain its product descriptor");

        let registration = NativeThreadStaticCleanupRegistration::new(
            product,
            NativeStaticIdentity::new([12; 32]),
            detach_thread_static,
            finalizer(second_thread_cleanup),
            no_cleanup,
            detach_thread_static,
        );

        usize::from(register_thread_static(&registration).is_success())
    }

    extern "C" fn dependent_thread_entry(index: usize) -> NativeStaticHostEntry {
        if index == 0 {
            return THREAD_FIRST_ENTRY;
        }

        NativeStaticHostEntry::new(
            NativeStaticDuration::EXACT_THREAD,
            NativeStaticIdentity::new([12; 32]),
            1,
            0,
            dependent_thread_access,
            detach_thread_static,
            finalizer(second_thread_cleanup),
            no_cleanup,
            detach_thread_static,
            no_dependency,
            0,
        )
    }

    extern "C" fn thread_incident_entry(index: usize) -> NativeStaticHostEntry {
        match index {
            0 => THREAD_PANICKING_ENTRY,
            _ => THREAD_CONTINUING_ENTRY,
        }
    }

    fn append_thread_cleanup(value: usize) {
        THREAD_CLEANUP_ORDER.with(|order| {
            let value = order
                .get()
                .checked_mul(10)
                .and_then(|current| current.checked_add(value))
                .unwrap_or_else(|| panic!("test cleanup order must remain representable"));

            order.set(value);
        });
    }

    #[test]
    fn cold_synchronous_product_cleanup_never_retains_a_scheduler() {
        let _isolation = crate::native::test_runtime_isolation();

        extern "C" fn attached_cleanup(
            _: usize,
            _: &mut bray_runtime_abi::NativeRunOutcome,
        ) -> NativeStaticFinalizerStatus {
            assert!(crate::bootstrap::current_runtime_thread().is_some());

            assert!(
                crate::native::retain_current_runtime()
                    .expect("fixture runtime must remain retained")
                    .is_none()
            );

            NativeStaticFinalizerStatus::SUCCESS
        }

        extern "C" fn entry(_: usize) -> NativeStaticHostEntry {
            NativeStaticHostEntry::new(
                NativeStaticDuration::PRODUCT,
                NativeStaticIdentity::new([74; 32]),
                0,
                1,
                access,
                detach_thread_static,
                finalizer(attached_cleanup),
                no_cleanup,
                detach_thread_static,
                no_dependency,
                0,
            )
        }

        let descriptor = Box::leak(Box::new(
            NativeProductHostDescriptor::new(NativeProductIdentity::new([74; 32]), entry, 1)
                .with_execution(
                    crate::native::implementation::bray_runtime_product_execution_provider,
                    false,
                ),
        ));

        assert!(
            crate::native::retain_current_runtime()
                .expect("fixture runtime must remain retained")
                .is_none()
        );

        assert_eq!(
            control(descriptor, NativeProductHostOperation::FORM).status(),
            NativeProductHostStatus::SUCCESS
        );

        let closed = control(descriptor, NativeProductHostOperation::CLOSE);

        assert_eq!(closed.status(), NativeProductHostStatus::CLOSED);
        assert_eq!(closed.cleaned_statics(), 1);
        assert_eq!(closed.cleanup_incidents(), 0);
        assert!(crate::bootstrap::current_runtime_thread().is_none());

        assert!(
            crate::native::retain_current_runtime()
                .expect("fixture runtime must remain retained")
                .is_none()
        );
    }

    #[test]
    fn cleanup_attachment_rejection_preserves_static_owners_for_retry() {
        let _isolation = crate::native::test_runtime_isolation();

        let runtime = crate::native::retain_runtime()
            .expect("fixture `crate::native::retain_runtime` call must succeed");

        thread_local! {
            static RUNS: Cell<usize> = const { Cell::new(0) };
        }

        type Continuation =
            extern "C" fn(*const u8, usize, u32, &mut bray_runtime_abi::NativeRunOutcome) -> u32;

        use crate::native::bridge::{Callbacks, bray_runtime_entry_context_registration};

        extern "C" fn other_provider(
            _: Continuation,
            _: *const u8,
            _: usize,
            _: u32,
            _: &mut bray_runtime_abi::NativeRunOutcome,
        ) -> u32 {
            4
        }

        extern "C" fn cleanup(
            _: usize,
            _: &mut bray_runtime_abi::NativeRunOutcome,
        ) -> NativeStaticFinalizerStatus {
            RUNS.set(RUNS.get() + 1);

            NativeStaticFinalizerStatus::SUCCESS
        }

        extern "C" fn entry(index: usize) -> NativeStaticHostEntry {
            if index == 1 {
                return NativeStaticHostEntry::new(
                    NativeStaticDuration::EXACT_THREAD,
                    NativeStaticIdentity::new([77; 32]),
                    1,
                    0,
                    access,
                    detach_thread_static,
                    finalizer(cleanup),
                    no_cleanup,
                    detach_thread_static,
                    no_dependency,
                    0,
                );
            }

            NativeStaticHostEntry::new(
                NativeStaticDuration::PRODUCT,
                NativeStaticIdentity::new([76; 32]),
                0,
                1,
                access,
                detach_thread_static,
                finalizer(cleanup),
                no_cleanup,
                detach_thread_static,
                no_dependency,
                0,
            )
        }

        let descriptor = Box::leak(Box::new(
            NativeProductHostDescriptor::new(NativeProductIdentity::new([76; 32]), entry, 2)
                .with_execution(
                    crate::native::implementation::bray_runtime_product_execution_provider,
                    false,
                ),
        ));

        crate::native::with_static_cleanup_context(Some(&runtime), || {
            assert_eq!(
                control(descriptor, NativeProductHostOperation::FORM).status(),
                NativeProductHostStatus::SUCCESS
            );
        })
        .expect("fixture static cleanup context must be installed");

        let attachment = crate::bootstrap::RuntimeThreadScope::enter()
            .expect("fixture execution scope must admit entry");

        let mut previous = Callbacks::default();

        let other = Callbacks {
            context: other_provider as *const u8 as usize,
            report: 0,
        };

        assert_eq!(
            bray_runtime_entry_context_registration(&other, &mut previous),
            0
        );

        assert_eq!(previous, Callbacks::default());

        let failed = control(descriptor, NativeProductHostOperation::CLOSE);

        assert_eq!(failed.status(), NativeProductHostStatus::RUNTIME_FAILURE);
        assert_eq!(failed.state(), NativeProductHostState::CLOSING);
        assert_eq!(failed.cleaned_statics(), 0);
        assert_eq!(RUNS.get(), 0);

        assert_eq!(
            bray_runtime_entry_context_registration(&Callbacks::default(), &mut previous),
            0
        );

        assert_eq!(previous, other);

        let closed = control(descriptor, NativeProductHostOperation::CLOSE);

        assert_eq!(closed.status(), NativeProductHostStatus::CLOSED);
        assert_eq!(closed.cleaned_statics(), 1);
        assert_eq!(RUNS.get(), 1);
        assert_eq!(attachment.finish(), 0);
        runtime.release();
    }

    #[test]
    fn thread_static_registration_cannot_change_its_declared_cleanup_execution() {
        let _isolation = crate::native::test_runtime_isolation();

        let descriptor = Box::leak(Box::new(
            NativeProductHostDescriptor::new(
                NativeProductIdentity::new([78; 32]),
                thread_order_entry,
                2,
            )
            .with_execution(
                crate::native::implementation::bray_runtime_product_execution_provider,
                false,
            ),
        ));

        let attachment = crate::bootstrap::RuntimeThreadScope::enter()
            .expect("fixture execution scope must admit entry");

        let registration = NativeThreadStaticCleanupRegistration::new(
            descriptor,
            NativeStaticIdentity::new([11; 32]),
            detach_thread_static,
            NativeStaticFinalizer::new(
                NativeStaticFinalizerExecution::ASYNCHRONOUS,
                admit_no_backing,
                first_thread_cleanup,
                resolve_success,
            ),
            no_cleanup,
            detach_thread_static,
        );

        assert_eq!(
            register_thread_static(&registration),
            bray_runtime_abi::NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(
            control(descriptor, NativeProductHostOperation::OBSERVE).thread_attachments(),
            0
        );

        assert_eq!(
            control(descriptor, NativeProductHostOperation::CLOSE).status(),
            NativeProductHostStatus::CLOSED
        );

        assert_eq!(attachment.finish(), 0);
    }

    #[test]
    fn failed_tls_backing_admission_can_retry_without_publishing_an_owner() {
        let _isolation = crate::native::test_runtime_isolation();

        THREAD_CLEANUP_ORDER.set(0);

        extern "C" fn reject(_: u32, outcome: &mut bray_runtime_abi::NativeRunOutcome) {
            *outcome = bray_runtime_abi::NativeRunOutcome::new(
                bray_runtime_abi::NativeRunState::RUNTIME_FAILURE,
                0,
            );
        }

        let descriptor = Box::leak(Box::new(NativeProductHostDescriptor::new(
            NativeProductIdentity::new([96; 32]),
            thread_order_entry,
            2,
        )));

        let scope = crate::bootstrap::RuntimeThreadScope::enter().expect("attachment enters");

        let failed = NativeThreadStaticCleanupRegistration::new(
            descriptor,
            NativeStaticIdentity::new([11; 32]),
            detach_thread_static,
            NativeStaticFinalizer::new(
                NativeStaticFinalizerExecution::SYNCHRONOUS,
                reject,
                first_thread_cleanup,
                resolve_success,
            ),
            no_cleanup,
            detach_thread_static,
        );

        assert_eq!(
            register_thread_static(&failed),
            bray_runtime_abi::NativeRuntimeStatus::OUTGOING_STORAGE_UNAVAILABLE
        );

        assert_eq!(THREAD_CLEANUP_ORDER.get(), 0);

        let accepted = NativeThreadStaticCleanupRegistration::new(
            descriptor,
            NativeStaticIdentity::new([11; 32]),
            detach_thread_static,
            finalizer(first_thread_cleanup),
            no_cleanup,
            detach_thread_static,
        );

        assert!(register_thread_static(&accepted).is_success());

        assert_eq!(
            control(descriptor, NativeProductHostOperation::FINISH_ROOT).state(),
            NativeProductHostState::CLOSED
        );

        assert_eq!(THREAD_CLEANUP_ORDER.get(), 1);
        assert_eq!(scope.finish(), 0);
    }

    #[test]
    fn thread_static_registration_admits_dependency_accessors_before_the_consumer() {
        let _isolation = crate::native::test_runtime_isolation();

        THREAD_CLEANUP_ORDER.set(0);
        DEPENDENCY_ACCESSES.set(0);

        let descriptor = Box::leak(Box::new(NativeProductHostDescriptor::new(
            NativeProductIdentity::new([97; 32]),
            dependent_thread_entry,
            2,
        )));

        DEPENDENCY_PRODUCT.set(Some(descriptor));

        let scope = crate::bootstrap::RuntimeThreadScope::enter().expect("attachment enters");

        let consumer = NativeThreadStaticCleanupRegistration::new(
            descriptor,
            NativeStaticIdentity::new([11; 32]),
            detach_thread_static,
            finalizer(first_thread_cleanup),
            no_cleanup,
            detach_thread_static,
        );

        assert!(register_thread_static(&consumer).is_success());
        assert_eq!(DEPENDENCY_ACCESSES.get(), 1);

        assert_eq!(
            control(descriptor, NativeProductHostOperation::FINISH_ROOT).state(),
            NativeProductHostState::CLOSED
        );

        assert_eq!(THREAD_CLEANUP_ORDER.get(), 12);
        assert_eq!(scope.finish(), 0);

        DEPENDENCY_PRODUCT.set(None);
    }

    #[test]
    fn thread_static_product_preserves_an_existing_cleanup_runtime() {
        let _isolation = crate::native::test_runtime_isolation();

        let runtime = crate::native::retain_runtime()
            .expect("fixture `crate::native::retain_runtime` call must succeed");

        let descriptor = Box::leak(Box::new(
            NativeProductHostDescriptor::new(
                NativeProductIdentity::new([75; 32]),
                thread_order_entry,
                2,
            )
            .with_execution(
                crate::native::implementation::bray_runtime_product_execution_provider,
                false,
            ),
        ));

        let ((), incident) = crate::native::with_static_cleanup_context(Some(&runtime), || {
            assert_eq!(
                control(descriptor, NativeProductHostOperation::FORM).status(),
                NativeProductHostStatus::SUCCESS
            );

            extern "C" fn retained_cleanup(
                _: usize,
                _: &mut bray_runtime_abi::NativeRunOutcome,
            ) -> NativeStaticFinalizerStatus {
                crate::native::retain_current_runtime()
                    .expect("cleanup retains its existing runtime")
                    .expect("cleanup has its selected runtime")
                    .release();

                NativeStaticFinalizerStatus::SUCCESS
            }

            let registration = NativeThreadStaticCleanupRegistration::new(
                descriptor,
                NativeStaticIdentity::new([11; 32]),
                detach_thread_static,
                finalizer(retained_cleanup),
                no_cleanup,
                detach_thread_static,
            );

            assert!(register_thread_static(&registration).is_success());

            assert_eq!(
                control(descriptor, NativeProductHostOperation::FINISH_ROOT).status(),
                NativeProductHostStatus::CLOSED
            );
        })
        .expect("fixture static cleanup context must be installed");

        assert!(incident.is_none());
        runtime.release();
    }

    #[test]
    fn closed_product_descriptor_address_can_be_reused_without_reviving_its_state() {
        let mut descriptor = NativeProductHostDescriptor::new(
            NativeProductIdentity::new([94; 32]),
            delayed_entry,
            0,
        );

        let address = std::ptr::from_ref(&descriptor);

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::FORM).status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::CLOSE).state(),
            NativeProductHostState::CLOSED
        );

        descriptor = NativeProductHostDescriptor::new(
            NativeProductIdentity::new([95; 32]),
            delayed_entry,
            0,
        );

        assert_eq!(address, std::ptr::from_ref(&descriptor));

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::FORM).status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::ACQUIRE_ENTRY).active_entries(),
            1
        );

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::CLOSE).state(),
            NativeProductHostState::CLOSING
        );

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::RELEASE_ENTRY).state(),
            NativeProductHostState::CLOSED
        );
    }

    #[test]
    fn product_host_delays_cleanup_until_every_obligation_is_released() {
        CLEANUPS.store(0, Ordering::SeqCst);

        let descriptor =
            NativeProductHostDescriptor::new(NativeProductIdentity::new([9; 32]), delayed_entry, 1);

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::ACQUIRE_ENTRY).status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::ACQUIRE_EXTERNAL).status(),
            NativeProductHostStatus::SUCCESS
        );

        let closing = control(&descriptor, NativeProductHostOperation::CLOSE);

        assert_eq!(closing.state(), NativeProductHostState::CLOSING);
        assert_eq!(closing.status(), NativeProductHostStatus::PENDING);
        assert_eq!(CLEANUPS.load(Ordering::SeqCst), 0);

        let pending = control(&descriptor, NativeProductHostOperation::RELEASE_ENTRY);

        assert_eq!(pending.state(), NativeProductHostState::CLOSING);
        assert_eq!(CLEANUPS.load(Ordering::SeqCst), 0);

        let closed = control(&descriptor, NativeProductHostOperation::RELEASE_EXTERNAL);

        assert_eq!(closed.state(), NativeProductHostState::CLOSED);
        assert_eq!(closed.status(), NativeProductHostStatus::CLOSED);
        assert_eq!(closed.cleaned_statics(), 1);
        assert_eq!(CLEANUPS.load(Ordering::SeqCst), 1);

        let observed = control(&descriptor, NativeProductHostOperation::OBSERVE);

        assert_eq!(observed.state(), NativeProductHostState::CLOSED);
        assert_eq!(CLEANUPS.load(Ordering::SeqCst), 1);

        let formed_again = control(&descriptor, NativeProductHostOperation::FORM);

        assert_eq!(formed_again.state(), NativeProductHostState::CLOSED);
        assert_eq!(formed_again.status(), NativeProductHostStatus::CLOSED);
        assert_eq!(CLEANUPS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn product_host_serializes_entry_acquisition_with_closure() {
        let descriptor: &'static NativeProductHostDescriptor =
            Box::leak(Box::new(NativeProductHostDescriptor::new(
                NativeProductIdentity::new([31; 32]),
                delayed_entry,
                0,
            )));

        let barrier = Arc::new(Barrier::new(9));

        assert_eq!(
            control(descriptor, NativeProductHostOperation::ACQUIRE_EXTERNAL).status(),
            NativeProductHostStatus::SUCCESS
        );

        let workers = (0..8)
            .map(|_| {
                let barrier = Arc::clone(&barrier);

                std::thread::spawn(move || {
                    barrier.wait();

                    for _ in 0..100 {
                        let acquired =
                            control(descriptor, NativeProductHostOperation::ACQUIRE_ENTRY);

                        match acquired.status() {
                            NativeProductHostStatus::SUCCESS => {
                                assert!(matches!(
                                    control(descriptor, NativeProductHostOperation::RELEASE_ENTRY,)
                                        .status(),
                                    NativeProductHostStatus::SUCCESS
                                        | NativeProductHostStatus::PENDING
                                ));
                            }
                            NativeProductHostStatus::CLOSED => {}
                            status => panic!("entry race returned unexpected status {status:?}"),
                        }
                    }
                })
            })
            .collect::<Vec<_>>();

        barrier.wait();

        let closing = control(descriptor, NativeProductHostOperation::CLOSE);

        assert_eq!(closing.status(), NativeProductHostStatus::PENDING);

        for worker in workers {
            worker
                .join()
                .unwrap_or_else(|error| panic!("entry worker must finish: {error:?}"));
        }

        let closed = control(descriptor, NativeProductHostOperation::RELEASE_EXTERNAL);

        assert_eq!(closed.state(), NativeProductHostState::CLOSED);
        assert_eq!(closed.active_entries(), 0);
    }

    #[test]
    fn worker_and_foreign_attachment_releases_preserve_the_other_kind() {
        let _isolation = crate::native::test_runtime_isolation();

        let descriptor = NativeProductHostDescriptor::new(
            NativeProductIdentity::new([75; 32]),
            delayed_entry,
            0,
        )
        .with_execution(
            crate::native::implementation::bray_runtime_product_execution_provider,
            false,
        );

        for (operation, status, attachments) in [
            (
                NativeProductHostOperation::ACQUIRE_WORKER_ATTACHMENT,
                NativeProductHostStatus::SUCCESS,
                1,
            ),
            (
                NativeProductHostOperation::RELEASE_ATTACHMENT,
                NativeProductHostStatus::INVALID_ARGUMENT,
                1,
            ),
            (
                NativeProductHostOperation::ACQUIRE_ATTACHMENT,
                NativeProductHostStatus::SUCCESS,
                2,
            ),
            (
                NativeProductHostOperation::RELEASE_WORKER_ATTACHMENT,
                NativeProductHostStatus::SUCCESS,
                1,
            ),
            (
                NativeProductHostOperation::RELEASE_WORKER_ATTACHMENT,
                NativeProductHostStatus::INVALID_ARGUMENT,
                1,
            ),
            (
                NativeProductHostOperation::RELEASE_ATTACHMENT,
                NativeProductHostStatus::SUCCESS,
                0,
            ),
        ] {
            let observed = control(&descriptor, operation);

            assert_eq!(observed.status(), status);
            assert_eq!(observed.thread_attachments(), attachments);
        }

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::CLOSE).status(),
            NativeProductHostStatus::CLOSED
        );
    }

    #[test]
    fn product_cleanup_reports_incidents_and_continues() {
        CONTINUING_PRODUCT_CLEANUPS.store(0, Ordering::SeqCst);
        PRODUCT_DESTRUCTIONS.store(0, Ordering::SeqCst);
        PRODUCT_PHASE_ORDER.store(0, Ordering::SeqCst);

        let panicking_identity = NativeStaticIdentity::new([41; 32]);

        let descriptor = Box::leak(Box::new(NativeProductHostDescriptor::new(
            NativeProductIdentity::new([43; 32]),
            product_incident_entry,
            2,
        )));

        INCIDENT_PROVIDER.set(Some(descriptor));

        let closed = control(descriptor, NativeProductHostOperation::CLOSE);

        assert_eq!(
            RELEASED_PROVIDER.take(),
            Some((NativeProductHostState::CLOSING, 0))
        );

        INCIDENT_PROVIDER.set(None);

        assert_eq!(closed.state(), NativeProductHostState::CLOSED);
        assert_eq!(closed.status(), NativeProductHostStatus::INCIDENTS);
        assert_eq!(closed.cleanup_incidents(), 1);
        assert_eq!(closed.last_incident(), panicking_identity);
        assert_eq!(closed.cleaned_statics(), 2);
        assert_eq!(CONTINUING_PRODUCT_CLEANUPS.load(Ordering::SeqCst), 1);
        assert_eq!(PRODUCT_DESTRUCTIONS.load(Ordering::SeqCst), 2);
        assert_eq!(PRODUCT_PHASE_ORDER.load(Ordering::SeqCst), 1212);
        assert_eq!(RELEASED_PRODUCT_PHASE.take(), Some(1));
    }

    #[test]
    fn thread_static_cleanup_follows_host_order_for_each_exact_attachment() {
        THREAD_CLEANUP_ORDER.set(0);

        let first_identity = NativeStaticIdentity::new([11; 32]);
        let second_identity = NativeStaticIdentity::new([12; 32]);

        let descriptor = Box::leak(Box::new(
            NativeProductHostDescriptor::new(
                NativeProductIdentity::new([13; 32]),
                thread_order_entry,
                2,
            )
            .with_execution(
                crate::native::implementation::bray_runtime_product_execution_provider,
                false,
            ),
        ));

        let first_registration = NativeThreadStaticCleanupRegistration::new(
            descriptor,
            first_identity,
            detach_thread_static,
            finalizer(first_thread_cleanup),
            no_cleanup,
            detach_thread_static,
        );

        let second_registration = NativeThreadStaticCleanupRegistration::new(
            descriptor,
            second_identity,
            detach_thread_static,
            finalizer(second_thread_cleanup),
            no_cleanup,
            detach_thread_static,
        );

        let scope = crate::bootstrap::RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("test thread must attach: {error:?}"));

        assert!(register_thread_static(&second_registration).is_success());
        assert!(register_thread_static(&first_registration).is_success());
        assert!(register_thread_static(&first_registration).is_success());

        drop(scope);

        assert_eq!(THREAD_CLEANUP_ORDER.get(), 12);

        let observed = control(descriptor, NativeProductHostOperation::OBSERVE);

        assert_eq!(observed.state(), NativeProductHostState::OPEN);
        assert_eq!(observed.thread_attachments(), 0);

        let scope = crate::bootstrap::RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("test thread must reattach: {error:?}"));

        assert!(register_thread_static(&first_registration).is_success());

        drop(scope);

        assert_eq!(THREAD_CLEANUP_ORDER.get(), 121);

        assert_eq!(
            control(descriptor, NativeProductHostOperation::CLOSE).state(),
            NativeProductHostState::CLOSED
        );
    }

    #[test]
    fn thread_static_cleanup_reports_incidents_and_continues() {
        THREAD_CLEANUP_ORDER.set(0);

        let panicking_identity = NativeStaticIdentity::new([21; 32]);
        let continuing_identity = NativeStaticIdentity::new([22; 32]);

        let descriptor = Box::leak(Box::new(
            NativeProductHostDescriptor::new(
                NativeProductIdentity::new([23; 32]),
                thread_incident_entry,
                2,
            )
            .with_execution(
                crate::native::implementation::bray_runtime_product_execution_provider,
                false,
            ),
        ));

        let panicking_registration = NativeThreadStaticCleanupRegistration::new(
            descriptor,
            panicking_identity,
            detach_thread_static,
            finalizer(panicking_thread_cleanup),
            no_cleanup,
            detach_thread_static,
        );

        let continuing_registration = NativeThreadStaticCleanupRegistration::new(
            descriptor,
            continuing_identity,
            detach_thread_static,
            finalizer(second_thread_cleanup),
            no_cleanup,
            detach_thread_static,
        );

        INCIDENT_PROVIDER.set(Some(descriptor));

        let scope = crate::bootstrap::RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("test thread must attach: {error:?}"));

        assert!(register_thread_static(&panicking_registration).is_success());
        assert!(register_thread_static(&continuing_registration).is_success());

        drop(scope);

        let observed = control(descriptor, NativeProductHostOperation::OBSERVE);

        assert_eq!(
            RELEASED_PROVIDER.take(),
            Some((NativeProductHostState::OPEN, 1))
        );

        INCIDENT_PROVIDER.set(None);

        assert_eq!(THREAD_CLEANUP_ORDER.get(), 2);
        assert_eq!(observed.cleanup_incidents(), 1);
        assert_eq!(observed.last_incident(), panicking_identity);
    }

    #[test]
    fn foreign_thread_attachments_are_product_scoped() {
        THREAD_CLEANUP_ORDER.set(0);

        let first_identity = NativeStaticIdentity::new([11; 32]);
        let second_identity = NativeStaticIdentity::new([12; 32]);

        let first_product = Box::leak(Box::new(
            NativeProductHostDescriptor::new(
                NativeProductIdentity::new([51; 32]),
                thread_order_entry,
                2,
            )
            .with_execution(
                crate::native::implementation::bray_runtime_product_execution_provider,
                false,
            ),
        ));

        let second_product = Box::leak(Box::new(
            NativeProductHostDescriptor::new(
                NativeProductIdentity::new([52; 32]),
                thread_order_entry,
                2,
            )
            .with_execution(
                crate::native::implementation::bray_runtime_product_execution_provider,
                false,
            ),
        ));

        assert_eq!(
            control(
                first_product,
                NativeProductHostOperation::ATTACH_CURRENT_THREAD,
            )
            .status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(
            control(
                second_product,
                NativeProductHostOperation::ATTACH_CURRENT_THREAD,
            )
            .status(),
            NativeProductHostStatus::SUCCESS
        );

        let first_attachment = thread_attachment_identity(first_product);
        let second_attachment = thread_attachment_identity(second_product);

        assert_ne!(first_attachment, 0);
        assert_ne!(second_attachment, 0);
        assert_ne!(first_attachment, second_attachment);

        let first_registration = NativeThreadStaticCleanupRegistration::new(
            first_product,
            first_identity,
            detach_thread_static,
            finalizer(first_thread_cleanup),
            no_cleanup,
            detach_thread_static,
        );

        let second_registration = NativeThreadStaticCleanupRegistration::new(
            second_product,
            second_identity,
            detach_thread_static,
            finalizer(second_thread_cleanup),
            no_cleanup,
            detach_thread_static,
        );

        assert!(register_thread_static(&first_registration).is_success());
        assert!(register_thread_static(&second_registration).is_success());

        assert_eq!(
            control(
                first_product,
                NativeProductHostOperation::DETACH_CURRENT_THREAD,
            )
            .status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(THREAD_CLEANUP_ORDER.get(), 1);

        assert_eq!(
            thread_attachment_identity(second_product),
            second_attachment
        );

        assert_eq!(
            control(
                second_product,
                NativeProductHostOperation::DETACH_CURRENT_THREAD,
            )
            .status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(THREAD_CLEANUP_ORDER.get(), 12);

        assert_eq!(
            control(
                first_product,
                NativeProductHostOperation::ATTACH_CURRENT_THREAD,
            )
            .status(),
            NativeProductHostStatus::SUCCESS
        );

        let reattached = thread_attachment_identity(first_product);

        assert_ne!(reattached, first_attachment);

        assert_eq!(
            control(
                first_product,
                NativeProductHostOperation::DETACH_CURRENT_THREAD,
            )
            .status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(
            control(first_product, NativeProductHostOperation::CLOSE).state(),
            NativeProductHostState::CLOSED
        );

        assert_eq!(
            control(second_product, NativeProductHostOperation::CLOSE).state(),
            NativeProductHostState::CLOSED
        );
    }

    #[test]
    fn nested_foreign_entry_retains_one_epoch_until_outer_cleanup() {
        let _isolation = crate::native::test_runtime_isolation();

        std::thread::spawn(|| {
            THREAD_CLEANUP_ORDER.set(0);

            let descriptor = Box::leak(Box::new(NativeProductHostDescriptor::new(
                NativeProductIdentity::new([93; 32]),
                thread_order_entry,
                2,
            )));

            assert_eq!(
                control(
                    descriptor,
                    NativeProductHostOperation::ATTACH_CURRENT_THREAD
                )
                .status(),
                NativeProductHostStatus::SUCCESS
            );

            let epoch = thread_attachment_identity(descriptor);

            assert_ne!(epoch, 0);

            assert_eq!(
                control(
                    descriptor,
                    NativeProductHostOperation::ATTACH_CURRENT_THREAD
                )
                .thread_attachments(),
                1
            );

            assert_eq!(thread_attachment_identity(descriptor), epoch);

            let registration = NativeThreadStaticCleanupRegistration::new(
                descriptor,
                NativeStaticIdentity::new([11; 32]),
                detach_thread_static,
                finalizer(first_thread_cleanup),
                no_cleanup,
                detach_thread_static,
            );

            assert!(register_thread_static(&registration).is_success());

            assert_eq!(
                control(
                    descriptor,
                    NativeProductHostOperation::DETACH_CURRENT_THREAD
                )
                .thread_attachments(),
                1
            );

            assert_eq!(THREAD_CLEANUP_ORDER.get(), 0);
            assert_eq!(thread_attachment_identity(descriptor), epoch);

            assert_eq!(
                control(descriptor, NativeProductHostOperation::CLOSE).state(),
                NativeProductHostState::CLOSING
            );

            assert_ne!(
                control(
                    descriptor,
                    NativeProductHostOperation::ATTACH_CURRENT_THREAD
                )
                .status(),
                NativeProductHostStatus::SUCCESS
            );

            assert_eq!(
                control(
                    descriptor,
                    NativeProductHostOperation::DETACH_CURRENT_THREAD
                )
                .thread_attachments(),
                0
            );

            assert_eq!(THREAD_CLEANUP_ORDER.get(), 1);
            assert_eq!(thread_attachment_identity(descriptor), 0);
            assert!(crate::bootstrap::current_runtime_thread().is_none());
        })
        .join()
        .expect("nested attachment cleanup completes on its original thread");
    }

    #[test]
    fn cold_foreign_thread_exit_preserves_cleanup_context_dependencies() {
        let _isolation = crate::native::test_runtime_isolation();

        static OBSERVED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

        extern "C" fn observe_context(
            _: usize,
            _: &mut bray_runtime_abi::NativeRunOutcome,
        ) -> NativeStaticFinalizerStatus {
            assert!(crate::bootstrap::current_runtime_thread().is_some());
            assert!(crate::context::current_task_execution_context().is_none());
            assert!(!crate::context::current_run_cancellation_observable());
            assert!(!crate::context::current_run_cancellation_requested());

            assert!(
                crate::native::retain_current_runtime()
                    .expect("fixture runtime must remain retained")
                    .is_none()
            );

            #[cfg(feature = "test-output")]
            assert!(crate::context::current_task_output().is_none());

            OBSERVED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

            NativeStaticFinalizerStatus::SUCCESS
        }

        let descriptor: &'static NativeProductHostDescriptor = Box::leak(Box::new(
            NativeProductHostDescriptor::new(
                NativeProductIdentity::new([53; 32]),
                thread_order_entry,
                2,
            )
            .with_execution(
                crate::native::implementation::bray_runtime_product_execution_provider,
                false,
            ),
        ));

        for observe_first in [false, true] {
            std::thread::spawn(move || {
                if observe_first {
                    assert_eq!(
                        control(descriptor, NativeProductHostOperation::OBSERVE)
                            .thread_attachments(),
                        0
                    );
                }

                assert_eq!(
                    control(
                        descriptor,
                        NativeProductHostOperation::ATTACH_CURRENT_THREAD
                    )
                    .status(),
                    NativeProductHostStatus::SUCCESS
                );

                assert!(
                    crate::native::retain_current_runtime()
                        .expect("fixture worker must start")
                        .is_none()
                );

                let registration = NativeThreadStaticCleanupRegistration::new(
                    descriptor,
                    NativeStaticIdentity::new([11; 32]),
                    detach_thread_static,
                    finalizer(observe_context),
                    no_cleanup,
                    detach_thread_static,
                );

                assert!(register_thread_static(&registration).is_success());
            })
            .join()
            .expect(
                "foreign thread exit retains every cleanup context until attachment retirement",
            );

            let observed = control(descriptor, NativeProductHostOperation::OBSERVE);

            assert_eq!(observed.thread_attachments(), 0);
        }

        assert_eq!(OBSERVED.load(std::sync::atomic::Ordering::SeqCst), 2);

        assert_eq!(
            control(descriptor, NativeProductHostOperation::CLOSE).state(),
            NativeProductHostState::CLOSED
        );
    }

    #[test]
    fn closing_product_completes_through_outer_attachment_retirement() {
        let _isolation = crate::native::test_runtime_isolation();

        static RUNS: AtomicUsize = AtomicUsize::new(0);

        extern "C" fn cleanup(
            _: usize,
            _: &mut bray_runtime_abi::NativeRunOutcome,
        ) -> NativeStaticFinalizerStatus {
            assert!(crate::bootstrap::current_runtime_thread().is_some());
            assert_eq!(THREAD_CLEANUP_ORDER.get(), 0);
            THREAD_CLEANUP_ORDER.set(1);
            RUNS.fetch_add(1, Ordering::SeqCst);

            NativeStaticFinalizerStatus::SUCCESS
        }

        for native_exit in [false, true] {
            let descriptor: &'static NativeProductHostDescriptor = Box::leak(Box::new(
                NativeProductHostDescriptor::new(
                    NativeProductIdentity::new([170 + u8::from(native_exit); 32]),
                    thread_order_entry,
                    2,
                )
                .with_execution(
                    crate::native::implementation::bray_runtime_product_execution_provider,
                    false,
                ),
            ));

            std::thread::spawn(move || {
                THREAD_CLEANUP_ORDER.set(0);

                let scope = if native_exit {
                    assert_eq!(
                        control(
                            descriptor,
                            NativeProductHostOperation::ATTACH_CURRENT_THREAD
                        )
                        .status(),
                        NativeProductHostStatus::SUCCESS,
                    );

                    None
                } else {
                    Some(
                        crate::bootstrap::RuntimeThreadScope::enter()
                            .expect("physical attachment enters"),
                    )
                };

                let registration = NativeThreadStaticCleanupRegistration::new(
                    descriptor,
                    NativeStaticIdentity::new([11; 32]),
                    detach_thread_static,
                    finalizer(cleanup),
                    no_cleanup,
                    detach_thread_static,
                );

                assert!(register_thread_static(&registration).is_success());

                assert_eq!(
                    control(descriptor, NativeProductHostOperation::CLOSE).status(),
                    NativeProductHostStatus::PENDING,
                );

                if let Some(scope) = scope {
                    assert_eq!(scope.finish(), 0);
                    assert_eq!(THREAD_CLEANUP_ORDER.get(), 1);
                }
            })
            .join()
            .expect("pending product cleanup completes on its retiring thread");

            let closed = control(descriptor, NativeProductHostOperation::OBSERVE);

            assert_eq!(closed.status(), NativeProductHostStatus::CLOSED);
            assert_eq!(closed.thread_attachments(), 0);
        }

        assert_eq!(RUNS.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn finishing_root_drains_implicit_thread_statics_once() {
        THREAD_CLEANUP_ORDER.set(0);

        let descriptor = Box::leak(Box::new(
            NativeProductHostDescriptor::new(
                NativeProductIdentity::new([71; 32]),
                thread_order_entry,
                2,
            )
            .with_execution(
                crate::native::implementation::bray_runtime_product_execution_provider,
                false,
            ),
        ));

        let scope = crate::bootstrap::RuntimeThreadScope::enter().expect("runtime thread enters");

        let registration = NativeThreadStaticCleanupRegistration::new(
            descriptor,
            NativeStaticIdentity::new([11; 32]),
            detach_thread_static,
            finalizer(first_thread_cleanup),
            no_cleanup,
            detach_thread_static,
        );

        assert!(register_thread_static(&registration).is_success());

        assert_eq!(
            control(descriptor, NativeProductHostOperation::OBSERVE).thread_attachments(),
            1
        );

        let closed = control(descriptor, NativeProductHostOperation::FINISH_ROOT);

        assert_eq!(closed.state(), NativeProductHostState::CLOSED);
        assert_eq!(closed.status(), NativeProductHostStatus::CLOSED);
        assert_eq!(closed.thread_attachments(), 0);
        assert_eq!(THREAD_CLEANUP_ORDER.get(), 1);

        assert_eq!(
            control(descriptor, NativeProductHostOperation::FINISH_ROOT).state(),
            NativeProductHostState::CLOSED
        );

        drop(scope);
        assert_eq!(THREAD_CLEANUP_ORDER.get(), 1);
    }

    #[test]
    fn finishing_root_preserves_foreign_and_external_obligations() {
        let descriptor = NativeProductHostDescriptor::new(
            NativeProductIdentity::new([72; 32]),
            delayed_entry,
            0,
        )
        .with_execution(
            crate::native::implementation::bray_runtime_product_execution_provider,
            false,
        );

        assert_eq!(
            control(
                &descriptor,
                NativeProductHostOperation::ATTACH_CURRENT_THREAD
            )
            .status(),
            NativeProductHostStatus::SUCCESS
        );

        let rejected = control(&descriptor, NativeProductHostOperation::FINISH_ROOT);

        assert_eq!(rejected.status(), NativeProductHostStatus::INVALID_ARGUMENT);
        assert_eq!(rejected.state(), NativeProductHostState::OPEN);
        assert_eq!(rejected.thread_attachments(), 1);

        assert_eq!(
            control(
                &descriptor,
                NativeProductHostOperation::DETACH_CURRENT_THREAD
            )
            .status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::ACQUIRE_EXTERNAL).status(),
            NativeProductHostStatus::SUCCESS
        );

        let pending = control(&descriptor, NativeProductHostOperation::FINISH_ROOT);

        assert_eq!(pending.status(), NativeProductHostStatus::PENDING);
        assert_eq!(pending.state(), NativeProductHostState::CLOSING);

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::RELEASE_EXTERNAL).state(),
            NativeProductHostState::CLOSED
        );
    }

    #[test]
    fn foreign_attachment_retains_product_before_thread_static_access() {
        let descriptor = NativeProductHostDescriptor::new(
            NativeProductIdentity::new([61; 32]),
            delayed_entry,
            0,
        )
        .with_execution(
            crate::native::implementation::bray_runtime_product_execution_provider,
            false,
        );

        let attached = control(
            &descriptor,
            NativeProductHostOperation::ATTACH_CURRENT_THREAD,
        );

        assert_eq!(attached.status(), NativeProductHostStatus::SUCCESS);
        assert_eq!(attached.thread_attachments(), 1);

        let closing = control(&descriptor, NativeProductHostOperation::CLOSE);

        assert_eq!(closing.state(), NativeProductHostState::CLOSING);
        assert_eq!(closing.status(), NativeProductHostStatus::PENDING);

        let detached = control(
            &descriptor,
            NativeProductHostOperation::DETACH_CURRENT_THREAD,
        );

        assert_eq!(detached.state(), NativeProductHostState::CLOSED);
        assert_eq!(detached.thread_attachments(), 0);
    }
}
