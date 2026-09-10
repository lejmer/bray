use std::io::Write;

use bray_runtime_abi::{
    NativeCleanupIncident, NativeRunOutcome, NativeRuntimeStatus, NativeSourceAnchor,
    NativeTypeIdentity,
};

use crate::{CleanupIncident, CleanupIncidentOrigin, CleanupIncidentProducer, CleanupReportSink};

use super::frame::NativeTerminalState;

// Each resume or synchronous callback borrows its owning run for that callback's scope.
scoped_tls::scoped_thread_local!(static CURRENT_INCIDENT_OWNER: NativeTerminalState);

pub(super) fn with_incident_owner<T>(
    owner: &NativeTerminalState,
    callback: impl FnOnce() -> T,
) -> T {
    CURRENT_INCIDENT_OWNER.set(owner, callback)
}

pub(super) fn retain_cleanup_incident(
    incident: crate::incident::OwnedCleanupIncident,
) -> Result<(), crate::incident::OwnedCleanupIncident> {
    if !CURRENT_INCIDENT_OWNER.is_set() {
        return Err(incident);
    }

    CURRENT_INCIDENT_OWNER.with(|owner| owner.record_cleanup_incident(incident));

    Ok(())
}

const VALUE_CLEANUP_IDENTITY: [u8; 32] = *b"bray.value.cleanup.v1\0\0\0\0\0\0\0\0\0\0\0";

pub(super) fn retain_cleanup_incidents(incidents: Vec<crate::incident::OwnedCleanupIncident>) {
    if incidents.is_empty() {
        return;
    }

    super::state::with_runtime(|runtime| {
        let producer = crate::current_task_execution_context()
            .map_or(crate::CleanupIncidentProducer::SynchronousRoot, |context| {
                crate::CleanupIncidentProducer::Task(context.task())
            });

        let origin = crate::CleanupIncidentOrigin::new(
            bray_runtime_model::ProtectedAsyncFrameId::new(VALUE_CLEANUP_IDENTITY),
            bray_runtime_model::ProtectedFrameStateId::new(0),
        );

        for incident in incidents {
            if let Err(incident) = retain_cleanup_incident(incident) {
                runtime
                    .cleanup_reports
                    .transfer_owned(producer, origin, incident);
            }
        }
    })
    .unwrap_or_else(|status| std::panic::resume_unwind(Box::new(status)));
}

pub(crate) fn with_cleanup_incident_owner<T>(
    callback: impl FnOnce(&dyn Fn(crate::incident::OwnedCleanupIncident)) -> T,
) -> (T, Vec<crate::incident::OwnedCleanupIncident>) {
    let terminal = NativeTerminalState::new();

    let result = with_incident_owner(&terminal, || {
        callback(&|incident| terminal.record_cleanup_incident(incident))
    });

    (result, terminal.take_cleanup_incidents())
}

native_export! {
    /// Consumes a valid error payload, retaining it in the current run or destroying it on rejection.
    pub extern "C" fn bray_runtime_cleanup_incident_transfer(incident: &NativeCleanupIncident) -> NativeRuntimeStatus {
        let Some(incident) = crate::incident::OwnedCleanupIncident::native(*incident) else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        match retain_cleanup_incident(incident) {
            Ok(()) => NativeRuntimeStatus::SUCCESS,
            Err(incident) => {
                drop(incident);

                NativeRuntimeStatus::NOT_INITIALIZED
            }
        }
    }
}

native_export! {
    /// Reports compiler-retained metadata while leaving the error payload owned by its caller.
    pub extern "C" fn bray_runtime_cleanup_incident_detail_reporting(incident: &NativeCleanupIncident) -> NativeRuntimeStatus {
        if !incident.is_valid() {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }

        report_error_details("cleanup_error", incident.type_identity(), incident.source())
    }
}

pub(super) fn report_error_details(
    kind: &str,
    identity: NativeTypeIdentity,
    source: NativeSourceAnchor,
) -> NativeRuntimeStatus {
    match write_error_details(&mut std::io::stderr().lock(), kind, identity, source) {
        Ok(()) => NativeRuntimeStatus::SUCCESS,
        Err(_) => NativeRuntimeStatus::RUNTIME_FAILURE,
    }
}

