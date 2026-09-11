use bray_runtime_abi::{
    NativeRuntimeStatus, NativeSourceAnchor, NativeTypeIdentity,
    NativeValueCleanup,
};

native_export! {
    pub extern "C" fn bray_runtime_entry_result_admission(
        product: Option<&bray_runtime_abi::NativeProductHostDescriptor>,
        size: usize,
        alignment: usize,
        error_offset: usize,
        broadcast: Option<&NativeValueCleanup>,
        lifecycle: Option<&NativeValueCleanup>,
        destination: Option<&mut usize>,
    ) -> bray_runtime_abi::NativeTaskAllocation {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (Some(product), Some(lifecycle), Some(destination)) = (product, lifecycle, destination) else {
                return bray_runtime_abi::NativeTaskAllocation::failure(NativeRuntimeStatus::INVALID_ARGUMENT);
            };
            match super::state::with_runtime(|runtime| runtime.admit_returned_value(
                crate::product::product_key(product), size, alignment, error_offset, broadcast.copied(), *lifecycle,
            )).and_then(|result| result) {
                Ok((task, address)) => {
                    *destination = address;
                    bray_runtime_abi::NativeTaskAllocation::success(task)
                }
                Err(status) => bray_runtime_abi::NativeTaskAllocation::failure(status),
            }
        })).unwrap_or_else(|_| bray_runtime_abi::NativeTaskAllocation::failure(NativeRuntimeStatus::PANICKED))
    }
}

native_export! {
    pub extern "C" fn bray_runtime_entry_result_resolution(task: u64, returned_error: u8) -> NativeRuntimeStatus {
        super::export::contain_status(|| {
            if returned_error > 1 {
                return NativeRuntimeStatus::INVALID_ARGUMENT;
            }
            let Some(task) = bray_runtime_abi::NativeTaskHandle::new(task) else {
                return NativeRuntimeStatus::INVALID_ARGUMENT;
            };
            super::state::with_runtime(|runtime| {
                if returned_error == 1 { runtime.resolve_returned_value(task) }
                else { runtime.release_returned_value(task) }
            }).unwrap_or_else(|status| status)
        })
    }
}

