use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bray_runtime_abi::{
    NativeActivationDescriptor, NativeCleanupStorageShape, NativeFrameAffinity, NativeFrameExit,
    NativeFrameProgress, NativeFrameProgressKind, NativeFrameState, NativeInactiveFrame,
    NativeLaneRequirements, NativeReportRecords, NativeRunOutcome, NativeRunResultLayout,
    NativeRunState, NativeRuntimeStatus, NativeTaskHandle, NativeValueCleanupCallback,
};

use super::state::{runtime_failure, with_runtime};
use crate::RuntimePanic;

const TASK_OBSERVATION_FRAME_IDENTITY: [u8; 32] = *b"bray.task.observation.frame.v1\0\0";

// Each of the two cleanup callbacks may publish a report and then unwind.
const OUTGOING_RECORDS: usize = 4;

struct TaskObservation {
    backing: crate::backing::Backing,
    descriptor: NativeActivationDescriptor,
    result: usize,
    outgoing: Mutex<crate::outgoing::OutgoingRecords>,
    task: NativeTaskHandle,
    layout: NativeRunResultLayout,
    cancellation: Option<NativeValueCleanupCallback>,
    lifecycle: Option<NativeValueCleanupCallback>,
    request_cancellation: bool,
    requirements: NativeLaneRequirements,
    retired: AtomicBool,
    resolving: AtomicBool,
    completion: Mutex<Completion>,
}

enum Completion {
    Pending,
    Ready(NativeRunOutcome),
    Consumed,
}

impl TaskObservation {
    fn resume(&self) -> NativeFrameProgress {
        let progress = self.poll();

        if self.resolving.load(Ordering::Acquire) {
            if progress.kind() == NativeFrameProgressKind::COMPLETED {
                self.resolve_owned_task();
            } else if progress.kind() == NativeFrameProgressKind::SUSPENDED {
                return NativeFrameProgress::new(progress.kind(), 1, progress.payload());
            }
        }

        progress
    }

    fn poll(&self) -> NativeFrameProgress {
        let pending = matches!(
            *self
                .completion
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            Completion::Pending
        );

        if pending {
            if self.request_cancellation {
                let status = with_runtime(|runtime| runtime.request_cancellation(self.task))
                    .unwrap_or_else(|status| status);

                if !status.is_success() {
                    return failure_progress();
                }
            }

            let context = crate::current_task_execution_context()
                .expect("observation runs within its admitted task context");

            let wake = context.wake_handle().clone();

            let outcome = with_runtime(|runtime| {
                runtime.join(
                    self.task,
                    Arc::new(move || {
                        let _ = wake.wake();
                    }),
                )
            })
            .unwrap_or_else(runtime_failure);

            if outcome.state() == NativeRunState::PENDING {
                return NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 0, 0);
            }

            let outcome = match outcome.state() {
                NativeRunState::COMPLETED => {
                    transfer_outcome(outcome, self.result, self.layout)
                        .expect("the admitted observation slot matches its task result");

                    NativeRunOutcome::new(
                        NativeRunState::COMPLETED,
                        self.result
                            .checked_add(self.layout.completed_offset())
                            .expect("the completed payload lies within admitted storage"),
                    )
                }
                NativeRunState::PANICKED | NativeRunState::CANCELLED => outcome,
                _ => return failure_progress(),
            };

            *self
                .completion
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Completion::Ready(outcome);
        }

