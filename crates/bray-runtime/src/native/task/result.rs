use std::cell::RefCell;

use bray_runtime_abi::{
    NativePanicReport, NativeRunOutcome, NativeRunResultLayout, NativeRunState, NativeRuntimeStatus,
};

thread_local! {
    static CLEANUP_EVENTS: RefCell<Vec<(&'static str, u8)>> = const { RefCell::new(Vec::new()) };
}

fn transfer_outcome(
    mut outcome: NativeRunOutcome,
    destination: usize,
    layout: NativeRunResultLayout,
) -> Result<(), NativeRuntimeStatus> {
    let status = crate::report_provider::bray_runtime_task_result_transfer(
        &mut outcome, std::ptr::with_exposed_provenance_mut(destination), &layout,
    );

    if status.is_success() {
        Ok(())
    } else {
        Err(status)
    }
}

#[test]
fn staged_completion_retains_its_payload_when_the_root_uses_the_same_slot() {
    let completed = 42_u64;
    let mut storage = ResultStorage([0xff; RESULT_SIZE]);

    storage.0[8..16].copy_from_slice(&completed.to_ne_bytes());

    let destination = storage.0.as_mut_ptr().addr();

    transfer_outcome(
        NativeRunOutcome::new(NativeRunState::COMPLETED, destination + 8),
        destination,
        result_layout(),
    )
    .expect("fixture `transfer_outcome` call must succeed");

    assert_eq!(storage.0[0], COMPLETED_TAG);
    assert_eq!(&storage.0[8..16], &completed.to_ne_bytes());
}

const COMPLETED_TAG: u8 = 3;
const PANICKED_TAG: u8 = 5;
const CANCELLED_TAG: u8 = 7;

const RESULT_SIZE: usize = size_of::<PanicDestination>();

#[repr(C)]
struct PanicDestination {
    tag: u64,
    report: NativePanicReport,
}

#[repr(C, align(8))]
struct ResultStorage([u8; RESULT_SIZE]);

#[test]
fn terminal_outcomes_form_their_selected_run_result_variants() {
    let completed = 42_u64;

    let cases = [
        (
            NativeRunOutcome::new(NativeRunState::COMPLETED, (&raw const completed).addr()),
            COMPLETED_TAG,
            completed,
        ),
        (
            NativeRunOutcome::panicked(bray_runtime_abi::NativePanicReport::empty()),
            PANICKED_TAG,
            0,
        ),
        (
            NativeRunOutcome::new(NativeRunState::CANCELLED, 0),
            CANCELLED_TAG,
            0,
        ),
    ];

    for (outcome, expected_tag, expected_payload) in cases {
        let mut storage = ResultStorage([0xff; RESULT_SIZE]);

        assert_eq!(
            transfer_outcome(outcome, storage.0.as_mut_ptr().addr(), result_layout()),
            Ok(())
        );

        assert_eq!(storage.0[0], expected_tag);

        assert_eq!(
            u64::from_ne_bytes(
                storage.0[8..16]
                    .try_into()
                    .unwrap_or_else(|_| panic!("payload must occupy eight bytes"))
            ),
            expected_payload
        );
    }
}

#[test]
fn transferred_panic_header_owns_its_message_until_destination_disposal() {
    use bray_runtime_abi::{
        NativePanicCause, NativePanicMessage, NativePanicPrimary, NativePanicReport,
        NativeSourceAnchor,
    };

    extern "C" fn release(_: usize, _: usize, _: &mut NativeRunOutcome) {
        CLEANUP_EVENTS.with_borrow_mut(|events| events.push(("report release", 1)));
    }

    CLEANUP_EVENTS.with_borrow_mut(Vec::clear);

    let report = crate::frame::native_report(NativePanicPrimary::new(
        NativePanicCause::MESSAGE,
        NativeSourceAnchor::unavailable(),
        NativePanicMessage::new(1, 0, None, Some(release)),
    ));

    let mut destination = PanicDestination {
        tag: 0,
        report: NativePanicReport::empty(),
    };

    transfer_outcome(
        NativeRunOutcome::panicked(report),
        std::ptr::from_mut(&mut destination).addr(),
        result_layout(),
    )
    .expect("fixture `transfer_outcome` call must succeed");

    assert_eq!(destination.tag, u64::from(PANICKED_TAG));

    CLEANUP_EVENTS.with_borrow(|events| assert!(events.is_empty()));

    assert!(destination.report.consume(false).is_success());

    drop(destination);

    CLEANUP_EVENTS.with_borrow(|events| assert_eq!(*events, [("report release", 1)]));
}

#[test]
fn unresolved_and_failed_outcomes_keep_their_runtime_status() {
    let mut storage = ResultStorage([0xff; RESULT_SIZE]);

    assert_eq!(
        transfer_outcome(
            NativeRunOutcome::new(NativeRunState::PENDING, 0),
            storage.0.as_mut_ptr().addr(),
            result_layout(),
        ),
        Err(NativeRuntimeStatus::PENDING)
    );

    assert_eq!(
        transfer_outcome(
            NativeRunOutcome::new(NativeRunState::RUNTIME_FAILURE, 0),
            storage.0.as_mut_ptr().addr(),
            result_layout(),
        ),
        Err(NativeRuntimeStatus::RUNTIME_FAILURE)
    );
}

fn result_layout() -> NativeRunResultLayout {
    NativeRunResultLayout::new(
        RESULT_SIZE,
        align_of::<PanicDestination>(),
        size_of::<u8>(),
        u64::from(COMPLETED_TAG),
        std::mem::offset_of!(PanicDestination, report),
        size_of::<u64>(),
        align_of::<u64>(),
        u64::from(PANICKED_TAG),
        std::mem::offset_of!(PanicDestination, report),
        u64::from(CANCELLED_TAG),
    )
}
