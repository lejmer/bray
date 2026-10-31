use std::cell::Cell;
use std::sync::Arc;

use bray_runtime_abi::{
    NativeActivationDescriptor, NativeCleanupStorageShape, NativeFrameAffinity, NativeFrameExit,
    NativeFrameProgress, NativeFrameProgressKind, NativeFrameState, NativeLaneRequirements,
    NativeRuntimeConfiguration, NativeRuntimeStatus, NativeRunState,
};

use crate::native::{activation, bridge, fixtures, implementation, state};
use crate::{TaskExecutionContext, context};
use crate::report_provider;

use super::NativeTaskServices;

thread_local! {
    static CALLBACK_ADMISSION: Cell<Option<bray_runtime_abi::NativeTaskHandle>> = const { Cell::new(None) };
}

#[test]
fn cancellation_before_dispatch_is_idempotent_and_does_not_publish_completion() {
    startup();

    let task = start();

    assert!(task.cancellation.request());
    assert!(!task.cancellation.request());
    assert_eq!(task.snapshot().phase, 1);
    assert!(task.cancellation.observation().requested());

    finish(&task);
    assert!(!task.cancellation.request());
    assert!(!task.cancellation.is_requested());
    drop(task);
    assert_eq!(implementation::bray_runtime_execution_shutdown(), NativeRuntimeStatus::SUCCESS);
}

#[test]
fn cancellation_requested_by_a_running_callback_survives_dispatch_release() {
    extern "C-unwind" fn request(output: &mut NativeFrameProgress, context: usize) {
        let current = crate::current_task_execution_context().expect("producer has a task context");

        assert!(current.cancellation().request());
        assert!(!current.cancellation().request());
        suspend(output, context);
    }

    startup();

    let task = start_with_resume(request);

    assert_eq!(implementation::bray_runtime_main_thread_lane_drive(), NativeRuntimeStatus::SUCCESS);
    assert_eq!(task.snapshot().phase, 3);
    assert!(task.cancellation.is_requested());

    finish(&task);
    drop(task);
    assert_eq!(implementation::bray_runtime_execution_shutdown(), NativeRuntimeStatus::SUCCESS);
}

#[test]
fn nested_native_shields_preserve_requests_until_the_outer_leave() {
    startup();

    let task = start();

    assert_eq!(implementation::bray_runtime_main_thread_lane_drive(), NativeRuntimeStatus::SUCCESS);

    let outer = task.cancellation.shield();
    let inner = task.cancellation.shield();

    assert!(task.cancellation.request());
    assert!(task.cancellation.observation().requested());
    assert!(!task.cancellation.is_requested());
    assert_eq!(task.snapshot().phase, 3);
    assert_eq!(implementation::bray_runtime_main_thread_lane_drive(), NativeRuntimeStatus::PENDING);

    drop(inner);
    assert!(!task.cancellation.is_requested());
    assert_eq!(implementation::bray_runtime_main_thread_lane_drive(), NativeRuntimeStatus::PENDING);
    drop(outer);
    assert!(task.cancellation.is_requested());

    finish(&task);
    drop(task);
    assert_eq!(implementation::bray_runtime_execution_shutdown(), NativeRuntimeStatus::SUCCESS);
}

#[test]
fn terminal_delivery_between_registration_and_suspension_retains_the_wake() {
    extern "C-unwind" fn register_then_resolve(output: &mut NativeFrameProgress, context: usize) {
        if let Some(handle) = CALLBACK_ADMISSION.take() {
            let parent = state::current_services().expect("producer retains its parent services");

            let child = state::with_runtime(|runtime| runtime.retained_task(handle))
                .expect("producer retains its runtime").expect("fixture child remains live");

            assert_eq!(report_provider::bray_runtime_task_control_wait(child.control(), parent.control(), 0), NativeRuntimeStatus::PENDING);
            finish(&child);
        }

        suspend(output, context);
    }

    startup();

    let child = start();

    CALLBACK_ADMISSION.set(Some(child.handle()));

    let parent = start_with_resume(register_then_resolve);

    for _ in 0..2 {
        assert_eq!(implementation::bray_runtime_main_thread_lane_drive(), NativeRuntimeStatus::SUCCESS);
    }

    assert_eq!(parent.snapshot().phase, 3);
    assert!(!parent.cancellation.is_requested());
    assert_eq!(implementation::bray_runtime_main_thread_lane_drive(), NativeRuntimeStatus::SUCCESS);
    assert_eq!(implementation::bray_runtime_main_thread_lane_drive(), NativeRuntimeStatus::PENDING);

    finish(&parent);
    drop(parent);
    drop(child);
    assert_eq!(implementation::bray_runtime_execution_shutdown(), NativeRuntimeStatus::SUCCESS);
}

