use bray_runtime_abi::NativeRunOutcome;

pub(super) fn allocation_failure() -> NativeRunOutcome {
    NativeRunOutcome::panicked(crate::frame::native_report(
        bray_runtime_abi::NativePanicPrimary::new(
            bray_runtime_abi::NativePanicCause::ALLOCATION_FAILURE,
            bray_runtime_abi::NativeSourceAnchor::unavailable(),
            bray_runtime_abi::NativePanicMessage::empty(),
        ),
    ))
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

        let first = bray_runtime_outgoing_activation();
        let second = bray_runtime_outgoing_activation();

        assert!(!admit(usize::MAX));

        let mut primary = outcome(1);
        let mut incident = outcome(2);

        bray_runtime_outgoing_retirement(first, &mut primary);
        bray_runtime_outgoing_retirement(second, &mut incident);
        bray_runtime_outgoing_discharge(2);

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
}
