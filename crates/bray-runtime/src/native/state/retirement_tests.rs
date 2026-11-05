use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::{
    NativeActivationDescriptor, NativeCleanupStorageShape, NativeFrameAffinity,
    NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind, NativeFrameState,
    NativeLaneRequirements, NativeRuntimeConfiguration, NativeRuntimeStatus,
};

use crate::native::{export, fixtures};

#[test]
fn thread_exit_retires_its_source_claims_with_or_without_explicit_shutdown() {
    let _isolation = super::test_runtime_isolation();

    for observe_first in [false, true] {
        for explicit_shutdown in [false, true] {
            let task = std::thread::spawn(move || {
                if observe_first {
                    assert!(super::retain_current_runtime().expect("Source lookup succeeds").is_none());
                }

                assert!(super::initialize(NativeRuntimeConfiguration::new(4, 4)).is_success());

                let task = export::bray_runtime_task_allocation().task().expect("the Source admission succeeds");

                assert_eq!(fixtures::snapshot(&fixtures::task(task)).phase, 0);

                if explicit_shutdown {
                    assert!(super::shutdown().is_success());
                }

                task
            }).join().expect("the exact-thread owner finishes retirement");

            assert!(crate::event::NativeCallback::task_owner(task).is_none(), "retirement releases unstarted Source claims and native callback owners");
        }
    }
}

#[test]
fn callback_isolation_allows_nested_runtime_creation() {
    let isolation = super::test_runtime_isolation();

    assert!(isolation.is_some());
    assert!(super::test_runtime_isolation().is_none());

    let retained = super::retain_runtime().expect("a standalone Source execution forms");

    retained.release();
    assert!(super::test_runtime_isolation().is_none());
    drop(isolation);
    assert!(super::test_runtime_isolation().is_some());
}

#[test]
fn independent_entry_restores_the_complete_source_context_after_unwind() {
    extern "C" fn requested(_: usize) -> u32 { 1 }

    extern "C-unwind" fn resume(output: &mut NativeFrameProgress, _: usize) {
        let owner = crate::current_task_execution_context().expect("Source installs the actual admitted context");
        let lane = crate::context::current_task_execution_lane().expect("Source retains its selected lane");

        crate::context::with_native_thread_cancellation(requested, 0, || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                crate::context::with_independent_execution_context(|| {
                    assert!(crate::current_task_execution_context().is_none());
                    assert!(!crate::current_run_cancellation_requested());
                    panic!("exercise complete context restoration");
                });
            }));

            assert!(result.is_err());

            let restored = crate::current_task_execution_context().expect("the same Source owner is restored");

            assert_eq!(restored.task(), owner.task());
            assert_eq!(restored.state(), owner.state());
            assert_eq!(crate::context::current_task_execution_lane(), Some(lane));
            assert!(crate::current_run_cancellation_requested());
        });

        *output = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
    }

    let _isolation = super::test_runtime_isolation();

    assert!(fixtures::startup(NativeRuntimeConfiguration::new(1, 1)).is_success());

    let task = export::bray_runtime_task_allocation().task().expect("the Source admission succeeds");
    let frame = fixtures::admit(0, NativeActivationDescriptor {
        storage: NativeCleanupStorageShape::new([61; 32], 8, 8),
        header: 0,
        completion: 0,
        completion_bytes: 0,
        completion_align: 1,
        states: 1,
        state,
        resume,
        cancel: resume,
        broadcast,
        resolve,
        move_completion: ignore_completion,
        destroy: ignore,
    });
    let mut rejected = bray_runtime_abi::NativeRunOutcome::new(bray_runtime_abi::NativeRunState::COMPLETED, 0);

    assert!(export::bray_runtime_task_start(task, std::ptr::from_ref(&frame).addr(), &mut rejected).is_success());
    assert!(export::bray_runtime_main_thread_lane_drive().is_success());
    assert_eq!(fixtures::observe(task).state(), bray_runtime_abi::NativeRunState::COMPLETED);
    assert!(fixtures::complete(task).is_success());
    assert!(super::shutdown().is_success());
}

#[test]
fn cleanup_binding_restores_an_unrelated_execution_owner() {
    let _isolation = super::test_runtime_isolation();
    let retained = std::thread::spawn(|| {
        assert!(super::initialize(NativeRuntimeConfiguration::new(2, 1)).is_success());

        super::retain_current_runtime().expect("the execution is open").expect("the actual owner is retained")
    }).join().expect("the Source owner transfers between native threads");

    assert!(fixtures::startup(NativeRuntimeConfiguration::new(2, 1)).is_success());

    let outer = export::bray_runtime_task_allocation().task().expect("the outer execution admits its task");
    let (inner, status) = super::with_cleanup_runtime(&retained, || {
        export::bray_runtime_task_allocation().task().expect("the borrowed execution admits its own task")
    }).expect("the complete owner binds for cleanup");

    assert!(status.is_success());
    assert_eq!(export::bray_runtime_task_destruction(inner), NativeRuntimeStatus::INVALID_ARGUMENT, "a task from another execution cannot be resolved through the restored outer binding");
    assert!(export::bray_runtime_task_destruction(outer).is_success());

    let (destroyed, status) = super::with_cleanup_runtime(&retained, || export::bray_runtime_task_destruction(inner))
        .expect("the same retained execution can resolve its source claim");

    assert!(destroyed.is_success());
    assert!(status.is_success());
    retained.release();
    assert!(super::shutdown().is_success());
}

#[test]
fn cloned_service_claims_keep_admission_open_until_the_last_owner_retires() {
    let _isolation = super::test_runtime_isolation();
    let retained = super::retain_runtime().expect("standalone execution retention succeeds");
    let remaining = retained.clone();

    retained.release();

    let (task, status) = super::with_cleanup_runtime(&remaining, || {
        export::bray_runtime_task_allocation().task().expect("an independent service owner keeps admission open")
    }).expect("the remaining owner supplies the cleanup context");

    assert!(status.is_success());
    assert!(crate::event::NativeCallback::task_owner(task).is_some());
    remaining.release();
    assert!(crate::event::NativeCallback::task_owner(task).is_none(), "the last owner retires its remaining unstarted admissions");
    assert!(super::retain_current_runtime().expect("lookup succeeds after retirement").is_none());
}

extern "C" fn state(_: usize, _: u32, _: u32, _: usize) -> NativeFrameState {
    NativeFrameState::new(NativeFrameAffinity::MOVABLE, NativeLaneRequirements::NONE)
}

extern "C-unwind" fn ignore(_: usize) {}
extern "C-unwind" fn ignore_completion(_: usize, _: usize) {}
extern "C-unwind" fn broadcast(_: &mut NativeFrameProgress, _: usize) {}
extern "C-unwind" fn resolve(output: &mut NativeFrameProgress, _: usize, _: NativeFrameExit) {
    *output = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
}
