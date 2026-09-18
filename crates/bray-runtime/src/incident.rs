use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::{NativeCleanupIncident, NativeRuntimeStatus};

pub(crate) struct OwnedCleanupIncident {
    kind: Option<IncidentKind>,
}

enum IncidentKind {
    Native(NativeCleanupIncident),
    Report(bray_runtime_abi::NativePanicReport),
    Panic(Box<dyn Any + Send>),
}

impl OwnedCleanupIncident {
    pub(crate) fn outcome(mut outcome: bray_runtime_abi::NativeRunOutcome) -> Option<Self> {
        match outcome.state() {
            bray_runtime_abi::NativeRunState::COMPLETED => None,
            bray_runtime_abi::NativeRunState::PANICKED => {
                Some(Self::report_owner(outcome.take_report()))
            }
            _ => Some(Self::runtime_failure()),
        }
    }

    pub(crate) fn native(incident: NativeCleanupIncident) -> Option<Self> {
        incident.is_valid().then_some(Self {
            kind: Some(IncidentKind::Native(incident)),
        })
    }

    pub(crate) fn report_owner(report: bray_runtime_abi::NativePanicReport) -> Self {
        Self {
            kind: Some(IncidentKind::Report(report)),
        }
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
            Some(IncidentKind::Report(mut owned)) => return owned.consume(report),
            None => return NativeRuntimeStatus::SUCCESS,
        };

        let reporting = report
            .then(|| catch_unwind(AssertUnwindSafe(|| (incident.report())(incident.payload()))));

        let mut published =
            bray_runtime_abi::NativeRunOutcome::new(bray_runtime_abi::NativeRunState::COMPLETED, 0);

        let mut released =
            bray_runtime_abi::NativeRunOutcome::new(bray_runtime_abi::NativeRunState::COMPLETED, 0);

        let destruction = catch_unwind(AssertUnwindSafe(|| {
            (incident.destroy())(incident.payload(), &mut published, &mut released)
        }));

        let mut status = match reporting {
            Some(Ok(status)) => status,
            Some(Err(payload)) => {
                dispose_panic(payload);

                NativeRuntimeStatus::PANICKED
            }
            None => NativeRuntimeStatus::SUCCESS,
        };

        for mut incident in [published, released].into_iter().filter_map(Self::outcome) {
            let disposal = incident.dispose(report);

            if status.is_success() {
                status = if disposal.is_success() {
                    NativeRuntimeStatus::PANICKED
                } else {
                    disposal
                };
            }
        }

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
    let mut pending = Some(payload);
    let mut status = NativeRuntimeStatus::SUCCESS;