        if !self.retired.load(Ordering::Acquire) {
            match destroy_task(self.task) {
                Ok(()) => self.retired.store(true, Ordering::Release),
                Err(NativeRuntimeStatus::PENDING) => {
                    return NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 0, 0);
                }
                Err(status) => panic!("owned terminal task retirement failed: {status:?}"),
            }
        }

        NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
    }

    fn take_completion(&self) -> Option<NativeRunOutcome> {
        match std::mem::replace(
            &mut *self
                .completion
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            Completion::Consumed,
        ) {
            Completion::Ready(outcome) => Some(outcome),
            Completion::Consumed => None,
            Completion::Pending => panic!("task observation must be terminal before consumption"),
        }
    }

    fn move_completion(&self, destination: usize) {
        assert!(
            self.retired.load(Ordering::Acquire),
            "completion publication follows task retirement"
        );

        let outcome = self
            .take_completion()
            .expect("task observation completion can only move once");

        transfer_outcome(outcome, destination, self.layout)
            .unwrap_or_else(|_| panic!("task observation result transfer failed"));
    }

    fn resolve_owned_task(&self) {
        assert!(
            self.retired.load(Ordering::Acquire),
            "cleanup follows owned task retirement"
        );

        assert!(
            !matches!(
                *self
                    .completion
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
                Completion::Pending
            ),
            "the activation driver awaits task resolution before disposing its outcome"
        );

        let completion = self.take_completion();

        let mut incident: Option<RuntimePanic> = None;

        let mut outgoing = std::mem::take(
            &mut *self
                .outgoing
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );

        if let Some(outcome) = completion {
            match transfer_outcome(outcome, self.result, self.layout) {
                Ok(()) => {
                    for cleanup in [self.cancellation, self.lifecycle].into_iter().flatten() {
                        let mut outcome = NativeRunOutcome::new(NativeRunState::COMPLETED, 0);

                        if let Err(found) = catch_unwind(AssertUnwindSafe(|| {
                            cleanup(
                                std::ptr::with_exposed_provenance_mut(self.result),
                                &mut outcome,
                            );
                        })) {
                            let panic = RuntimePanic::from_payload(found, &mut outgoing);

                            super::outgoing::append_panic(&mut outcome, panic, &mut outgoing);
                        }

                        match outcome.state() {
                            NativeRunState::PANICKED => {
                                let mut report = RuntimePanic::from_native(outcome.take_report());

                                report.reserve_from(&mut outgoing);

                                if let Some(incident) = &mut incident {
                                    incident.append(report, &mut outgoing);
                                } else {
                                    incident = Some(report);
                                }
                            }
                            // Disposal already abandons this observation. Cancellation does not
                            // skip the remaining lifecycle operation or release its retained task.
                            NativeRunState::COMPLETED | NativeRunState::CANCELLED => {}
                            state => panic!(
                                "frame cleanup must return a terminal source outcome, got {state:?}"
                            ),
                        }
                    }
                }
                Err(status) => RuntimePanic::record(&mut incident, Box::new(status), &mut outgoing),
            }
        }

        if let Some(incident) = incident {
            resume_unwind(Box::new(incident));
        }
    }
}

