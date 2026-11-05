use std::num::NonZeroUsize;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::rc::Rc;
    use std::sync::atomic::Ordering;

    use bray_runtime_abi::{NativeRuntimeConfiguration, NativeTaskHandle};
    use bray_runtime_model::{ProtectedFrameStateId, RuntimeCapability};

    use crate::test_support::TestFrame;
    use crate::{
        CancellationContext, ExecutionLane, ExecutionLanePlacement, ExecutionWorkload, Scheduler,
        SchedulerLimits, TaskControlBlock, TaskExecutionContext,
    };

    extern "C" fn cancellation_requested(_: usize) -> u32 {
        1
    }

    #[test]
    fn thread_exit_retires_the_runtime_owner_and_stops_workers() {
        for observe_first in [false, true] {
            let cores = std::thread::spawn(move || {
                if observe_first {
                    assert!(super::current_runtime().is_none());
                }

                let mut cores = Vec::new();

                for explicit_shutdown in [true, false] {
                    assert!(super::initialize(NativeRuntimeConfiguration::new(4, 4)).is_success());

                    let runtime = super::current_runtime().expect("runtime is initialized");

                    cores.push(std::sync::Arc::clone(&runtime.core));
                    assert_eq!(runtime.core.owners.load(Ordering::Acquire), 1);
                    drop(runtime);

                    if explicit_shutdown {
                        assert!(super::shutdown().is_success());
                    }
                }

                cores
            })
            .join()
            .expect("native owner retirement completes before thread exit");

            for core in cores {
                assert_eq!(core.owners.load(Ordering::Acquire), 0);
                assert!(core.workers.is_stopping(&core.scheduler));

                assert_eq!(
                    core.scheduler
                        .task_count()
                        .expect("fixture scheduler is available"),
                    0
                );
            }
        }
    }

    #[test]
    fn callback_isolation_allows_nested_runtime_creation() {
        let isolation = super::test_runtime_isolation();

        assert!(isolation.is_some());
        assert!(super::test_runtime_isolation().is_none());

        let retained = super::retain_runtime()
            .unwrap_or_else(|status| panic!("nested runtime must initialize: {status:?}"));

        retained.release();
        assert!(super::test_runtime_isolation().is_none());

        drop(isolation);

        assert!(super::test_runtime_isolation().is_some());
    }

    #[test]
    fn independent_native_entry_restores_the_outer_native_task_after_unwind() {
        let task = NativeTaskHandle::new(7).expect("fixed task handle is nonzero");

        super::with_current_task(task, || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                super::with_independent_execution_context(|| {
                    assert_eq!(super::current_task(), None);
                    panic!("exercise native-task restoration");
                });
            }));

            assert!(result.is_err());
            assert_eq!(super::current_task(), Some(task));
        });

        assert_eq!(super::current_task(), None);
    }

    #[test]
    fn independent_entry_restores_complete_context_after_unwind() {
        let _isolation = super::test_runtime_isolation();

        let retained = super::retain_runtime()
            .unwrap_or_else(|status| panic!("runtime must initialize: {status:?}"));

        let ((), status) = super::super::binding::with_cleanup_runtime(&retained, || {
            let runtime = super::current_runtime().expect("cleanup runtime must be bound");

            let task = TaskControlBlock::start(
                crate::test_support::admit_task(),
                TestFrame::completing(1),
            );

            let cancellation = CancellationContext::root();

            assert!(cancellation.request());

            let lane = ExecutionLane::new(
                ExecutionLanePlacement::PinnedWorker(runtime.thread.runtime().id()),
                ExecutionWorkload::Cooperative,
            );

            let scheduler = Scheduler::new(
                [RuntimeCapability::CooperativeExecution],
                runtime.thread.runtime().id(),
                SchedulerLimits::new(
                    NonZeroUsize::new(1).expect("fixed worker limit is nonzero"),
                    NonZeroUsize::new(1).expect("fixed timer limit is nonzero"),
                ),
            ).expect("scheduler backing must be admitted");

            let state = ProtectedFrameStateId::new(0);

            let registration = scheduler
                .register_task(
                    task.id(),
                    task.execution_state(),
                    runtime.thread.runtime().id(),
                    &cancellation,
                )
                .expect("test task must register");

            #[cfg(feature = "test-output")]
            let output = Some(bray_platform::RunOutputContext::discarded());

            #[cfg(not(feature = "test-output"))]
            let output = ();

            let execution = TaskExecutionContext::new(
                task.id(),
                state,
                cancellation,
                &output,
                lane,
                registration.wake_handle(),
            );

            let native_task = NativeTaskHandle::new(7).expect("fixed task handle is nonzero");

            crate::context::with_native_thread_cancellation(cancellation_requested, 0, || {
                crate::context::with_task_execution_context(execution, || {
                    super::with_current_task(native_task, || {
                        assert_eq!(crate::context::current_task_execution_lane(), Some(lane));
                        assert!(crate::current_run_cancellation_requested());

                        #[cfg(feature = "test-output")]
                        assert!(crate::context::current_task_output().is_some());

                        for unwind in [false, true] {
                            let result = catch_unwind(AssertUnwindSafe(|| {
                                super::with_independent_execution_context(|| {
                                    assert!(super::current_runtime().is_none());
                                    assert_eq!(super::current_task(), None);
                                    assert!(crate::current_task_execution_context().is_none());
                                    assert!(!crate::current_run_cancellation_requested());

                                    if unwind {
                                        panic!("exercise complete entry restoration");
                                    }
                                });
                            }));

                            assert_eq!(result.is_err(), unwind);

                            assert!(std::rc::Rc::ptr_eq(
                                &super::current_runtime().expect("fixture `super::super::binding::with_cleanup_runtime` call must succeed"),
                                &runtime
                            ));

                            assert_eq!(super::current_task(), Some(native_task));
                            assert_eq!(crate::context::current_task_execution_lane(), Some(lane));
                            assert!(crate::current_run_cancellation_requested());
                        }

                        let result = catch_unwind(AssertUnwindSafe(|| {
                            super::super::binding::with_cleanup_runtime(&retained, || {
                                assert!(super::current_runtime().is_some());
                                assert_eq!(super::current_task(), None);
                                assert!(crate::context::current_task_execution_context().is_none());
                                assert_eq!(crate::context::current_task_execution_lane(), None);
                                assert!(!crate::current_run_cancellation_requested());

                                #[cfg(feature = "test-output")]
                                assert!(crate::context::current_task_output().is_none());

                                panic!("exercise unwinding restoration");
                            })
                            .expect("nested cleanup entry must bind");
                        }));

                        assert!(result.is_err());
                        assert_eq!(super::current_task(), Some(native_task));
                        assert_eq!(crate::context::current_task_execution_lane(), Some(lane));
                        assert!(crate::current_run_cancellation_requested());

                        #[cfg(feature = "test-output")]
                        assert!(crate::context::current_task_output().is_some());
                    });
                });
            });
        })
        .unwrap_or_else(|status| panic!("cleanup entry must bind: {status:?}"));

        assert!(status.is_success());
        assert!(super::current_runtime().is_none());
        assert_eq!(super::current_task(), None);

        retained.release();
    }

    #[test]
    fn independent_runtime_owners_retire_without_releasing_borrowed_outer_bindings() {
        let _isolation = super::test_runtime_isolation();

        assert!(super::initialize(NativeRuntimeConfiguration::new(4, 4)).is_success());

        let outer = super::current_runtime().expect("outer runtime initialized");

        super::with_bound_runtime(Rc::clone(&outer), || {
            assert_eq!(outer.core.owners.load(Ordering::Acquire), 1);
        });

        assert_eq!(outer.core.owners.load(Ordering::Acquire), 1);

        for explicit_shutdown in [false, true] {
            let inner = super::with_independent_execution_context(|| {
                assert!(super::initialize(NativeRuntimeConfiguration::new(4, 4)).is_success());

                let inner = super::current_runtime().expect("inner runtime initialized");
                let core = std::sync::Arc::clone(&inner.core);

                if explicit_shutdown {
                    assert!(super::shutdown().is_success());
                }

                core
            });

            assert_eq!(inner.owners.load(Ordering::Acquire), 0);
            assert!(inner.workers.is_stopping(&inner.scheduler));

            assert!(Rc::ptr_eq(
                &super::current_runtime().expect("fixture runtime must be active"),
                &outer
            ));

            assert_eq!(outer.core.owners.load(Ordering::Acquire), 1);
        }

        let mut unwound = None;

        let result = catch_unwind(AssertUnwindSafe(|| {
            super::with_independent_execution_context(|| {
                assert!(super::initialize(NativeRuntimeConfiguration::new(4, 4)).is_success());

                let runtime = super::current_runtime().expect("unwinding runtime initialized");

                unwound = Some(std::sync::Arc::clone(&runtime.core));
                panic!("exercise owned-runtime retirement while unwinding");
            });
        }));

        assert!(result.is_err());

        let unwound = unwound.expect("fixture `unwound` call must succeed");

        assert_eq!(unwound.owners.load(Ordering::Acquire), 0);
        assert!(unwound.workers.is_stopping(&unwound.scheduler));

        assert!(Rc::ptr_eq(
            &super::current_runtime().expect("fixture runtime must be active"),
            &outer
        ));

        let retained = super::with_independent_execution_context(|| {
            assert!(super::initialize(NativeRuntimeConfiguration::new(4, 4)).is_success());

            super::retain_current_runtime()
                .expect("fixture runtime must remain retained")
                .expect("successful fixture query must contain its expected result")
        });

        assert_eq!(retained.core.owners.load(Ordering::Acquire), 1);
        assert!(!retained.core.workers.is_stopping(&retained.core.scheduler));
        retained.release();
        assert_eq!(retained.core.owners.load(Ordering::Acquire), 0);
        assert!(retained.core.workers.is_stopping(&retained.core.scheduler));
        assert!(super::shutdown().is_success());
        assert_eq!(outer.core.owners.load(Ordering::Acquire), 0);
    }
