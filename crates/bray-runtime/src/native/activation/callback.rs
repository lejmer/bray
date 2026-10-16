use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::{
    NativeActivationDescriptor, NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind,
    NativeReportRecords,
};

use crate::{RuntimePanic, outgoing::OutgoingRecords};

// The caller owns the result destination and admitted records throughout the
// callback. A native unwind must not discard an already published Bray report.
pub(in crate::native) fn invoke(
    destination: &mut NativeFrameProgress,
    outgoing: &mut OutgoingRecords,
    callback: impl FnOnce(&mut NativeFrameProgress),
) {
    let Err(payload) = catch_unwind(AssertUnwindSafe(|| callback(destination))) else {
        return;
    };

    let mut report = (destination.kind() == NativeFrameProgressKind::PANICKED)
        .then(|| RuntimePanic::from_native(destination.take_report()));

    RuntimePanic::record(&mut report, payload, outgoing);

    *destination = NativeFrameProgress::panicked(
        report
            .expect("a caught native unwind supplies its owned report")
            .into_native(),
    );
}

enum Operation {
    Resume,
    Cancel,
    Broadcast,
    Resolve(NativeFrameExit),
    Move(usize),
    Destroy,
}

/// Invokes a producer callback with exclusive destinations supplied by the Bray dispatcher.
/// The caller supplies the matching provider context and disjoint initialized
/// output slots, with an empty destination report and exclusively owned records.
/// Retaining operations keep their frame live. Destruction may release it, while
/// the descriptor and provider code remain live until the callback returns.
pub extern "C" fn bray_runtime_activation_callback(
    execution: &bray_runtime_abi::NativeActivationExecution,
    descriptor: &NativeActivationDescriptor,
    context: usize,
    operation: u32,
    argument: usize,
    destination: &mut NativeFrameProgress,
    records: &mut NativeReportRecords,
) {
    let selected = match operation {
        0 => Operation::Resume,
        1 => Operation::Cancel,
        2 => Operation::Broadcast,
        3 => Operation::Resolve(match argument {
            0 => NativeFrameExit::COMPLETED,
            1 => NativeFrameExit::CANCELLED,
            2 => NativeFrameExit::PANICKED,
            3 => NativeFrameExit::RUNTIME_FAILURE,
            _ => panic!("unknown activation lifecycle exit {argument}"),
        }),
        4 => Operation::Move(argument),
        5 => Operation::Destroy,
        _ => panic!("unknown activation callback operation {operation}"),
    };

    let initial = match selected {
        Operation::Resume | Operation::Cancel => NativeFrameProgressKind::RUNTIME_FAILURE,
        _ => NativeFrameProgressKind::COMPLETED,
    };

    let NativeReportRecords { head, tail, count } = std::mem::take(records);
    let mut outgoing = OutgoingRecords::from_parts(head, tail, count);

    *destination = NativeFrameProgress::new(initial, 0, 0);

    invoke(destination, &mut outgoing, |destination| {
        super::execution::with_execution(execution, || match selected {
            Operation::Resume => (descriptor.resume)(destination, context),
            Operation::Cancel => (descriptor.cancel)(destination, context),
            Operation::Broadcast => (descriptor.broadcast)(destination, context),
            Operation::Resolve(exit) => (descriptor.resolve)(destination, context, exit),
            Operation::Move(address) => (descriptor.move_completion)(context, address),
            Operation::Destroy => (descriptor.destroy)(context),
        });
    });

    let (head, tail, count) = outgoing.into_parts();

    *records = NativeReportRecords { head, tail, count };
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use crate::{RuntimePanic, outgoing::OutgoingRecords};
    use bray_runtime_abi::{
        NativeActivationDescriptor, NativeCleanupStorageShape, NativeFrameAffinity,
        NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind, NativeFrameState,
        NativeLaneRequirements, NativeReportRecords, NativeRuntimeStatus,
    };

    thread_local! {
        static CALLS: RefCell<Vec<(u32, usize, usize)>> = const { RefCell::new(Vec::new()) };
    }

    #[test]
    fn callbacks_keep_explicit_progress_and_do_not_admit_storage() {
        extern "C" fn state(_: usize, _: u32, _: u32, _: usize) -> NativeFrameState {
            NativeFrameState::new(NativeFrameAffinity::MOVABLE, NativeLaneRequirements::NONE)
        }

        extern "C-unwind" fn resume(destination: &mut NativeFrameProgress, context: usize) {
            assert_eq!(
                crate::current_task_execution_context()
                    .unwrap()
                    .state()
                    .raw(),
                0
            );

            let nested = bray_runtime_abi::NativeActivationExecution {
                address: 23,
                identity: [1; 32],
                state: 8,
                requirements: state(23, 8, 0, 0),
                origin: 0,
            };

            let nested_panic = std::panic::catch_unwind(|| {
                super::super::execution::with_execution(&nested, || {
                    assert_eq!(
                        crate::current_task_execution_context()
                            .unwrap()
                            .state()
                            .raw(),
                        8
                    );
                    panic!("nested callback unwind");
                });
            });

            assert!(nested_panic.is_err());
            assert_eq!(
                crate::current_task_execution_context()
                    .unwrap()
                    .state()
                    .raw(),
                0
            );
            CALLS.with_borrow_mut(|calls| calls.push((0, context, 0)));
            *destination = NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 7, 0);
        }

        extern "C-unwind" fn cancel(destination: &mut NativeFrameProgress, context: usize) {
            CALLS.with_borrow_mut(|calls| calls.push((1, context, 0)));
            *destination = NativeFrameProgress::new(NativeFrameProgressKind::CANCELLED, 0, 0);
        }

        extern "C-unwind" fn broadcast(_: &mut NativeFrameProgress, context: usize) {
            CALLS.with_borrow_mut(|calls| calls.push((2, context, 0)));
        }

        extern "C-unwind" fn resolve(
            destination: &mut NativeFrameProgress,
            context: usize,
            exit: NativeFrameExit,
        ) {
            CALLS.with_borrow_mut(|calls| {
                calls.push((3, context, usize::from(exit == NativeFrameExit::PANICKED)))
            });

            *destination = NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 9, 0);
        }

        extern "C-unwind" fn completion(context: usize, destination: usize) {
            CALLS.with_borrow_mut(|calls| calls.push((4, context, destination)));
        }

        extern "C-unwind" fn destroy(context: usize) {
            CALLS.with_borrow_mut(|calls| calls.push((5, context, 0)));
        }

        let execution = bray_runtime_abi::NativeActivationExecution {
            address: 17,
            identity: [0; 32],
            state: 0,
            requirements: state(17, 0, 0, 0),
            origin: 1,
        };

        CALLS.with_borrow_mut(Vec::clear);

        let descriptor = NativeActivationDescriptor {
            storage: NativeCleanupStorageShape::new([0; 32], 512, 8),
            header: 0,
            completion: 400,
            completion_bytes: 8,
            completion_align: 8,
            states: 10,
            state,
            resume,
            cancel,
            broadcast,
            resolve,
            move_completion: completion,
            destroy,
        };

        let _isolation = crate::native::test_runtime_isolation();
        let thread = crate::bootstrap::RuntimeThreadScope::enter().unwrap();

        let task = crate::TaskControlBlock::start(
            crate::test_support::admit_task(),
            crate::test_support::TestFrame::completing(1),
        );

        let scheduler = crate::Scheduler::new(
            [bray_runtime_model::RuntimeCapability::CooperativeExecution],
            thread.runtime().id(),
            crate::SchedulerLimits::new(
                std::num::NonZeroUsize::new(1).unwrap(),
                std::num::NonZeroUsize::new(1).unwrap(),
            ),
        );

        let registration =
            crate::test_support::register_task(&scheduler, &task, thread.runtime().id());
        let lane = registration.lane().unwrap();

        let native_context = crate::TaskExecutionContext::new(
            task.id(),
            bray_runtime_model::ProtectedFrameStateId::new(99),
            task.cancellation_context().clone(),
            task.output_context().clone(),
            lane,
            registration.wake_handle(),
        );

        let mut records = NativeReportRecords::default();
        let _denied = crate::outgoing::tests::reject_admission();
        let mut observed = Vec::new();

        crate::context::with_task_execution_context(native_context, || {
            for (operation, argument) in [(0, 0), (1, 0), (2, 0), (3, 2), (4, 37), (5, 0)] {
                let mut progress =
                    NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);

                super::bray_runtime_activation_callback(
                    &execution,
                    &descriptor,
                    17,
                    operation,
                    argument,
                    &mut progress,
                    &mut records,
                );

                assert_eq!(
                    crate::current_task_execution_context()
                        .unwrap()
                        .state()
                        .raw(),
                    99,
                );

                assert_eq!(crate::context::current_task_execution_lane(), Some(lane));
                observed.push((progress.kind(), progress.state()));
            }
        });

        assert_eq!(
            observed,
            [
                (NativeFrameProgressKind::SUSPENDED, 7),
                (NativeFrameProgressKind::CANCELLED, 0),
                (NativeFrameProgressKind::COMPLETED, 0),
                (NativeFrameProgressKind::SUSPENDED, 9),
                (NativeFrameProgressKind::COMPLETED, 0),
                (NativeFrameProgressKind::COMPLETED, 0),
            ]
        );

        CALLS.with_borrow(|calls| {
            assert_eq!(
                calls.as_slice(),
                [
                    (0, 17, 0),
                    (1, 17, 0),
                    (2, 17, 0),
                    (3, 17, 1),
                    (4, 17, 37),
                    (5, 17, 0)
                ]
            )
        });

        assert_eq!((records.head, records.tail, records.count), (0, 0, 0));
    }

    #[test]
    fn callback_unwind_keeps_a_previously_published_report() {
        struct Payload(Arc<AtomicUsize>);

        impl Drop for Payload {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let released = Arc::new(AtomicUsize::new(0));
        let mut outgoing = OutgoingRecords::admit(2).unwrap();
        let first = RuntimePanic::new(Payload(Arc::clone(&released)), &mut outgoing).into_native();
        let mut progress = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
        let _denied = crate::outgoing::tests::reject_admission();

        super::invoke(&mut progress, &mut outgoing, |destination| {
            *destination = NativeFrameProgress::panicked(first);
            std::panic::panic_any(Payload(Arc::clone(&released)));
        });

        assert_eq!(progress.kind(), NativeFrameProgressKind::PANICKED);
        assert_eq!(released.load(Ordering::SeqCst), 0);

        let mut report = RuntimePanic::from_native(progress.take_report());

        assert_eq!(report.consume(false), NativeRuntimeStatus::SUCCESS);
        assert_eq!(released.load(Ordering::SeqCst), 2);
    }
}