    while let Some(payload) = pending.take() {
        match payload.downcast::<crate::RuntimePanic>() {
            Ok(report) => {
                let mut report = *report;
                let found = report.consume(false);

                if status.is_success() {
                    status = found;
                }
            }
            Err(payload) => {
                if let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(payload))) {
                    status = NativeRuntimeStatus::PANICKED;
                    pending = Some(payload);
                }
            }
        }
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

    extern "C-unwind" fn destroy(
        payload: usize,
        _: &mut bray_runtime_abi::NativeRunOutcome,
        _: &mut bray_runtime_abi::NativeRunOutcome,
    ) {
        record("release", payload);
    }

    fn native(
        payload: usize,
        report: extern "C-unwind" fn(usize) -> NativeRuntimeStatus,
        destroy: bray_runtime_abi::NativeCleanupIncidentDestroyCallback,
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
    fn finalizer_bridge_retains_published_incident_before_callback_panic() {
        let _failure = crate::outgoing::tests::reject_admission();

        let incidents = crate::product::collect_finalizer_incidents(|destination, outcome| {
            *destination = NativeCleanupIncident::new(
                17,
                NativeTypeIdentity::new([7; 32]),
                NativeSourceAnchor::unavailable(),
                report,
                destroy,
            );

            *outcome = published_panic(18);

            panic_any(Release(19));
        });

        assert_eq!(incidents.iter().flatten().count(), 3);

        for incident in incidents.into_iter().flatten() {
            assert!(incident.report().is_success());
        }

        assert_eq!(
            events(),
            [
                ("report", 17),
                ("release", 17),
                ("native release", 18),
                ("panic release", 19)
            ]
        );
    }

    extern "C" fn release_published_panic(
        payload: usize,
        _: usize,
        _: &mut bray_runtime_abi::NativeRunOutcome,
    ) {
        record("native release", payload);
    }

    fn published_panic(payload: usize) -> bray_runtime_abi::NativeRunOutcome {
        bray_runtime_abi::NativeRunOutcome::panicked(crate::frame::native_report(
            bray_runtime_abi::NativePanicPrimary::new(
                bray_runtime_abi::NativePanicCause::MESSAGE,
                NativeSourceAnchor::unavailable(),
                bray_runtime_abi::NativePanicMessage::new(
                    payload,
                    0,
                    None,
                    Some(release_published_panic),
                ),
            ),
        ))
    }

    extern "C-unwind" fn published_then_panicking_destroy(
        payload: usize,
        outcome: &mut bray_runtime_abi::NativeRunOutcome,
        release: &mut bray_runtime_abi::NativeRunOutcome,
    ) {
        *outcome = published_panic(payload);
        *release = published_panic(payload + 1);

        panic_any(Release(payload + 2));
    }

    #[test]
    fn typed_error_destruction_retains_a_published_panic_before_unwinding() {
        let _failure = crate::outgoing::tests::reject_admission();

        assert_eq!(
            native(21, report, published_then_panicking_destroy).report(),
            NativeRuntimeStatus::PANICKED
        );

        assert_eq!(
            events(),
            [
                ("report", 21),
                ("native release", 21),
                ("native release", 22),
                ("panic release", 23)
            ]
        );
    }

    #[test]
    fn finalizer_bridge_disposes_unreported_incidents_after_transfer() {
        let incidents = crate::product::collect_finalizer_incidents(|destination, _| {
            *destination = NativeCleanupIncident::new(
                19,
                NativeTypeIdentity::new([7; 32]),
                NativeSourceAnchor::unavailable(),
                report,
                destroy,
            );

            panic_any(Release(20));
        });

        drop(incidents);

        assert_eq!(events(), [("release", 19), ("panic release", 20)]);
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

    struct FailingCompletion;

    impl Drop for FailingCompletion {
        fn drop(&mut self) {
            panic_any(Release(2));
        }
    }

    struct FailingCleanupFrame(crate::test_support::TestFrame);

    impl crate::ProtectedFrame for FailingCleanupFrame {
        type Output = FailingCompletion;

        fn descriptor(&self) -> &bray_runtime_model::ProtectedFrameDescriptor {
            self.0.descriptor()
        }

        fn resume(
            self: std::pin::Pin<&mut Self>,
            _: crate::FrameContext,
        ) -> crate::FrameProgress<Self::Output> {
            crate::FrameProgress::Completed(FailingCompletion)
        }

        fn broadcast_tasks(self: std::pin::Pin<&mut Self>) {
            panic_any(Release(1));
        }

        fn resolve_lifecycle(self: std::pin::Pin<&mut Self>, _: crate::FrameExit) {
            panic_any(Release(3));
        }
    }

    impl Drop for FailingCleanupFrame {
        fn drop(&mut self) {
            panic_any(Release(4));
        }
    }

    #[test]
    fn admitted_task_retains_every_cleanup_failure_after_further_admission_fails() {
        let task = crate::TaskControlBlock::start(
            crate::test_support::admit_task(),
            FailingCleanupFrame(crate::test_support::TestFrame::completing(0)),
        );

        let _failure = crate::outgoing::tests::reject_admission();

        assert!(matches!(
            crate::TaskAdmission::new(),
            Err(crate::TaskStartError::OutgoingStorageUnavailable)
        ));

        assert!(matches!(
            task.resume(),
            Ok(crate::TaskResumeStatus::Terminal(
                crate::RunOutcomeKind::Panicked
            ))
        ));

        let crate::RunOutcome::Panicked(report) = task.take_outcome().unwrap() else {
            panic!("cleanup failures must retain a panic outcome");
        };

        assert_eq!(report.suppressed_count(), 3);

        drop(task);

        assert!(events().is_empty());

        drop(report);

        assert_eq!(
            events(),
            [
                ("panic release", 1),
                ("panic release", 2),
                ("panic release", 3),
                ("panic release", 4)
            ]
        );
    }

    extern "C-unwind" fn failing_report(payload: usize) -> NativeRuntimeStatus {
        record("report", payload);
        panic_any(Release(1));
    }

    extern "C-unwind" fn failing_destroy(
        payload: usize,
        _: &mut bray_runtime_abi::NativeRunOutcome,
        _: &mut bray_runtime_abi::NativeRunOutcome,
    ) {
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
        struct FailingRelease(Option<RuntimePanic>);

        impl Drop for FailingRelease {
            fn drop(&mut self) {
                record("release", 1);
                panic_any(self.0.take().unwrap());
            }
        }

        let mut admitted = crate::outgoing::OutgoingRecords::admit(2).unwrap();
        let nested = RuntimePanic::new(Release(2), &mut admitted);
        let mut panic = RuntimePanic::new(FailingRelease(Some(nested)), &mut admitted);

        panic.push_suppressed(
            Box::new(Release(3)),
            &mut crate::outgoing::OutgoingRecords::admit(1).unwrap(),
        );

        drop(panic);

        assert_eq!(
            events(),
            [("release", 1), ("panic release", 2), ("panic release", 3)]
        );
    }

    fn transfer(sink: &CleanupReportSink, payload: Box<dyn std::any::Any + Send>) {
        let mut admitted = crate::outgoing::OutgoingRecords::admit(1).unwrap();
        let panic = RuntimePanic::from_payload(payload, &mut admitted);

        sink.transfer(
            CleanupIncidentProducer::SynchronousRoot,
            CleanupIncidentOrigin::new(
                bray_runtime_model::ProtectedAsyncFrameId::new([9; 32]),
                bray_runtime_model::ProtectedFrameStateId::new(1),
            ),
            panic,
            &mut admitted,
        );
    }

    extern "C-unwind" fn reentrant_report(payload: usize) -> NativeRuntimeStatus {
        REENTRANT.with_borrow(|sink| {
            let sink = sink.as_ref().unwrap();
            transfer(sink, Box::new(Release(3)));
            sink.drain(|_| panic!("nested reporting must defer to the active drain"));
        });

        report(payload)
    }

    extern "C-unwind" fn reentrant_destroy(
        payload: usize,
        outcome: &mut bray_runtime_abi::NativeRunOutcome,
        release: &mut bray_runtime_abi::NativeRunOutcome,
    ) {
        REENTRANT.with_borrow(|sink| {
            let sink = sink.as_ref().unwrap();
            transfer(sink, Box::new(Release(4)));
            sink.drain(|_| panic!("nested release must defer to the active drain"));
        });

        destroy(payload, outcome, release);
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

        transfer(&sink, Box::new(Release(5)));

        sink.drain(drop);

        assert_eq!(events(), [("panic release", 5)]);
        assert_eq!(sink.pending_count(), 0);
    }

    #[test]
    fn concurrent_drain_leaves_pending_incidents_to_the_active_drain() {
        let sink = CleanupReportSink::new();

        transfer(&sink, Box::new(()));
        transfer(&sink, Box::new(()));

        let mut ordinals = Vec::new();

        sink.drain(|incident| {
            ordinals.push(incident.ordinal());

            if incident.ordinal() == 0 {
                std::thread::scope(|scope| {
                    scope.spawn(|| {
                        transfer(&sink, Box::new(()));
                        sink.drain(|_| panic!("concurrent drain must defer to the active drain"));
                    });
                });
            }
        });

        assert_eq!(ordinals, [0, 1, 2]);
        assert_eq!(sink.pending_count(), 0);
    }

    #[test]
    fn runtime_primary_and_nested_children_release_in_encounter_order() {
        let mut child_admitted = crate::outgoing::OutgoingRecords::admit(2).unwrap();
        let mut child = RuntimePanic::new(Release(2), &mut child_admitted);

        child.push_suppressed(Box::new(Release(3)), &mut child_admitted);

        let mut primary_admitted = crate::outgoing::OutgoingRecords::admit(3).unwrap();
        let mut primary = RuntimePanic::new(Release(1), &mut primary_admitted);

        primary.push_suppressed(Box::new(child), &mut primary_admitted);

        primary.push_suppressed(Box::new(Release(4)), &mut primary_admitted);

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
                let mut initial = crate::outgoing::OutgoingRecords::admit(1).unwrap();
                let mut panic = RuntimePanic::new(Release(0), &mut initial);

                for index in 1..20_000 {
                    let mut admitted = crate::outgoing::OutgoingRecords::admit(2).unwrap();
                    let mut parent = RuntimePanic::new(Release(index), &mut admitted);

                    parent.push_suppressed(Box::new(panic), &mut admitted);

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
            assert!(incident.dispose().is_success());
        });

        assert_eq!(*released.lock().unwrap(), [address]);
    }
}
