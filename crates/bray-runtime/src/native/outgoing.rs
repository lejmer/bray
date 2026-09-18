use bray_runtime_abi::{NativePanicReport, NativeRunOutcome, NativeRunState};

use crate::RuntimePanic;
use crate::outgoing::OutgoingRecords;

pub(super) fn allocation_failure() -> NativeRunOutcome {
    NativeRunOutcome::panicked(crate::frame::native_report(
        bray_runtime_abi::NativePanicPrimary::new(
            bray_runtime_abi::NativePanicCause::ALLOCATION_FAILURE,
            bray_runtime_abi::NativeSourceAnchor::unavailable(),
            bray_runtime_abi::NativePanicMessage::empty(),
        ),
    ))
}

native_export! {
    pub extern "C" fn bray_runtime_outgoing_admission(count: usize, outcome: &mut NativeRunOutcome) {
        if OutgoingRecords::admit_source(count).is_err() {
            *outcome = allocation_failure();
        }
    }
}

native_export! {
    pub extern "C" fn bray_runtime_outgoing_discharge(count: usize) {
        OutgoingRecords::discharge_source(count);
    }
}

native_export! {
    pub extern "C" fn bray_runtime_outgoing_activation() -> usize {
        let (record, _, _) = OutgoingRecords::activate_source().into_parts();

        record
    }
}

native_export! {
    pub extern "C" fn bray_runtime_outgoing_retirement(record: usize, outcome: &mut NativeRunOutcome) {
        let reservation = OutgoingRecords::from_parts(record, record, 1);

        if outcome.state() == NativeRunState::PANICKED {
            let mut panic = RuntimePanic::from_native(outcome.take_report());

            panic.retain_reservation(reservation);

            *outcome = NativeRunOutcome::panicked(panic.into_native());
        }
    }
}

native_export! {
    pub extern "C" fn bray_runtime_panic_report_suppression(primary: &mut NativePanicReport, incident: &mut NativePanicReport) -> NativePanicReport {
        let mut primary = RuntimePanic::from_native(std::mem::replace(primary, NativePanicReport::empty()));
        let incident = RuntimePanic::from_native(std::mem::replace(incident, NativePanicReport::empty()));
        let mut unused = OutgoingRecords::default();

        primary.append(incident, &mut unused);
        primary.into_native()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        NativeRunOutcome, bray_runtime_outgoing_activation, bray_runtime_outgoing_admission,
        bray_runtime_outgoing_discharge, bray_runtime_outgoing_retirement,
        bray_runtime_panic_report_suppression,
    };
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

        bray_runtime_outgoing_admission(count, &mut outcome);

        outcome.state() == bray_runtime_abi::NativeRunState::COMPLETED
    }

    fn outcome(id: usize) -> NativeRunOutcome {
        NativeRunOutcome::panicked(crate::frame::native_report(NativePanicPrimary::new(
            NativePanicCause::ASSERTION,
            NativeSourceAnchor::new(1, 2, 3, 4),
            NativePanicMessage::new(id, 0, None, Some(release)),
        )))
    }

    #[test]
    fn admitted_records_follow_reports_after_owner_discharge_with_further_admission_denied() {
        RELEASES.with_borrow_mut(Vec::clear);

        assert!(admit(2));

        let first = bray_runtime_outgoing_activation();
        let second = bray_runtime_outgoing_activation();
        let failure = crate::outgoing::tests::reject_admission();

        assert!(!admit(1));

        let mut primary = outcome(1);
        let mut incident = outcome(2);

        bray_runtime_outgoing_retirement(first, &mut primary);
        bray_runtime_outgoing_retirement(second, &mut incident);
        bray_runtime_outgoing_discharge(2);

        assert!(!admit(1));

        let mut report = bray_runtime_panic_report_suppression(
            &mut primary.take_report(),
            &mut incident.take_report(),
        );

        assert!(RELEASES.with_borrow(Vec::is_empty));
        assert!(report.consume(false).is_success());
        assert!(report.consume(false).is_success());
        assert_eq!(RELEASES.with_borrow(Clone::clone), [1, 2]);

        drop(failure);
    }
}
