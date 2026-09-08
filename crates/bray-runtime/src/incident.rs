use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::{
    NativeBrayCallOutcome, NativeCleanupIncident, NativePanicReportCallbacks, NativeRuntimeStatus,
};

pub(crate) struct OwnedCleanupIncident {
    kind: CleanupIncidentKind,
}

enum CleanupIncidentKind {
    Native(Option<NativeCleanupIncident>),
    NativePanic {
        payload: Option<usize>,
        callbacks: NativePanicReportCallbacks,
    },
    Cancellation,
    Panic {
        _payload: Box<dyn Any + Send>,
    },
    RuntimeFailure,
}

impl OwnedCleanupIncident {
    pub(crate) fn native(incident: NativeCleanupIncident) -> Option<Self> {
        incident.is_valid().then(|| Self {
            kind: CleanupIncidentKind::Native(Some(incident)),
        })
    }

    pub(crate) fn panic(payload: Box<dyn Any + Send>) -> Self {
        Self {
            kind: CleanupIncidentKind::Panic { _payload: payload },
        }
    }

    pub(crate) fn boundary(
        outcome: NativeBrayCallOutcome,
        callbacks: NativePanicReportCallbacks,
    ) -> Option<Self> {
        let kind = if let Some(payload) = outcome.panic_report() {
            CleanupIncidentKind::NativePanic {
                payload: Some(payload),
                callbacks,
            }
        } else if outcome.is_cancelled() {
            CleanupIncidentKind::Cancellation
        } else {
            return None;
        };

        Some(Self { kind })
    }

    pub(crate) const fn runtime_failure() -> Self {
        Self {
            kind: CleanupIncidentKind::RuntimeFailure,
        }
    }

    pub(crate) fn take_panic_report(&mut self) -> Option<usize> {
        match &mut self.kind {
            CleanupIncidentKind::NativePanic { payload, .. } => payload.take(),
            CleanupIncidentKind::Native(_)
            | CleanupIncidentKind::Cancellation
            | CleanupIncidentKind::Panic { .. }
            | CleanupIncidentKind::RuntimeFailure => None,
        }
    }

    pub(crate) fn attach_to_report(&mut self, primary: usize) -> Option<usize> {
        let (secondary, callbacks) = match &mut self.kind {
            CleanupIncidentKind::Native(incident) => {
                let incident = incident.take()?;
                let callbacks = incident.panics();
                let report = (callbacks.construct_cleanup())(&incident);

                (report, callbacks)
            }
            CleanupIncidentKind::NativePanic { payload, callbacks } => {
                (payload.take()?, *callbacks)
            }
            CleanupIncidentKind::Cancellation
            | CleanupIncidentKind::Panic { .. }
            | CleanupIncidentKind::RuntimeFailure => return None,
        };

        Some((callbacks.suppress())(primary, secondary))
    }

    pub(super) fn report(mut self) -> bool {
        if let CleanupIncidentKind::NativePanic { payload, callbacks } = &mut self.kind {
            let Some(payload) = payload.take() else {
                return true;
            };

            let reporting_failed = callback_failed(callbacks.report(), payload);
            let destruction_failed = callback_failed(callbacks.destroy(), payload);

            return reporting_failed || destruction_failed;
        }

        let CleanupIncidentKind::Native(incident) = &mut self.kind else {
            return false;
        };

        let Some(incident) = incident.take() else {
            return true;
        };

        let reporting_failed = catch_unwind(AssertUnwindSafe(|| (incident.report())(&incident)))
            .map_or(true, |status| !status.is_success());

        let destruction_failed = catch_unwind(AssertUnwindSafe(|| {
            (incident.destroy())(incident.payload())
        }))
        .map_or(true, |outcome| {
            Self::boundary(outcome, incident.panics()).is_some_and(Self::report)
        });

        reporting_failed || destruction_failed
    }
}

impl Drop for OwnedCleanupIncident {
    fn drop(&mut self) {
        if let CleanupIncidentKind::NativePanic { payload, callbacks } = &mut self.kind {
            if let Some(payload) = payload.take() {
                let _ = callback_failed(callbacks.destroy(), payload);
            }

            return;
        }

        let CleanupIncidentKind::Native(incident) = &mut self.kind else {
            return;
        };

        if let Some(incident) = incident.take()
            && let Ok(outcome) = catch_unwind(AssertUnwindSafe(|| {
                (incident.destroy())(incident.payload())
            }))
        {
            drop(Self::boundary(outcome, incident.panics()));
        }
    }
}