pub(super) fn create(
    task: NativeTaskHandle,
    request_cancellation: bool,
    layout: NativeRunResultLayout,
    cancellation: Option<NativeValueCleanupCallback>,
    lifecycle: Option<NativeValueCleanupCallback>,
    requirements: u32,
) -> Option<NativeInactiveFrame> {
    if !layout.is_valid() {
        return None;
    }

    let mut header_bytes = 0;
    let mut header_alignment = 0;

    crate::report_provider::bray_runtime_activation_header_layout(
        &mut header_bytes,
        &mut header_alignment,
    );

    let header = std::alloc::Layout::from_size_align(header_bytes, header_alignment)
        .expect("Bray reports its valid concrete header layout");

    let payload = std::alloc::Layout::from_size_align(layout.size(), layout.alignment()).ok()?;

    let (storage, completion) = header.extend(payload).ok()?;

    let storage = storage.pad_to_align();

    let shape = NativeCleanupStorageShape::new(
        TASK_OBSERVATION_FRAME_IDENTITY,
        storage.size(),
        storage.align(),
    );

    let backing = crate::backing::Backing::admit_shape(shape).ok()?;

    let result = backing
        .address()
        .checked_add(completion)
        .expect("admitted layout contains the completion slot");

    let mut outgoing = crate::outgoing::OutgoingRecords::admit(OUTGOING_RECORDS + 4).ok()?;

    let (head, tail, count) = outgoing.take(4).into_parts();

    let mut callback_records = NativeReportRecords { head, tail, count };

    let context = Box::new(TaskObservation {
        backing,
        descriptor: NativeActivationDescriptor {
            storage: shape,
            header: 0,
            completion,
            completion_bytes: layout.size(),
            completion_align: layout.alignment(),
            states: 2,
            state,
            resume,
            cancel,
            broadcast: broadcast_tasks,
            resolve: resolve_lifecycle,
            move_completion,
            destroy,
        },
        result,
        outgoing: Mutex::new(outgoing),
        task,
        layout,
        cancellation,
        lifecycle,
        request_cancellation,
        requirements: NativeLaneRequirements::from_bits(requirements),
        retired: AtomicBool::new(false),
        resolving: AtomicBool::new(false),
        completion: Mutex::new(Completion::Pending),
    });

    let token = NativeInactiveFrame::new(context.backing.lease(), &context.descriptor);

    crate::report_provider::bray_runtime_activation_initialization(
        &token,
        (&*context as *const TaskObservation).addr(),
        &mut callback_records,
    );

    assert_eq!(
        crate::report_provider::bray_runtime_activation_publication(&token),
        0,
        "the initialized provider publishes its exact admitted lease"
    );

    let _ = Box::into_raw(context);

    Some(token)
}

extern "C" fn state(
    context: usize,
    state: u32,
    operation: u32,
    argument: usize,
) -> NativeFrameState {
    let requirements = if (operation == 3 && argument != 0) || (operation < 2 && state == 1) {
        observation(context).requirements
    } else {
        NativeLaneRequirements::NONE
    };

    let affinity = if requirements.bits() & NativeLaneRequirements::MAIN_THREAD.bits() != 0 {
        NativeFrameAffinity::MAIN_THREAD
    } else {
        NativeFrameAffinity::MOVABLE
    };

    NativeFrameState::new(affinity, requirements)
}

extern "C-unwind" fn resume(destination: &mut NativeFrameProgress, context: usize) {
    *destination = observation(context).resume();
}

extern "C-unwind" fn cancel(destination: &mut NativeFrameProgress, context: usize) {
    let observation = observation(context);
    let _ = with_runtime(|runtime| runtime.request_cancellation(observation.task));

    *destination = observation.resume();
}

extern "C-unwind" fn broadcast_tasks(_: &mut NativeFrameProgress, context: usize) {
    let observation = observation(context);
    let _ = with_runtime(|runtime| runtime.request_cancellation(observation.task));
}

extern "C-unwind" fn resolve_lifecycle(
    destination: &mut NativeFrameProgress,
    context: usize,
    exit: NativeFrameExit,
) {
    if exit == NativeFrameExit::COMPLETED {
        return;
    }

    let observation = observation(context);

    observation.resolving.store(true, Ordering::Release);

    if !observation.retired.load(Ordering::Acquire) {
        let _ = with_runtime(|runtime| runtime.request_cancellation(observation.task));
    }

    *destination = observation.resume();
}

extern "C-unwind" fn move_completion(context: usize, destination: usize) {
    observation(context).move_completion(destination);
}

#[expect(
    unsafe_code,
    reason = "protected-frame destruction consumes the exact transferred observation allocation"
)]
extern "C-unwind" fn destroy(context: usize) {
    let observation = unsafe { Box::from_raw(context as *mut TaskObservation) };

    assert!(
        observation.retired.load(Ordering::Acquire),
        "activation resolves the task before releasing its provider"
    );
}

#[expect(
    unsafe_code,
    reason = "native frame callbacks borrow the live transferred observation context"
)]
fn observation(context: usize) -> &'static TaskObservation {
    unsafe { &*(context as *const TaskObservation) }
}