fn write_error_details(
    writer: &mut dyn Write,
    kind: &str,
    identity: NativeTypeIdentity,
    source: NativeSourceAnchor,
) -> std::io::Result<()> {
    write!(writer, "{kind} type=")?;

    for byte in identity.bytes() {
        write!(writer, "{byte:02x}")?;
    }

    if source.is_available() {
        writeln!(
            writer,
            " source={} start={} end={} version={}",
            source.source(),
            source.start(),
            source.end(),
            source.version()
        )
    } else {
        writeln!(writer, " source=unavailable")
    }
}

pub(super) fn finish_synchronous_incidents(
    terminal: &NativeTerminalState,
    outcome: NativeRunOutcome,
) -> NativeRunOutcome {
    let mut next_ordinal = 0;
    let mut outcome = outcome;

    loop {
        outcome = terminal.resolve_cleanup_outcome(outcome);

        let incidents = terminal.take_cleanup_incidents();

        if incidents.is_empty() {
            return outcome;
        }

        let mut failed = false;

        for incident in incidents.into_iter().rev() {
            let incident = CleanupIncident::next(
                &mut next_ordinal,
                CleanupIncidentProducer::SynchronousRoot,
                CleanupIncidentOrigin::SynchronousRoot,
                incident,
            );

            failed |= !report_cleanup_incident(incident).is_success();
        }

        if failed {
            super::host::record_cleanup_failure(1);
        }
    }
}

pub(super) fn report_cleanup_incidents(reports: &CleanupReportSink) -> NativeRuntimeStatus {
    let mut status = NativeRuntimeStatus::SUCCESS;

    reports.drain(|incident| {
        if !report_cleanup_incident(incident).is_success() {
            status = NativeRuntimeStatus::RUNTIME_FAILURE;
        }
    });

    status
}