fn callback_failed(
    callback: extern "C-unwind" fn(usize) -> NativeRuntimeStatus,
    payload: usize,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| callback(payload))).map_or(true, |status| !status.is_success())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_abi::{
        NativeBrayCallOutcome, NativeCleanupIncident, NativeRuntimeStatus, NativeSourceAnchor,
        NativeTypeIdentity,
    };

    use super::OwnedCleanupIncident;

    static REPORTS: AtomicUsize = AtomicUsize::new(0);
    static DESTROYS: AtomicUsize = AtomicUsize::new(0);

    extern "C-unwind" fn report(incident: &NativeCleanupIncident) -> NativeRuntimeStatus {
        REPORTS.fetch_add(incident.payload(), Ordering::SeqCst);

        NativeRuntimeStatus::SUCCESS
    }

    extern "C-unwind" fn destroy(payload: usize) -> NativeBrayCallOutcome {
        DESTROYS.fetch_add(payload, Ordering::SeqCst);

        NativeBrayCallOutcome::completed()
    }

    extern "C-unwind" fn unexpected_panic(_: usize) -> NativeRuntimeStatus {
        panic!("this error destructor must not produce a native panic");
    }

    fn native_incident(payload: usize) -> OwnedCleanupIncident {
        OwnedCleanupIncident::native(NativeCleanupIncident::new(
            payload,
            NativeTypeIdentity::new([7; 32]),
            NativeSourceAnchor::unavailable(),
            report,
            destroy,
            crate::test_support::panic_callbacks(unexpected_panic, unexpected_panic),
        ))
        .unwrap_or_else(|| panic!("test incident must satisfy the native contract"))
    }

    extern "C-unwind" fn reporting_panics(incident: &NativeCleanupIncident) -> NativeRuntimeStatus {
        REPORTS.fetch_add(incident.payload(), Ordering::SeqCst);

        panic!("test reporting failure");
    }

    #[test]
    fn native_payloads_are_reported_or_dropped_exactly_once() {
        REPORTS.store(0, Ordering::SeqCst);
        DESTROYS.store(0, Ordering::SeqCst);

        assert!(!native_incident(3).report());
        assert_eq!(REPORTS.load(Ordering::SeqCst), 3);
        assert_eq!(DESTROYS.load(Ordering::SeqCst), 3);

        drop(native_incident(5));

        assert_eq!(DESTROYS.load(Ordering::SeqCst), 8);

        let reports = crate::CleanupReportSink::new();

        let origin = crate::CleanupIncidentOrigin::new(
            bray_runtime_model::ProtectedAsyncFrameId::new([9; 32]),
            bray_runtime_model::ProtectedFrameStateId::new(1),
        );

        reports.transfer(
            crate::CleanupIncidentProducer::SynchronousRoot,
            origin,
            native_incident(7),
        );

        reports.drain(|incident| assert!(!incident.report_native_payload()));

        assert_eq!(REPORTS.load(Ordering::SeqCst), 10);
        assert_eq!(DESTROYS.load(Ordering::SeqCst), 15);

        let mut panic = crate::RuntimePanic::new("primary");

        panic.push_suppressed(Box::new(native_incident(11)));

        assert_eq!(panic.suppressed_count(), 1);

        drop(panic);

        assert_eq!(REPORTS.load(Ordering::SeqCst), 10);
        assert_eq!(DESTROYS.load(Ordering::SeqCst), 26);

        let incident = OwnedCleanupIncident::native(NativeCleanupIncident::new(
            13,
            NativeTypeIdentity::new([7; 32]),
            NativeSourceAnchor::unavailable(),
            reporting_panics,
            destroy,
            crate::test_support::panic_callbacks(unexpected_panic, unexpected_panic),
        ))
        .unwrap();

        assert!(incident.report());

        assert_eq!(REPORTS.load(Ordering::SeqCst), 23);
        assert_eq!(DESTROYS.load(Ordering::SeqCst), 39);
    }

    #[test]
    fn opaque_panics_retain_representation_specific_ownership() {
        static OBSERVED: AtomicUsize = AtomicUsize::new(0);
        static RELEASED: AtomicUsize = AtomicUsize::new(0);

        extern "C-unwind" fn observe(payload: usize) -> NativeRuntimeStatus {
            OBSERVED.fetch_add(payload, Ordering::SeqCst);

            NativeRuntimeStatus::SUCCESS
        }

        extern "C-unwind" fn release(payload: usize) -> NativeRuntimeStatus {
            RELEASED.fetch_add(payload, Ordering::SeqCst);

            NativeRuntimeStatus::SUCCESS
        }

        extern "C-unwind" fn failed_observer(_: usize) -> NativeRuntimeStatus {
            panic!("reporting failed before destruction");
        }

        let callbacks = crate::test_support::panic_callbacks(observe, release);

        let panic = |payload, callbacks| {
            OwnedCleanupIncident::boundary(
                NativeBrayCallOutcome::panicked(payload).unwrap(),
                callbacks,
            )
            .unwrap()
        };

        assert!(!panic(16, callbacks).report());

        assert_eq!(OBSERVED.load(Ordering::SeqCst), 16);
        assert_eq!(RELEASED.load(Ordering::SeqCst), 16);

        drop(panic(32, callbacks));

        assert_eq!(OBSERVED.load(Ordering::SeqCst), 16);
        assert_eq!(RELEASED.load(Ordering::SeqCst), 48);

        assert!(
            panic(
                64,
                crate::test_support::panic_callbacks(failed_observer, release)
            )
            .report()
        );

        assert_eq!(RELEASED.load(Ordering::SeqCst), 112);

        assert!(
            OwnedCleanupIncident::boundary(NativeBrayCallOutcome::completed(), callbacks).is_none()
        );

        assert!(
            !OwnedCleanupIncident::boundary(NativeBrayCallOutcome::cancelled(), callbacks)
                .unwrap()
                .report()
        );

        assert_eq!(OBSERVED.load(Ordering::SeqCst), 16);
        assert_eq!(RELEASED.load(Ordering::SeqCst), 112);
    }

    #[test]
    fn destructor_panics_are_owned_until_reported_or_dropped() {
        static ERROR_RELEASES: AtomicUsize = AtomicUsize::new(0);
        static PANIC_REPORTS: AtomicUsize = AtomicUsize::new(0);
        static PANIC_RELEASES: AtomicUsize = AtomicUsize::new(0);

        extern "C-unwind" fn report_error(_: &NativeCleanupIncident) -> NativeRuntimeStatus {
            NativeRuntimeStatus::SUCCESS
        }

        extern "C-unwind" fn destroy_error(_: usize) -> NativeBrayCallOutcome {
            ERROR_RELEASES.fetch_add(1, Ordering::SeqCst);

            NativeBrayCallOutcome::panicked(16).unwrap()
        }

        extern "C-unwind" fn report_panic(payload: usize) -> NativeRuntimeStatus {
            assert_eq!(payload, 16);
            PANIC_REPORTS.fetch_add(1, Ordering::SeqCst);

            NativeRuntimeStatus::SUCCESS
        }

        extern "C-unwind" fn destroy_panic(payload: usize) -> NativeRuntimeStatus {
            assert_eq!(payload, 16);
            PANIC_RELEASES.fetch_add(1, Ordering::SeqCst);

            NativeRuntimeStatus::SUCCESS
        }

        let incident = || {
            OwnedCleanupIncident::native(NativeCleanupIncident::new(
                8,
                NativeTypeIdentity::new([3; 32]),
                NativeSourceAnchor::unavailable(),
                report_error,
                destroy_error,
                crate::test_support::panic_callbacks(report_panic, destroy_panic),
            ))
            .unwrap()
        };

        assert!(!incident().report());

        drop(incident());

        assert_eq!(ERROR_RELEASES.load(Ordering::SeqCst), 2);
        assert_eq!(PANIC_REPORTS.load(Ordering::SeqCst), 1);
        assert_eq!(PANIC_RELEASES.load(Ordering::SeqCst), 2);
    }
}