fn destroy_task(task: NativeTaskHandle) -> Result<(), NativeRuntimeStatus> {
    let status = with_runtime(|runtime| {
        let status = runtime.retire_task(task);

        if status.is_success() {
            runtime.destroy_task(task)
        } else {
            status
        }
    })?;

    if status.is_success() {
        Ok(())
    } else {
        Err(status)
    }
}

#[expect(
    unsafe_code,
    reason = "the compiler-provided validated layout governs the exact terminal payload move"
)]
pub(super) fn transfer_outcome(
    mut outcome: NativeRunOutcome,
    destination: usize,
    layout: NativeRunResultLayout,
) -> Result<(), NativeRuntimeStatus> {
    if destination == 0 || !layout.is_valid() {
        return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
    }

    let retained_payload = outcome.state() == NativeRunState::COMPLETED
        && destination.checked_add(layout.completed_offset()) == Some(outcome.payload());

    let destination = destination as *mut u8;

    if !retained_payload {
        unsafe { destination.write_bytes(0, layout.size()) };
    }

    let report;

    let (tag, payload) = if outcome.state() == NativeRunState::COMPLETED {
        (
            layout.completed_tag(),
            Some((
                layout.completed_offset(),
                outcome.payload(),
                layout.completed_size(),
            )),
        )
    } else if outcome.state() == NativeRunState::PANICKED {
        // The existing payload copy transfers this header into the validated destination.
        report = std::mem::ManuallyDrop::new(outcome.take_report());

        (
            layout.panicked_tag(),
            Some((
                layout.panicked_offset(),
                std::ptr::from_ref(&*report).addr(),
                size_of::<bray_runtime_abi::NativePanicReport>(),
            )),
        )
    } else if outcome.state() == NativeRunState::CANCELLED {
        (layout.cancelled_tag(), None)
    } else if outcome.state() == NativeRunState::PENDING {
        return Err(NativeRuntimeStatus::PENDING);
    } else {
        return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
    };

    unsafe {
        if let Some((offset, source, size)) = payload
            && size != 0
            && !retained_payload
        {
            if source == 0 {
                return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
            }

            std::ptr::copy_nonoverlapping(source as *const u8, destination.add(offset), size);
        }

        match layout.tag_size() {
            1 => destination.cast::<u8>().write(tag as u8),
            2 => destination.cast::<u16>().write_unaligned(tag as u16),
            4 => destination.cast::<u32>().write_unaligned(tag as u32),
            8 => destination.cast::<u64>().write_unaligned(tag),
            _ => return Err(NativeRuntimeStatus::INVALID_ARGUMENT),
        }
    }

    Ok(())
}