#[test]
fn committed_delivery_prevents_an_unpopped_waiter_from_withdrawing() {
    extern "C" fn reenter_dispatch(identity: u64) {
        let Some(waiter) = CALLBACK_ADMISSION.take() else { return; };

        assert_ne!(identity, waiter.raw());

        let parent = state::with_runtime(|runtime| runtime.retained_task(waiter))
            .expect("delivery retains its runtime").expect("the unpopped waiter remains live");

        // Another notification is already committed to this parent. Its
        // registration must survive this attempted resumption until its pop.
        let (status, _, _) = parent.dispatch();

        assert_eq!(status, NativeRuntimeStatus::PENDING.code());
        assert_eq!(parent.snapshot().phase, 3);
    }

    startup();

    let child = start();
    let parents = [start(), start()];

    for _ in 0..3 {
        assert_eq!(implementation::bray_runtime_main_thread_lane_drive(), NativeRuntimeStatus::SUCCESS);
    }

    for parent in &parents {
        assert_eq!(report_provider::bray_runtime_task_control_wait(child.control(), parent.control(), 0), NativeRuntimeStatus::PENDING);
    }

    CALLBACK_ADMISSION.set(Some(parents[0].handle()));
    assert!(child.cancellation.request());
    drive_with_wake(&child, reenter_dispatch);
    assert!(CALLBACK_ADMISSION.get().is_none());

    for parent in &parents { finish(parent); }
    finish(&child);
    drop(parents);
    drop(child);
    assert_eq!(implementation::bray_runtime_execution_shutdown(), NativeRuntimeStatus::SUCCESS);
}

#[test]
fn unobserved_retirement_does_not_claim_the_parents_wait_link() {
    startup();

    let child = start();
    let parent = start();

    for _ in 0..2 {
        assert_eq!(implementation::bray_runtime_main_thread_lane_drive(), NativeRuntimeStatus::SUCCESS);
    }

    assert_eq!(report_provider::bray_runtime_task_control_wait(child.control(), parent.control(), 1), NativeRuntimeStatus::PENDING);
    assert_eq!(report_provider::bray_runtime_task_control_wait(child.control(), parent.control(), 0), NativeRuntimeStatus::PENDING);

    finish(&child);
    finish(&parent);
    drop(child);
    drop(parent);
    assert_eq!(implementation::bray_runtime_execution_shutdown(), NativeRuntimeStatus::SUCCESS);
}

#[test]
fn resumption_withdraws_head_middle_and_tail_links_before_reusing_the_owner() {
    startup();

    let first = start();
    let second = start();
    let parents = [start(), start(), start(), start()];

    for _ in 0..6 {
        assert_eq!(implementation::bray_runtime_main_thread_lane_drive(), NativeRuntimeStatus::SUCCESS);
    }

    for parent in &parents {
        assert_eq!(report_provider::bray_runtime_task_control_wait(first.control(), parent.control(), 0), NativeRuntimeStatus::PENDING);
    }

    // Remove a middle link, the tail and the head. Every released owner can
    // immediately wait on a different child, while the remaining link delivers.
    for index in [2, 0, 3] {
        assert_eq!(implementation::bray_runtime_wake(parents[index].handle()), NativeRuntimeStatus::SUCCESS);
        assert_eq!(implementation::bray_runtime_main_thread_lane_drive(), NativeRuntimeStatus::SUCCESS);
        assert_eq!(report_provider::bray_runtime_task_control_wait(second.control(), parents[index].control(), 0), NativeRuntimeStatus::PENDING);
    }

    finish(&first);
    finish(&second);

    for parent in &parents { finish(parent); }
    drop(parents);
    drop(first);
    drop(second);
    assert_eq!(implementation::bray_runtime_execution_shutdown(), NativeRuntimeStatus::SUCCESS);
}

#[test]
fn committed_delivery_retains_the_cell_after_reentrant_source_resolution() {
    extern "C" fn resolve_during_wake(identity: u64) {
        let handle = bray_runtime_abi::NativeTaskHandle::new(identity).expect("waiter identity is live");

        let task = state::with_runtime(|runtime| runtime.retained_task(handle))
            .expect("callback retains its runtime").expect("registered parent remains live");

        finish(&task);
        drop(task);

        let replacement = state::with_runtime(|runtime| runtime.allocate().task())
            .expect("callback retains its runtime").expect("callback can admit another task");

        assert_ne!(replacement.raw() & 0xffffffff, identity & 0xffffffff);
        CALLBACK_ADMISSION.set(Some(replacement));
    }

    startup();
    CALLBACK_ADMISSION.set(None);

    let child = start();
    let parent = start();
    let parent_handle = parent.handle();

    for _ in 0..2 {
        assert_eq!(implementation::bray_runtime_main_thread_lane_drive(), NativeRuntimeStatus::SUCCESS);
    }

    assert_eq!(report_provider::bray_runtime_task_control_wait(child.control(), parent.control(), 0), NativeRuntimeStatus::PENDING);
    drop(parent);
    assert!(child.cancellation.request());
    drive_with_wake(&child, resolve_during_wake);

    state::with_runtime(|runtime| {
        let replacement = runtime.allocate().task().expect("returned delivery releases its last cell reference");

        assert_eq!(replacement.raw() & 0xffffffff, parent_handle.raw() & 0xffffffff);
        assert_ne!(replacement, parent_handle);
        assert_eq!(runtime.destroy_task(replacement), NativeRuntimeStatus::SUCCESS);

        let admitted = CALLBACK_ADMISSION.take().expect("callback records its temporary admission");

        assert_eq!(runtime.destroy_task(admitted), NativeRuntimeStatus::SUCCESS);
    }).expect("fixture runtime remains active");

    finish(&child);
    drop(child);
    assert_eq!(implementation::bray_runtime_execution_shutdown(), NativeRuntimeStatus::SUCCESS);
}

