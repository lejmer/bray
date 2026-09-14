use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::{NativeCleanupIncident, NativeRuntimeStatus};

pub(crate) struct OwnedCleanupIncident {
    kind: Option<IncidentKind>,
}

enum IncidentKind {
    Native(NativeCleanupIncident),
    Panic(Box<dyn Any + Send>),
}

impl OwnedCleanupIncident {
    pub(crate) fn native(incident: NativeCleanupIncident) -> Option<Self> {
        incident.is_valid().then_some(Self {
            kind: Some(IncidentKind::Native(incident)),
        })
    }

    pub(crate) fn panic(payload: Box<dyn Any + Send>) -> Self {
        Self {
            kind: Some(IncidentKind::Panic(payload)),
        }
    }

    pub(crate) const fn runtime_failure() -> Self {
        Self { kind: None }
    }

    pub(crate) fn report(mut self) -> NativeRuntimeStatus {
        self.dispose(true)
    }

    fn dispose(&mut self, report: bool) -> NativeRuntimeStatus {
        let incident = match self.kind.take() {
            Some(IncidentKind::Native(incident)) => incident,
            Some(IncidentKind::Panic(payload)) => return dispose_panic(payload),
            None => return NativeRuntimeStatus::SUCCESS,
        };

        let reporting = report
            .then(|| catch_unwind(AssertUnwindSafe(|| (incident.report())(incident.payload()))));

        let destruction = catch_unwind(AssertUnwindSafe(|| {
            (incident.destroy())(incident.payload())
        }));

        let status = match reporting {
            Some(Ok(status)) => status,
            Some(Err(payload)) => {
                dispose_panic(payload);

                NativeRuntimeStatus::PANICKED
            }
            None => NativeRuntimeStatus::SUCCESS,
        };

        if let Err(payload) = destruction {
            dispose_panic(payload);

            return if status.is_success() {
                NativeRuntimeStatus::PANICKED
            } else {
                status
            };
        }

        status
    }
}

pub(crate) fn dispose_panic(payload: Box<dyn Any + Send>) -> NativeRuntimeStatus {
    let mut current = Some(payload);
    let mut pending = Vec::new();
    let mut status = NativeRuntimeStatus::SUCCESS;

    while let Some(payload) = current {
        match payload.downcast::<crate::RuntimePanic>() {
            Ok(mut panic) => pending.extend(panic.take_payloads().rev()),
            Err(payload) => {
                if let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(payload))) {
                    status = NativeRuntimeStatus::PANICKED;
                    pending.push(payload);
                }
            }
        }

        current = pending.pop();
    }

    status
}