fn failure_progress() -> NativeFrameProgress {
    NativeFrameProgress::new(NativeFrameProgressKind::RUNTIME_FAILURE, 0, 0)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use bray_runtime_abi::{
        NativePanicReport, NativeRunOutcome, NativeRunResultLayout, NativeRunState,
        NativeRuntimeStatus,
    };

    use super::{Completion, TaskObservation, transfer_outcome};

    thread_local! {
        static CLEANUP_EVENTS: RefCell<Vec<(&'static str, u8)>> = const { RefCell::new(Vec::new()) };
    }

    struct CleanupFailure(u8);

    impl Drop for CleanupFailure {
        fn drop(&mut self) {
            CLEANUP_EVENTS.with_borrow_mut(|events| events.push(("release", self.0)));
        }
    }

    extern "C-unwind" fn failing_cancellation(_: *mut u8, _: &mut NativeRunOutcome) {
        CLEANUP_EVENTS.with_borrow_mut(|events| events.push(("cleanup", 1)));
        panic_any(CleanupFailure(1));
    }

    extern "C-unwind" fn failing_lifecycle(_: *mut u8, _: &mut NativeRunOutcome) {
        CLEANUP_EVENTS.with_borrow_mut(|events| events.push(("cleanup", 2)));
        panic_any(CleanupFailure(2));
    }

    #[test]
    fn cleanup_retains_secondary_failures_until_the_original_is_disposed() {
        let completed = 42_u64;

        let observation = observation(
            &completed,
            Some(failing_cancellation),
            Some(failing_lifecycle),
        );

        let failure =
            catch_unwind(AssertUnwindSafe(|| observation.resolve_owned_task())).expect_err("failing cleanup must propagate its retained panic");

        let failure = failure.downcast::<crate::RuntimePanic>().expect("fixture `downcast` call must succeed");

        assert_eq!(failure.suppressed_count(), 1);
        assert!(observation.retired.load(Ordering::Acquire));

        CLEANUP_EVENTS.with_borrow(|events| assert_eq!(*events, [("cleanup", 1), ("cleanup", 2)]));

        drop(failure);
        observation.resolve_owned_task();

        CLEANUP_EVENTS.with_borrow(|events| {
            assert_eq!(
                *events,
                [
                    ("cleanup", 1),
                    ("cleanup", 2),
                    ("release", 1),
                    ("release", 2),
                ]
            )
        });
    }

    extern "C" fn release_written(context: usize, _: usize, _: &mut NativeRunOutcome) {
        CLEANUP_EVENTS
            .with_borrow_mut(|events| events.push(("release", u8::try_from(context).expect("fixture value must fit the requested representation"))));
    }

    extern "C-unwind" fn write_then_unwind(_: *mut u8, outcome: &mut NativeRunOutcome) {
        CLEANUP_EVENTS.with_borrow_mut(|events| events.push(("cleanup", 8)));

        *outcome = NativeRunOutcome::panicked(crate::frame::native_report(
            bray_runtime_abi::NativePanicPrimary::new(
                bray_runtime_abi::NativePanicCause::ASSERTION,
                bray_runtime_abi::NativeSourceAnchor::unavailable(),
                bray_runtime_abi::NativePanicMessage::new(8, 0, None, Some(release_written)),
            ),
        ));

        panic_any(CleanupFailure(1));
    }

    extern "C-unwind" fn cancel_cleanup(_: *mut u8, outcome: &mut NativeRunOutcome) {
        CLEANUP_EVENTS.with_borrow_mut(|events| events.push(("cleanup", 3)));
        *outcome = NativeRunOutcome::new(NativeRunState::CANCELLED, 0);
    }

    #[test]
    fn cleanup_preserves_written_report_before_unwind_without_further_admission() {
        let completed = 42_u64;
        let observation = observation(&completed, Some(write_then_unwind), Some(failing_lifecycle));
        let _denied = crate::outgoing::tests::reject_admission();

        let failure =
            catch_unwind(AssertUnwindSafe(|| observation.resolve_owned_task())).expect_err("cleanup unwind must preserve its written report");

        let failure = failure.downcast::<crate::RuntimePanic>().expect("fixture `downcast` call must succeed");

        assert_eq!(failure.suppressed_count(), 2);
        assert!(observation.retired.load(Ordering::Acquire));
        CLEANUP_EVENTS.with_borrow(|events| assert_eq!(*events, [("cleanup", 8), ("cleanup", 2)]));
        drop(failure);
        observation.resolve_owned_task();

        CLEANUP_EVENTS.with_borrow(|events| {
            assert_eq!(
                *events,
                [
                    ("cleanup", 8),
                    ("cleanup", 2),
                    ("release", 8),
                    ("release", 1),
                    ("release", 2),
                ]
            )
        });
    }

    #[test]
    fn cancelled_cleanup_does_not_skip_lifecycle_or_task_retirement() {
        let completed = 42_u64;
        let observation = observation(&completed, Some(cancel_cleanup), Some(failing_lifecycle));

        let failure =
            catch_unwind(AssertUnwindSafe(|| observation.resolve_owned_task())).expect_err("failing lifecycle must propagate after cancelled cleanup");

        let failure = failure.downcast::<crate::RuntimePanic>().expect("fixture `downcast` call must succeed");

        assert_eq!(failure.suppressed_count(), 0);
        assert!(observation.retired.load(Ordering::Acquire));
        drop(failure);

        CLEANUP_EVENTS.with_borrow(|events| {
            assert_eq!(*events, [("cleanup", 3), ("cleanup", 2), ("release", 2),])
        });
    }

    fn observation(
        completed: &u64,
        cancellation: Option<super::NativeValueCleanupCallback>,
        lifecycle: Option<super::NativeValueCleanupCallback>,
    ) -> TaskObservation {
        let layout = result_layout();
        let backing = crate::backing::Backing::admit(layout.size(), layout.alignment()).expect("fixture outgoing records must admit the supplied values");
        let result = backing.address();

        transfer_outcome(
            NativeRunOutcome::new(
                NativeRunState::COMPLETED,
                std::ptr::from_ref(completed).addr(),
            ),
            result,
            layout,
        )
        .expect("fixture `transfer_outcome` call must succeed");

        // These cleanup tests begin after native task retirement. Native integration
        // tests separately exercise admission, dispatch and retirement ownership.
        TaskObservation {
            backing,
            descriptor: bray_runtime_abi::NativeActivationDescriptor {
                storage: bray_runtime_abi::NativeCleanupStorageShape::new(
                    [0; 32],
                    layout.size(),
                    layout.alignment(),
                ),
                header: 0,
                completion: 0,
                completion_bytes: layout.size(),
                completion_align: layout.alignment(),
                states: 2,
                state: super::state,
                resume: super::resume,
                cancel: super::cancel,
                broadcast: super::broadcast_tasks,
                resolve: super::resolve_lifecycle,
                move_completion: super::move_completion,
                destroy: super::destroy,
            },
            result,
            outgoing: Mutex::new(
                crate::outgoing::OutgoingRecords::admit(super::OUTGOING_RECORDS).expect("fixture outgoing records must admit the supplied values"),
            ),
            task: bray_runtime_abi::NativeTaskHandle::new(1).expect("fixture native task handle must be valid"),
            layout,
            cancellation,
            lifecycle,
            request_cancellation: false,
            requirements: bray_runtime_abi::NativeLaneRequirements::BLOCKING,
            retired: AtomicBool::new(true),
            resolving: AtomicBool::new(false),
            completion: Mutex::new(Completion::Ready(NativeRunOutcome::new(
                NativeRunState::COMPLETED,
                result + layout.completed_offset(),
            ))),
        }
    }

    #[test]
    fn completion_requires_retirement_and_moves_without_fallible_cleanup() {
        let completed = 42_u64;

        let observation = observation(
            &completed,
            Some(failing_cancellation),
            Some(failing_lifecycle),
        );

        let mut storage = ResultStorage([0xff; RESULT_SIZE]);

        observation.retired.store(false, Ordering::Release);

        assert!(
            catch_unwind(AssertUnwindSafe(
                || observation.move_completion(storage.0.as_mut_ptr().addr())
            ))
            .is_err()
        );

        assert_eq!(storage.0, [0xff; RESULT_SIZE]);
        observation.retired.store(true, Ordering::Release);

        let _denied = crate::outgoing::tests::reject_admission();

        observation.move_completion(storage.0.as_mut_ptr().addr());
        observation.resolve_owned_task();
        CLEANUP_EVENTS.with_borrow(|events| assert!(events.is_empty()));
        assert_eq!(storage.0[0], COMPLETED_TAG);
        assert_eq!(&storage.0[8..16], &completed.to_ne_bytes());

        assert!(
            catch_unwind(AssertUnwindSafe(
                || observation.move_completion(storage.0.as_mut_ptr().addr())
            ))
            .is_err()
        );
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
}