fn drive_with_wake(task: &Arc<NativeTaskServices>, wake: extern "C" fn(u64)) {
    state::with_runtime(|runtime| {
        let lane = task.registration.lane().expect("fixture has a main lane");

        let ready = runtime.scheduler.take_ready(lane).expect("ready queue is available")
            .expect("cancellation enqueues the fixture");

        assert_eq!(ready.task(), task.id());

        let context = TaskExecutionContext::new(task.id(), ready.state(), task.cancellation.clone(),
            &task.output, ready.lane(), task.registration.wake_handle());

        let _bridge = bridge::Scope::enter().expect("fixture owns its entry scope");

        let services = bray_runtime_abi::NativeTaskDispatchServices {
            invoke: activation::bray_runtime_activation_callback,
            ready: activation::readiness,
            transfer: super::dispatch::transfer,
            wake,
        };

        let mut progress = NativeFrameProgress::new(NativeFrameProgressKind::RUNTIME_FAILURE, 0, 0);
        let mut execution = bray_runtime_abi::NativeActivationExecution::default();

        let status = state::with_task_services(Arc::clone(task), || {
            context::with_task_execution_context(context, || {
                report_provider::bray_runtime_task_control_dispatch(task.control(), &services, &mut progress, &mut execution)
            })
        });

        assert_eq!(status, 1);
        assert_eq!(progress.kind(), NativeFrameProgressKind::CANCELLED);
        ready.suspend(activation::execution(&execution)).expect("dispatch publishes its terminal scheduling state");
    }).expect("fixture runtime remains active");
}

fn startup() {
    assert_eq!(implementation::bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(8, 1)), NativeRuntimeStatus::SUCCESS);
}

fn start() -> Arc<NativeTaskServices> {
    start_with_resume(suspend)
}

fn start_with_resume(resume: extern "C-unwind" fn(&mut NativeFrameProgress, usize)) -> Arc<NativeTaskServices> {
    let frame = fixtures::admit(0, NativeActivationDescriptor {
        storage: NativeCleanupStorageShape::new([91; 32], 8, 8),
        header: 0,
        completion: 0,
        completion_bytes: 0,
        completion_align: 1,
        states: 2,
        state: frame_state,
        resume,
        cancel,
        broadcast,
        resolve,
        move_completion,
        destroy,
    });

    state::with_runtime(|runtime| {
        let handle = runtime.allocate().task().expect("fixture task admission succeeds");

        assert_eq!(runtime.start(handle, &frame), NativeRuntimeStatus::SUCCESS);

        runtime.retained_task(handle).expect("started fixture remains registered")
    }).expect("fixture runtime is active")
}

fn finish(task: &NativeTaskServices) {
    task.cancellation.request();

    state::with_runtime(|runtime| {
        assert_eq!(runtime.resolve_task(task.handle()).state(), NativeRunState::CANCELLED);
        assert_eq!(runtime.resolve_task_retirement(task.handle()), NativeRuntimeStatus::SUCCESS);
        assert_eq!(runtime.destroy_task(task.handle()), NativeRuntimeStatus::SUCCESS);
    }).expect("fixture runtime is active through source resolution");
}

extern "C" fn frame_state(_: usize, _: u32, _: u32, _: usize) -> NativeFrameState {
    NativeFrameState::new(NativeFrameAffinity::MAIN_THREAD, NativeLaneRequirements::MAIN_THREAD)
}

extern "C-unwind" fn suspend(output: &mut NativeFrameProgress, _: usize) {
    *output = NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 1, 0);
}

extern "C-unwind" fn cancel(output: &mut NativeFrameProgress, _: usize) {
    *output = NativeFrameProgress::new(NativeFrameProgressKind::CANCELLED, 0, 0);
}

extern "C-unwind" fn broadcast(_: &mut NativeFrameProgress, _: usize) {}
extern "C-unwind" fn resolve(_: &mut NativeFrameProgress, _: usize, _: NativeFrameExit) {}
extern "C-unwind" fn move_completion(_: usize, _: usize) {}
extern "C-unwind" fn destroy(_: usize) {}