impl Drop for OwnedCleanupIncident {
    fn drop(&mut self) {
        self.dispose(false);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::panic::panic_any;
    use std::sync::{Arc, Mutex};

    use bray_runtime_abi::{
        NativeCleanupIncident, NativeRuntimeStatus, NativeSourceAnchor, NativeTypeIdentity,
    };

    use super::OwnedCleanupIncident;
    use crate::{CleanupIncidentOrigin, CleanupIncidentProducer, CleanupReportSink, RuntimePanic};

    thread_local! {
        static EVENTS: RefCell<Vec<(&'static str, usize)>> = const { RefCell::new(Vec::new()) };
        static REENTRANT: RefCell<Option<CleanupReportSink>> = const { RefCell::new(None) };
    }

    fn record(event: &'static str, payload: usize) {
        EVENTS.with_borrow_mut(|events| events.push((event, payload)));
    }

    fn events() -> Vec<(&'static str, usize)> {
        EVENTS.with_borrow_mut(std::mem::take)
    }

    extern "C-unwind" fn report(payload: usize) -> NativeRuntimeStatus {
        record("report", payload);

        NativeRuntimeStatus::SUCCESS
    }

    extern "C-unwind" fn destroy(payload: usize) {
        record("release", payload);
    }

    fn native(
        payload: usize,
        report: extern "C-unwind" fn(usize) -> NativeRuntimeStatus,
        destroy: extern "C-unwind" fn(usize),
    ) -> OwnedCleanupIncident {
        OwnedCleanupIncident::native(NativeCleanupIncident::new(
            payload,
            NativeTypeIdentity::new([7; 32]),
            NativeSourceAnchor::unavailable(),
            report,
            destroy,
        ))
        .unwrap()
    }

    #[test]
    fn reporting_borrows_then_destroys_the_owned_payload_once() {
        assert!(native(3, report, destroy).report().is_success());
        assert_eq!(events(), [("report", 3), ("release", 3)]);
    }

    #[test]
    fn dropping_an_unreported_incident_releases_its_payload_once() {
        drop(native(5, report, destroy));
        assert_eq!(events(), [("release", 5)]);
    }

    struct Release(usize);

    impl Drop for Release {
        fn drop(&mut self) {
            record("panic release", self.0);
        }
    }

    extern "C-unwind" fn failing_report(payload: usize) -> NativeRuntimeStatus {
        record("report", payload);
        panic_any(Release(1));
    }

    extern "C-unwind" fn failing_destroy(payload: usize) {
        record("release", payload);
        panic_any(Release(2));
    }

    #[test]
    fn report_and_release_failures_keep_payload_order_and_continue() {
        assert_eq!(
            native(7, failing_report, failing_destroy).report(),
            NativeRuntimeStatus::PANICKED
        );

        assert!(native(8, report, destroy).report().is_success());

        assert_eq!(
            events(),
            [
                ("report", 7),
                ("release", 7),
                ("panic release", 1),
                ("panic release", 2),
                ("report", 8),
                ("release", 8),
            ]
        );

        drop(native(9, report, failing_destroy));
        assert_eq!(events(), [("release", 9), ("panic release", 2)]);
    }

    #[test]
    fn reporting_status_keeps_its_cause_when_release_also_panics() {
        extern "C-unwind" fn rejected(_: usize) -> NativeRuntimeStatus {
            NativeRuntimeStatus::INVALID_ARGUMENT
        }

        assert_eq!(
            native(1, rejected, failing_destroy).report(),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(events(), [("release", 1), ("panic release", 2)]);
    }

    #[test]
    fn a_rust_destructor_failure_does_not_discard_later_cleanup() {
        struct FailingRelease;

        impl Drop for FailingRelease {
            fn drop(&mut self) {
                record("release", 1);
                panic_any(RuntimePanic::new(Release(2)));
            }
        }

        let mut panic = RuntimePanic::new(FailingRelease);
        panic.push_suppressed(Box::new(Release(3)));
        drop(panic);

        assert_eq!(
            events(),
            [("release", 1), ("panic release", 2), ("panic release", 3)]
        );
    }

    fn transfer(sink: &CleanupReportSink, payload: Box<dyn std::any::Any + Send>) {
        sink.transfer_erased(
            CleanupIncidentProducer::SynchronousRoot,
            CleanupIncidentOrigin::new(
                bray_runtime_model::ProtectedAsyncFrameId::new([9; 32]),
                bray_runtime_model::ProtectedFrameStateId::new(1),
            ),
            payload,
        );
    }

    extern "C-unwind" fn reentrant_report(payload: usize) -> NativeRuntimeStatus {
        REENTRANT.with_borrow(|sink| transfer(sink.as_ref().unwrap(), Box::new(Release(3))));

        report(payload)
    }

    extern "C-unwind" fn reentrant_destroy(payload: usize) {
        REENTRANT.with_borrow(|sink| transfer(sink.as_ref().unwrap(), Box::new(Release(4))));
        destroy(payload);
    }

    #[test]
    fn callbacks_append_after_existing_work_without_holding_the_sink_lock() {
        let sink = CleanupReportSink::new();
        REENTRANT.with_borrow_mut(|slot| *slot = Some(sink.clone()));
        transfer(&sink, Box::new(()));
        transfer(&sink, Box::new(()));
        let mut ordinals = Vec::new();

        sink.drain(|mut incident| {
            ordinals.push(incident.ordinal());

            match incident.ordinal() {
                0 => assert!(
                    native(1, reentrant_report, reentrant_destroy)
                        .report()
                        .is_success()
                ),
                1 => assert!(native(2, report, destroy).report().is_success()),
                _ => {}
            }

            assert!(incident.dispose().is_success());
        });

        assert_eq!(ordinals, [0, 1, 2, 3]);

        assert_eq!(
            events(),
            [
                ("report", 1),
                ("release", 1),
                ("report", 2),
                ("release", 2),
                ("panic release", 3),
                ("panic release", 4),
            ]
        );

        REENTRANT.with_borrow_mut(|slot| *slot = None);
        assert_eq!(sink.pending_count(), 0);
    }

    #[test]
    fn unwinding_a_sink_consumer_releases_the_detached_batch() {
        let sink = CleanupReportSink::new();

        for payload in 1..=3 {
            transfer(&sink, Box::new(Release(payload)));
        }

        let failure = std::panic::catch_unwind(|| sink.drain(|_| panic_any(Release(4))));

        assert!(failure.is_err());
        assert_eq!(sink.pending_count(), 0);

        assert_eq!(
            events(),
            [
                ("panic release", 1),
                ("panic release", 2),
                ("panic release", 3)
            ]
        );

        drop(failure);

        assert_eq!(events(), [("panic release", 4)]);
    }

    #[test]
    fn runtime_primary_and_nested_children_release_in_encounter_order() {
        let mut child = RuntimePanic::new(Release(2));
        child.push_suppressed(Box::new(Release(3)));
        let mut primary = RuntimePanic::new(Release(1));
        primary.push_suppressed(Box::new(child));
        primary.push_suppressed(Box::new(Release(4)));

        drop(primary);

        assert_eq!(
            events(),
            [
                ("panic release", 1),
                ("panic release", 2),
                ("panic release", 3),
                ("panic release", 4),
            ]
        );
    }

    #[test]
    fn deep_runtime_reports_dispose_on_a_small_stack() {
        std::thread::Builder::new()
            .stack_size(128 * 1024)
            .spawn(|| {
                let mut panic = RuntimePanic::new(Release(0));

                for index in 1..20_000 {
                    let mut parent = RuntimePanic::new(Release(index));
                    parent.push_suppressed(Box::new(panic));
                    panic = parent;
                }

                let sink = CleanupReportSink::new();
                transfer(&sink, Box::new(panic));
                sink.drain(drop);
                let released = events();
                assert_eq!(released.len(), 20_000);

                assert!(
                    released
                        .iter()
                        .map(|(_, index)| *index)
                        .eq((0..20_000).rev())
                );
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn erased_payload_transfer_keeps_identity_and_release_owner() {
        struct Payload(Arc<Mutex<Vec<usize>>>);

        impl Drop for Payload {
            fn drop(&mut self) {
                self.0.lock().unwrap().push(std::ptr::from_ref(self).addr());
            }
        }

        let released = Arc::new(Mutex::new(Vec::new()));
        let payload = Box::new(Payload(Arc::clone(&released)));
        let address = std::ptr::from_ref(payload.as_ref()).addr();

        let sink = CleanupReportSink::new();
        transfer(&sink, payload);

        sink.drain(|mut incident| {
            assert!(incident.payload_is::<Payload>());
            assert!(incident.dispose().is_success());
        });

        assert_eq!(*released.lock().unwrap(), [address]);
    }
}