pub(super) fn resolve_failure(
    identity: NativeTypeIdentity,
    source: NativeSourceAnchor,
    value: usize,
    broadcast: Option<&NativeValueCleanup>,
    lifecycle: Option<&NativeValueCleanup>,
) -> NativeRuntimeStatus {
    if !source.is_valid()
        || [broadcast, lifecycle].into_iter().flatten()
            .any(|cleanup| !cleanup.execution().completes_synchronously())
        || (value == 0 && (broadcast.is_some() || lifecycle.is_some()))
    {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    }

    let reported = if super::host::active() {
        super::host::record_returned_error();

        NativeRuntimeStatus::SUCCESS
    } else {
        super::incident::report_error_details("entry_error", identity, source)
    };

    if broadcast.is_none() && lifecycle.is_none() {
        return reported;
    }

    super::state::with_runtime(|_| {
        let ((), incidents) = super::incident::with_cleanup_incident_owner(|record| {
            for cleanup in [broadcast, lifecycle].into_iter().flatten() {
                for incident in super::value_cleanup::run(*cleanup, value) {
                    record(incident);
                }
            }
        });

        super::incident::retain_cleanup_incidents(incidents);

        reported
    })
    .unwrap_or_else(|status| status)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_abi::{
        NativeBrayCallOutcome, NativeCleanupExecution, NativeInactiveFrame, NativeRuntimeStatus,
        NativeSourceAnchor, NativeTypeIdentity, NativeValueCleanup,
    };

    #[test]
    fn entry_result_boundary_rejects_invalid_inputs_before_runtime_access() {
        let mut destination = 71;

        let allocation = super::bray_runtime_entry_result_admission(
            None,
            8,
            8,
            0,
            None,
            None,
            Some(&mut destination),
        );

        assert_eq!(allocation.status(), NativeRuntimeStatus::INVALID_ARGUMENT);
        assert_eq!(destination, 71);

        assert_eq!(
            super::bray_runtime_entry_result_resolution(1, 2),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );
    }

    #[test]
    fn entry_cleanup_continues_after_broadcast_failure_and_retains_the_panic() {
        static TRACE: AtomicUsize = AtomicUsize::new(0);
        static RELEASED: AtomicUsize = AtomicUsize::new(0);

        extern "C-unwind" fn broadcast(
            value: usize,
            _: &mut NativeInactiveFrame,
            outcome: &mut NativeBrayCallOutcome,
        ) -> NativeRuntimeStatus {
            assert_eq!(value, 7);
            assert_eq!(TRACE.fetch_add(1, Ordering::Relaxed), 0);

            let incident = crate::incident::OwnedCleanupIncident::boundary(
                NativeBrayCallOutcome::panicked(4).unwrap(),
                crate::test_support::panic_callbacks(release, release),
            )
            .unwrap();

            assert!(super::super::incident::retain_cleanup_incident(incident).is_ok());
            *outcome = NativeBrayCallOutcome::panicked(8).unwrap();

            NativeRuntimeStatus::SUCCESS
        }

        extern "C-unwind" fn lifecycle(
            value: usize,
            _: &mut NativeInactiveFrame,
            _: &mut NativeBrayCallOutcome,
        ) -> NativeRuntimeStatus {
            assert_eq!(value, 7);
            assert_eq!(TRACE.fetch_add(1, Ordering::Relaxed), 1);

            NativeRuntimeStatus::SUCCESS
        }

        extern "C-unwind" fn release(value: usize) -> NativeRuntimeStatus {
            RELEASED.store(
                RELEASED.load(Ordering::Relaxed) * 10 + value,
                Ordering::Relaxed,
            );

            NativeRuntimeStatus::SUCCESS
        }

        let panics = crate::test_support::panic_callbacks(release, release);

        let broadcast =
            NativeValueCleanup::new(NativeCleanupExecution::SYNCHRONOUS, None, broadcast, panics);

        let lifecycle =
            NativeValueCleanup::new(NativeCleanupExecution::SYNCHRONOUS, None, lifecycle, panics);

        let ((status, incidents), runtime_incidents) =
            crate::native::with_static_cleanup_runtime(|| {
                super::super::incident::with_cleanup_incident_owner(|_| {
                    super::resolve_failure(
                        NativeTypeIdentity::new([7; 32]),
                        NativeSourceAnchor::unavailable(),
                        7,
                        Some(&broadcast),
                        Some(&lifecycle),
                    )
                })
            });

        assert_eq!(status, NativeRuntimeStatus::SUCCESS);
        assert_eq!(TRACE.load(Ordering::Relaxed), 2);
        assert_eq!(incidents.len(), 2);
        assert!(runtime_incidents.is_empty());
        assert_eq!(RELEASED.load(Ordering::Relaxed), 0);

        drop(incidents);

        assert_eq!(RELEASED.load(Ordering::Relaxed), 48);
    }

    #[test]
    fn invalid_entry_cleanup_is_rejected_before_invoking_callbacks() {
        extern "C-unwind" fn unexpected(
            _: usize,
            _: &mut NativeInactiveFrame,
            _: &mut NativeBrayCallOutcome,
        ) -> NativeRuntimeStatus {
            panic!("invalid cleanup must not be invoked");
        }

        extern "C-unwind" fn unexpected_panic(_: usize) -> NativeRuntimeStatus {
            panic!("invalid cleanup must not create a panic report");
        }

        let cleanup = NativeValueCleanup::new(
            NativeCleanupExecution::ASYNCHRONOUS,
            None,
            unexpected,
            crate::test_support::panic_callbacks(unexpected_panic, unexpected_panic),
        );

        for (value, broadcast, lifecycle) in [(7, Some(&cleanup), None), (0, None, Some(&cleanup))]
        {
            assert_eq!(
                super::resolve_failure(
                    NativeTypeIdentity::new([7; 32]),
                    NativeSourceAnchor::unavailable(),
                    value,
                    broadcast,
                    lifecycle
                ),
                NativeRuntimeStatus::INVALID_ARGUMENT
            );
        }
    }
}
