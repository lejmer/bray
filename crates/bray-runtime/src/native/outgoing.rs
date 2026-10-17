use bray_runtime_abi::{NativeRunOutcome, NativeRunState};

pub(super) fn allocation_failure() -> NativeRunOutcome {
    NativeRunOutcome::panicked(crate::frame::native_report(
        bray_runtime_abi::NativePanicPrimary::new(
            bray_runtime_abi::NativePanicCause::ALLOCATION_FAILURE,
            bray_runtime_abi::NativeSourceAnchor::unavailable(),
            bray_runtime_abi::NativePanicMessage::empty(),
        ),
    ))
}

/// Appends a later panic without replacing an already-published primary owner.
pub(super) fn append_panic(
    outcome: &mut NativeRunOutcome,
    panic: crate::RuntimePanic,
    admitted: &mut crate::outgoing::OutgoingRecords,
) {
    let report = if outcome.state() == NativeRunState::PANICKED {
        let mut report = crate::RuntimePanic::from_native(outcome.take_report());

        report.append(panic, admitted);

        report
    } else {
        panic
    };

    *outcome = NativeRunOutcome::panicked(report.into_native());
}

#[cfg(test)]
mod tests {
    use crate::report_provider::{
        bray_runtime_outgoing_activation, bray_runtime_outgoing_admission,
        bray_runtime_outgoing_discharge, bray_runtime_outgoing_retirement,
        bray_runtime_panic_report_suppression,
    };
    use bray_runtime_abi::NativeRunOutcome;
    use bray_runtime_abi::{
        NativePanicCause, NativePanicMessage, NativePanicPrimary, NativeSourceAnchor,
    };
    use std::cell::RefCell;

    thread_local! { static RELEASES: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) }; }

    extern "C" fn release(id: usize, _: usize, _: &mut NativeRunOutcome) {
        RELEASES.with_borrow_mut(|releases| releases.push(id));
    }

    fn admit(count: usize) -> bool {
        let mut outcome = NativeRunOutcome::new(bray_runtime_abi::NativeRunState::COMPLETED, 0);

        bray_runtime_outgoing_admission(
            &bray_runtime_abi::NativeCleanupStorageBundle::new(count, &[]),
            &mut outcome,
        );

        outcome.state() == bray_runtime_abi::NativeRunState::COMPLETED
    }

    fn outcome(id: usize) -> NativeRunOutcome {
        NativeRunOutcome::panicked(crate::frame::native_report(NativePanicPrimary::new(
            NativePanicCause::ASSERTION,
            NativeSourceAnchor::new([0; 32], 1, 2, 3, 4),
            NativePanicMessage::new(id, 0, None, Some(release)),
        )))
    }

    #[test]
    fn empty_report_consumes_repeatedly_without_admission() {
        let mut report = crate::frame::native_report(NativePanicPrimary::empty());

        assert!(report.consume(false).is_success());
        assert!(report.consume(false).is_success());
        assert_eq!(report.outgoing_count(), 0);
        assert!(!report.has_reservation());
    }

    #[test]
    fn admitted_records_follow_reports_after_owner_discharge_with_further_admission_denied() {
        RELEASES.with_borrow_mut(Vec::clear);

        assert!(admit(2));

        let first = bray_runtime_outgoing_activation(std::ptr::null(), std::ptr::null_mut());
        let second = bray_runtime_outgoing_activation(std::ptr::null(), std::ptr::null_mut());

        assert!(!admit(usize::MAX));

        let mut primary = outcome(1);
        let mut incident = outcome(2);

        bray_runtime_outgoing_retirement(first, std::ptr::null(), &mut primary);
        bray_runtime_outgoing_retirement(second, std::ptr::null(), &mut incident);
        bray_runtime_outgoing_discharge(&bray_runtime_abi::NativeCleanupStorageBundle::new(2, &[]));

        assert!(!admit(usize::MAX));

        let mut report = bray_runtime_panic_report_suppression(
            &mut primary.take_report(),
            &mut incident.take_report(),
        );

        assert!(RELEASES.with_borrow(Vec::is_empty));
        assert!(report.consume(false).is_success());
        assert!(report.consume(false).is_success());
        assert_eq!(RELEASES.with_borrow(Clone::clone), [1, 2]);
    }

    #[test]
    fn detached_report_chain_round_trips_without_new_admission_and_releases_once() {
        RELEASES.with_borrow_mut(Vec::clear);

        let mut admitted = crate::outgoing::OutgoingRecords::admit(2).expect("report admission");

        let mut records =
            crate::RuntimePanic::from_native(outcome(1).take_report()).into_records(&mut admitted);

        let mut later =
            crate::RuntimePanic::from_native(outcome(2).take_report()).into_records(&mut admitted);

        records.append(&mut later);
        assert_eq!(admitted.len(), 0);

        let report = crate::RuntimePanic::from_records(records);

        assert_eq!(report.suppressed_count(), 1);

        let records = report.into_records(&mut admitted);

        assert_eq!(records.len(), 2);
        assert!(RELEASES.with_borrow(Vec::is_empty));
        drop(records);
        assert_eq!(RELEASES.with_borrow(Clone::clone), [1, 2]);
    }
}