fn report_cleanup_incident(incident: CleanupIncident) -> NativeRuntimeStatus {
    let report_failed =
        super::state::write_cleanup_incident_report(&mut std::io::stderr().lock(), &incident)
            .is_err();

    let payload_failed = incident.report_native_payload();

    if report_failed || payload_failed {
        NativeRuntimeStatus::RUNTIME_FAILURE
    } else {
        NativeRuntimeStatus::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use bray_runtime_abi::{
        NativeBrayCallOutcome, NativeCleanupIncident, NativePanicReportCallbacks, NativeRunOutcome,
        NativeRunState, NativeRuntimeStatus, NativeSourceAnchor, NativeTypeIdentity,
    };

    use super::{bray_runtime_cleanup_incident_transfer, with_incident_owner};
    use crate::native::frame::NativeTerminalState;

    #[derive(Debug, Eq, PartialEq)]
    enum Event {
        Report(usize),
        Destroy(usize),
        Attach(usize, usize),
    }

    thread_local! {
        static EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
        static REPORT_PAYLOADS: RefCell<Vec<NativeCleanupIncident>> = const { RefCell::new(Vec::new()) };
    }

    extern "C-unwind" fn report(value: usize) -> NativeRuntimeStatus {
        assert!(bray_platform::current_runtime_thread().is_some());
        EVENTS.with(|events| events.borrow_mut().push(Event::Report(value)));

        NativeRuntimeStatus::SUCCESS
    }

    extern "C-unwind" fn destroy(value: usize) -> NativeBrayCallOutcome {
        EVENTS.with(|events| events.borrow_mut().push(Event::Destroy(value)));

        NativeBrayCallOutcome::completed()
    }

    extern "C" fn construct(incident: &NativeCleanupIncident) -> usize {
        REPORT_PAYLOADS.with(|payloads| payloads.borrow_mut().push(*incident));

        incident.payload()
    }

    extern "C" fn suppress(primary: usize, secondary: usize) -> usize {
        EVENTS.with(|events| events.borrow_mut().push(Event::Attach(primary, secondary)));

        primary
    }

    extern "C-unwind" fn destroy_report(_: usize) -> NativeRuntimeStatus {
        let payloads = REPORT_PAYLOADS.with(|payloads| std::mem::take(&mut *payloads.borrow_mut()));

        for incident in payloads {
            (incident.destroy())(incident.payload());
        }

        NativeRuntimeStatus::SUCCESS
    }

    extern "C-unwind" fn report_error(incident: &NativeCleanupIncident) -> NativeRuntimeStatus {
        report(incident.payload())
    }

    fn incident(value: usize) -> NativeCleanupIncident {
        NativeCleanupIncident::new(
            value,
            NativeTypeIdentity::new([7; 32]),
            NativeSourceAnchor::unavailable(),
            report_error,
            destroy,
            NativePanicReportCallbacks::new(report, destroy_report, construct, suppress),
        )
    }

    fn events() -> Vec<Event> {
        EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()))
    }

    #[test]
    fn fallback_reporting_reads_metadata_without_observing_payload_memory() {
        for source in [
            NativeSourceAnchor::unavailable(),
            NativeSourceAnchor::new(9, 2, 13, 17),
        ] {
            let error = NativeCleanupIncident::new(
                usize::MAX,
                NativeTypeIdentity::new([0xab; 32]),
                source,
                report_error,
                destroy,
                incident(7).panics(),
            );

            let mut output = Vec::new();

            super::write_error_details(
                &mut output,
                "cleanup_error",
                error.type_identity(),
                error.source(),
            )
            .unwrap();

            let source = if source.is_available() {
                "9 start=2 end=13 version=17"
            } else {
                "unavailable"
            };

            assert_eq!(
                String::from_utf8(output).unwrap(),
                format!("cleanup_error type={} source={source}\n", "ab".repeat(32))
            );

            assert!(
                events().is_empty(),
                "metadata reporting must not invoke payload callbacks"
            );
        }

        assert_eq!(
            super::bray_runtime_cleanup_incident_detail_reporting(
                &crate::product::empty_native_incident()
            ),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );
    }

    #[test]
    fn rejected_transfers_destroy_valid_payloads_before_returning() {
        let error = incident(7);

        assert_eq!(
            bray_runtime_cleanup_incident_transfer(&error),
            NativeRuntimeStatus::NOT_INITIALIZED
        );

        assert_eq!(events(), [Event::Destroy(7)]);

        let terminal = NativeTerminalState::new();

        with_incident_owner(&terminal, || {
            let invalid_source = NativeCleanupIncident::new(
                11,
                NativeTypeIdentity::new([7; 32]),
                NativeSourceAnchor::new(9, 12, 3, 1),
                report_error,
                destroy,
                NativePanicReportCallbacks::new(report, destroy_report, construct, suppress),
            );

            for invalid in [incident(0), invalid_source] {
                assert!(crate::incident::OwnedCleanupIncident::native(invalid).is_none());

                assert_eq!(
                    bray_runtime_cleanup_incident_transfer(&invalid),
                    NativeRuntimeStatus::INVALID_ARGUMENT
                );

                assert!(terminal.take_cleanup_incidents().is_empty());
                assert!(events().is_empty());
            }
        });

        assert!(events().is_empty());
    }

    #[test]
    fn rejected_payload_destruction_can_reenter_incident_ownership() {
        extern "C-unwind" fn reenter(value: usize) -> NativeBrayCallOutcome {
            EVENTS.with(|events| events.borrow_mut().push(Event::Destroy(value)));

            let terminal = NativeTerminalState::new();

            with_incident_owner(&terminal, || {
                assert_eq!(
                    bray_runtime_cleanup_incident_transfer(&incident(13)),
                    NativeRuntimeStatus::SUCCESS
                );
            });

            drop(terminal.take_cleanup_incidents());

            NativeBrayCallOutcome::completed()
        }

        let error = NativeCleanupIncident::new(
            7,
            NativeTypeIdentity::new([7; 32]),
            NativeSourceAnchor::unavailable(),
            report_error,
            reenter,
            NativePanicReportCallbacks::new(report, destroy_report, construct, suppress),
        );

        assert_eq!(
            bray_runtime_cleanup_incident_transfer(&error),
            NativeRuntimeStatus::NOT_INITIALIZED
        );

        assert_eq!(events(), [Event::Destroy(7), Event::Destroy(13)]);
    }

    #[test]
    fn unwinding_stack_owned_cleanup_restores_the_parent_before_payload_destruction() {
        extern "C-unwind" fn reenter(value: usize) -> NativeBrayCallOutcome {
            EVENTS.with(|events| events.borrow_mut().push(Event::Destroy(value)));

            assert_eq!(
                bray_runtime_cleanup_incident_transfer(&incident(13)),
                NativeRuntimeStatus::SUCCESS
            );

            NativeBrayCallOutcome::completed()
        }

        let outer = NativeTerminalState::new();

        with_incident_owner(&outer, || {
            let empty = crate::test_support::with_allocation_failure(|| {
                super::with_cleanup_incident_owner(|_| 42)
            });

            assert_eq!(empty.0, 42);
            assert!(empty.1.is_empty());

            let unwind = std::panic::catch_unwind(|| {
                super::with_cleanup_incident_owner(|_| {
                    let error = NativeCleanupIncident::new(
                        7,
                        NativeTypeIdentity::new([7; 32]),
                        NativeSourceAnchor::unavailable(),
                        report_error,
                        reenter,
                        NativePanicReportCallbacks::new(
                            report,
                            destroy_report,
                            construct,
                            suppress,
                        ),
                    );

                    assert_eq!(
                        bray_runtime_cleanup_incident_transfer(&error),
                        NativeRuntimeStatus::SUCCESS
                    );

                    panic!("cleanup unwinds with an owned incident");
                });
            });

            assert!(unwind.is_err());
            assert_eq!(events(), [Event::Destroy(7)]);
        });

        let incidents = outer.take_cleanup_incidents();
        assert_eq!(incidents.len(), 1);
        drop(incidents);
        assert_eq!(events(), [Event::Destroy(13)]);
    }

    #[test]
    fn scoped_cleanup_returns_transfers_without_stealing_the_enclosing_owner() {
        let outer = NativeTerminalState::new();

        with_incident_owner(&outer, || {
            assert_eq!(
                bray_runtime_cleanup_incident_transfer(&incident(2)),
                NativeRuntimeStatus::SUCCESS
            );

            let (result, incidents) = super::with_cleanup_incident_owner(|record| {
                assert_eq!(
                    bray_runtime_cleanup_incident_transfer(&incident(3)),
                    NativeRuntimeStatus::SUCCESS
                );

                record(crate::incident::OwnedCleanupIncident::native(incident(4)).unwrap());

                assert_eq!(
                    bray_runtime_cleanup_incident_transfer(&incident(5)),
                    NativeRuntimeStatus::SUCCESS
                );

                17
            });

            assert_eq!(result, 17);
            assert_eq!(incidents.len(), 3);
            assert!(events().is_empty());

            assert_eq!(
                bray_runtime_cleanup_incident_transfer(&incident(7)),
                NativeRuntimeStatus::SUCCESS
            );

            drop(incidents);

            assert_eq!(
                events(),
                [Event::Destroy(3), Event::Destroy(4), Event::Destroy(5)]
            );
        });

        drop(outer.take_cleanup_incidents());

        assert_eq!(events(), [Event::Destroy(2), Event::Destroy(7)]);
    }

    #[test]
    fn nested_owners_restore_after_unwind_and_do_not_take_each_others_errors() {
        let outer = NativeTerminalState::new();
        let inner = NativeTerminalState::new();

        with_incident_owner(&outer, || {
            assert_eq!(
                bray_runtime_cleanup_incident_transfer(&incident(2)),
                NativeRuntimeStatus::SUCCESS
            );

            let failed = std::panic::catch_unwind(|| {
                with_incident_owner(&inner, || {
                    assert_eq!(
                        bray_runtime_cleanup_incident_transfer(&incident(3)),
                        NativeRuntimeStatus::SUCCESS
                    );

                    panic!("nested callback failed");
                })
            });

            assert!(failed.is_err());

            assert_eq!(
                bray_runtime_cleanup_incident_transfer(&incident(5)),
                NativeRuntimeStatus::SUCCESS
            );
        });

        assert!(events().is_empty());
        drop(inner.take_cleanup_incidents());
        assert_eq!(events(), [Event::Destroy(3)]);
        drop(outer.take_cleanup_incidents());
        assert_eq!(events(), [Event::Destroy(2), Event::Destroy(5)]);
        let rejected = incident(11);

        assert_eq!(
            bray_runtime_cleanup_incident_transfer(&rejected),
            NativeRuntimeStatus::NOT_INITIALIZED
        );

        assert_eq!(events(), [Event::Destroy(11)]);
    }

    extern "C" fn transfer_errors(_: usize, outcome: &mut NativeRunOutcome) {
        assert_eq!(
            bray_runtime_cleanup_incident_transfer(&incident(7)),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_cleanup_incident_transfer(&incident(11)),
            NativeRuntimeStatus::SUCCESS
        );

        *outcome = NativeRunOutcome::new(NativeRunState::CANCELLED, 0);
    }

    extern "C" fn panic_after_errors(value: usize, outcome: &mut NativeRunOutcome) {
        transfer_errors(value, outcome);
        *outcome = NativeRunOutcome::new(NativeRunState::PANICKED, 64);
    }

    extern "C" fn cleanup() {}

    #[test]
    fn synchronous_cancellation_reports_errors_in_reverse_order_before_destroying_them() {
        let outcome =
            crate::native::implementation::bray_runtime_substrate_foreign_callback_execution(
                transfer_errors,
                0,
                cleanup,
            );

        assert_eq!(outcome, NativeRunOutcome::new(NativeRunState::CANCELLED, 0));

        assert_eq!(
            events(),
            [
                Event::Report(11),
                Event::Destroy(11),
                Event::Report(7),
                Event::Destroy(7)
            ]
        );
    }

    #[test]
    fn synchronous_panics_transfer_errors_to_the_returned_report_owner() {
        let outcome =
            crate::native::implementation::bray_runtime_substrate_foreign_callback_execution(
                panic_after_errors,
                0,
                cleanup,
            );

        assert_eq!(outcome, NativeRunOutcome::new(NativeRunState::PANICKED, 64));
        assert_eq!(events(), [Event::Attach(64, 11), Event::Attach(64, 7)]);

        assert_eq!(
            destroy_report(outcome.payload()),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(events(), [Event::Destroy(11), Event::Destroy(7)]);
    }

    #[test]
    fn first_cleanup_panic_becomes_primary_and_owns_other_incidents() {
        for state in [
            NativeRunState::COMPLETED,
            NativeRunState::CANCELLED,
            NativeRunState::RUNTIME_FAILURE,
            NativeRunState::PANICKED,
        ] {
            let terminal = NativeTerminalState::new();

            with_incident_owner(&terminal, || {
                let callbacks = incident(7).panics();

                terminal.record_cleanup_incident(
                    crate::incident::OwnedCleanupIncident::native(incident(7)).unwrap(),
                );

                for payload in [32, 48] {
                    assert!(
                        super::retain_cleanup_incident(
                            crate::incident::OwnedCleanupIncident::boundary(
                                NativeBrayCallOutcome::panicked(payload).unwrap(),
                                callbacks
                            )
                            .unwrap()
                        )
                        .is_ok()
                    );
                }

                let outcome = terminal.resolve_cleanup_outcome(NativeRunOutcome::new(state, 64));

                let primary = if state == NativeRunState::PANICKED {
                    64
                } else {
                    32
                };

                assert_eq!(
                    outcome,
                    NativeRunOutcome::new(NativeRunState::PANICKED, primary)
                );

                assert!(terminal.take_cleanup_incidents().is_empty());

                let expected = if state == NativeRunState::PANICKED {
                    vec![
                        Event::Attach(64, 48),
                        Event::Attach(64, 32),
                        Event::Attach(64, 7),
                    ]
                } else {
                    vec![Event::Attach(32, 48), Event::Attach(32, 7)]
                };

                assert_eq!(events(), expected);

                assert_eq!(
                    destroy_report(outcome.payload()),
                    NativeRuntimeStatus::SUCCESS
                );

                assert_eq!(events(), [Event::Destroy(7)]);
            });
        }
    }

    extern "C-unwind" fn transfer_during_reporting(
        error: &NativeCleanupIncident,
    ) -> NativeRuntimeStatus {
        assert_eq!(
            bray_runtime_cleanup_incident_transfer(&incident(13)),
            NativeRuntimeStatus::SUCCESS
        );

        report(error.payload())
    }

    extern "C-unwind" fn transfer_during_destruction(value: usize) -> NativeBrayCallOutcome {
        assert_eq!(
            bray_runtime_cleanup_incident_transfer(&incident(17)),
            NativeRuntimeStatus::SUCCESS
        );

        destroy(value)
    }

    extern "C" fn transfer_reentrant_error(_: usize, outcome: &mut NativeRunOutcome) {
        let error = NativeCleanupIncident::new(
            7,
            NativeTypeIdentity::new([7; 32]),
            NativeSourceAnchor::unavailable(),
            transfer_during_reporting,
            transfer_during_destruction,
            incident(7).panics(),
        );

        assert_eq!(
            bray_runtime_cleanup_incident_transfer(&error),
            NativeRuntimeStatus::SUCCESS
        );

        *outcome = NativeRunOutcome::new(NativeRunState::CANCELLED, 0);
    }

    #[test]
    fn synchronous_reporting_drains_incidents_created_by_report_and_destroy_callbacks() {
        let outcome =
            crate::native::implementation::bray_runtime_substrate_foreign_callback_execution(
                transfer_reentrant_error,
                0,
                cleanup,
            );

        assert_eq!(outcome, NativeRunOutcome::new(NativeRunState::CANCELLED, 0));

        assert_eq!(
            events(),
            [
                Event::Report(7),
                Event::Destroy(7),
                Event::Report(17),
                Event::Destroy(17),
                Event::Report(13),
                Event::Destroy(13)
            ]
        );
    }
}
