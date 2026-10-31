use crate::incident::OwnedCleanupIncident;

pub(crate) fn run_static_finalizer(
    _admission: crate::TaskAdmission,
    frame: bray_runtime_abi::NativeInactiveFrame,
    resolve: bray_runtime_abi::NativeStaticFinalizerResolveCallback,
) -> [Option<OwnedCleanupIncident>; 4] {
    use bray_runtime_abi::{NativeRootHandle, NativeRunState};

    super::state::with_runtime(|runtime| {
        let allocation = runtime.allocate();

        let Some(task) = allocation.task() else {
            return [
                Some(OwnedCleanupIncident::runtime_failure()),
                None,
                None,
                None,
            ];
        };

        if !runtime.start(task, &frame).is_success() {
            return [
                Some(OwnedCleanupIncident::runtime_failure()),
                None,
                None,
                None,
            ];
        }

        let Some(root) = NativeRootHandle::new(task.raw()) else {
            return [
                Some(OwnedCleanupIncident::runtime_failure()),
                None,
                None,
                None,
            ];
        };

        let mut outcome = runtime.observe_root(root);

        let [first, second, third] = match outcome.state() {
            NativeRunState::COMPLETED => {
                crate::product::collect_finalizer_incidents(|incident, destination| {
                    resolve(outcome.payload(), (&raw mut *incident).addr(), destination)
                })
            }
            NativeRunState::PANICKED => [
                Some(OwnedCleanupIncident::report_owner(outcome.take_report())),
                None,
                None,
            ],
            NativeRunState::CANCELLED
            | NativeRunState::PENDING
            | NativeRunState::RUNTIME_FAILURE => {
                [Some(OwnedCleanupIncident::runtime_failure()), None, None]
            }
            _ => [Some(OwnedCleanupIncident::runtime_failure()), None, None],
        };

        let resolution = (!runtime.resolve_root_completion(root).is_success())
            .then(OwnedCleanupIncident::runtime_failure);

        [first, second, third, resolution]
    })
    .unwrap_or_else(|_| {
        [
            Some(OwnedCleanupIncident::runtime_failure()),
            None,
            None,
            None,
        ]
    })
}

pub(crate) fn with_static_cleanup_context<T>(
    runtime: Option<&super::state::RetainedRuntime>,
    callback: impl FnOnce() -> T,
) -> Result<(T, Option<OwnedCleanupIncident>), bray_runtime_abi::NativeRuntimeStatus> {
    let Some(runtime) = runtime else {
        let bridge = (!crate::bootstrap::in_thread_cleanup())
            .then(super::bridge::Scope::enter)
            .transpose()?;

        let result = super::state::with_independent_execution_context(callback);
        let incidents = bridge.map_or(0, super::bridge::Scope::finish);
        let incident = (incidents != 0).then(OwnedCleanupIncident::runtime_failure);

        return Ok((result, incident));
    };

    let ((result, reported), shutdown) = super::state::with_cleanup_runtime(runtime, || {
        let result = callback();

        let reported = super::state::with_runtime(|runtime| runtime.report_cleanup_incidents())
            .is_ok_and(|status| status.is_success());

        (result, reported)
    })?;

    let incident =
        (!reported || !shutdown.is_success()).then(OwnedCleanupIncident::runtime_failure);

    Ok((result, incident))
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::{
        NativeFrameAffinity, NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind,
        NativeFrameState, NativeInactiveFrame, NativeLaneRequirements, NativeStaticFinalizerStatus,
    };

    use super::run_static_finalizer;

    fn with_static_cleanup_runtime<T>(
        callback: impl FnOnce() -> T,
    ) -> (T, Option<crate::incident::OwnedCleanupIncident>) {
        let runtime = super::super::state::retain_runtime().expect("cleanup runtime forms");
        let result = super::with_static_cleanup_context(Some(&runtime), callback);

        runtime.release();

        result.expect("cleanup context attaches")
    }

    #[test]
    fn synchronous_cleanup_is_attached_without_a_scheduler_and_restores_context() {
        let _isolation = super::super::state::test_runtime_isolation();
        let cancellation = crate::CancellationContext::root();

        assert!(cancellation.request());

        assert!(
            super::super::state::retain_current_runtime()
                .expect("fixture runtime must remain retained")
                .is_none()
        );

        assert!(crate::bootstrap::current_runtime_thread().is_none());

        crate::context::with_run_cancellation_context(cancellation, || {
            let ((), incident) = super::with_static_cleanup_context(None, || {
                assert!(crate::bootstrap::current_runtime_thread().is_some());

                assert!(
                    super::super::state::retain_current_runtime()
                        .expect("fixture static cleanup context must be installed")
                        .is_none()
                );

                assert!(!crate::current_run_cancellation_requested());
            })
            .expect("fixture static cleanup context must be installed");

            assert!(incident.is_none());
            assert!(crate::current_run_cancellation_requested());
        });

        assert!(crate::bootstrap::current_runtime_thread().is_none());

        assert!(
            super::super::state::retain_current_runtime()
                .expect("fixture runtime must remain retained")
                .is_none()
        );
    }

    #[test]
    fn synchronous_cleanup_bridge_isolates_nested_entry_from_a_body_installed_runtime() {
        let _isolation = super::super::state::test_runtime_isolation();

        use super::super::entry::bray_runtime_foreign_callback_execution;

        extern "C" fn nested(
            _: usize,
            outcome: &mut bray_runtime_abi::NativeRunOutcome,
            _: &mut bray_runtime_abi::NativeReportRecords,
        ) {
            let isolated = super::super::state::with_runtime(|_| ()).is_err();

            *outcome = bray_runtime_abi::NativeRunOutcome::new(
                bray_runtime_abi::NativeRunState::COMPLETED,
                usize::from(isolated),
            );
        }

        let ((), incident) = super::with_static_cleanup_context(None, || {
            assert!(
                super::super::state::initialize(bray_runtime_abi::NativeRuntimeConfiguration::new(
                    16, 16
                ))
                .is_success()
            );

            let mut outcome = bray_runtime_abi::NativeRunOutcome::new(
                bray_runtime_abi::NativeRunState::RUNTIME_FAILURE,
                0,
            );

            assert_eq!(
                bray_runtime_foreign_callback_execution(nested, 0, &mut outcome),
                0
            );

            assert_eq!(outcome.state(), bray_runtime_abi::NativeRunState::COMPLETED);
            assert_eq!(outcome.payload(), 1);
            assert!(super::super::state::with_runtime(|_| ()).is_ok());
        })
        .expect("fixture static cleanup context must be installed");

        assert!(incident.is_none());
        assert!(super::super::state::with_runtime(|_| ()).is_err());
        assert!(crate::bootstrap::current_runtime_thread().is_none());
    }

    #[test]
    fn retained_cleanup_borrows_a_retiring_attachment_without_reacquiring_it() {
        let _isolation = super::super::state::test_runtime_isolation();

        extern "C" fn cleanup(identity: u64) -> u32 {
            crate::bootstrap::with_thread_cleanup(identity, || {
                let runtime = RETIRING_RUNTIME
                    .with_borrow(|runtime| runtime.clone())
                    .expect("fixture `with_borrow` call must succeed");

                let ((), incident) = super::with_static_cleanup_context(Some(&runtime), || {
                    assert_eq!(
                        crate::bootstrap::current_runtime_thread()
                            .expect("fixture static cleanup context must be installed")
                            .id()
                            .raw(),
                        identity
                    );
                })
                .expect("terminal cleanup borrows the established attachment");

                assert!(incident.is_none());
            });

            0
        }

        thread_local! {
            static RETIRING_RUNTIME: std::cell::RefCell<Option<super::super::state::RetainedRuntime>> = const { std::cell::RefCell::new(None) };
        }

        let runtime = super::super::state::retain_runtime()
            .expect("fixture `super::super::state::retain_runtime` call must succeed");

        let attachment = crate::bootstrap::RuntimeThreadScope::enter()
            .expect("fixture execution scope must admit entry");

        RETIRING_RUNTIME.set(Some(runtime.clone()));
        assert!(crate::bootstrap::register_thread_cleanup(cleanup));
        assert_eq!(attachment.finish(), 0);
        RETIRING_RUNTIME.set(None);
        runtime.release();
    }

    #[test]
    fn admitted_finalizer_runs_after_further_outgoing_admission_is_rejected() {
        let admission = crate::test_support::admit_task();
        let frame = inactive_frame(NativeFrameAffinity::MOVABLE, NativeLaneRequirements::NONE);
        let _failure = crate::outgoing::tests::reject_admission();

        assert!(crate::TaskAdmission::new().is_err());

        let (incidents, runtime_incident) =
            with_static_cleanup_runtime(|| run_static_finalizer(admission, frame, resolve));

        assert!(incidents.iter().all(Option::is_none));
        assert!(runtime_incident.is_none());
    }

    #[test]
    fn cleanup_runtime_reuses_an_active_foreign_thread_attachment() {
        let _attachment = crate::bootstrap::RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("test thread must attach: {error:?}"));

        let (incidents, runtime_incidents) = with_static_cleanup_runtime(|| {
            run_static_finalizer(
                crate::test_support::admit_task(),
                inactive_frame(
                    NativeFrameAffinity::ORIGIN_THREAD,
                    NativeLaneRequirements::NONE,
                ),
                resolve,
            )
        });

        assert!(incidents.iter().all(Option::is_none));
        assert!(runtime_incidents.is_none());
    }

    #[test]
    fn cleanup_runtime_services_every_owned_cleanup_lane() {
        let cases = [
            (NativeFrameAffinity::MOVABLE, NativeLaneRequirements::NONE),
            (
                NativeFrameAffinity::MOVABLE,
                NativeLaneRequirements::BLOCKING,
            ),
            (
                NativeFrameAffinity::MOVABLE,
                NativeLaneRequirements::COMPUTE,
            ),
        ];

        let (incidents, runtime_incidents) = with_static_cleanup_runtime(|| {
            cases
                .into_iter()
                .flat_map(|(affinity, requirements)| {
                    run_static_finalizer(
                        crate::test_support::admit_task(),
                        inactive_frame(affinity, requirements),
                        resolve,
                    )
                })
                .collect::<Vec<_>>()
        });

        assert!(incidents.iter().all(Option::is_none));
        assert!(runtime_incidents.is_none());
    }

    #[test]
    fn cleanup_runtime_requires_a_retained_main_thread_identity() {
        let main_frame = || {
            inactive_frame(
                NativeFrameAffinity::MAIN_THREAD,
                NativeLaneRequirements::MAIN_THREAD,
            )
        };

        let (incidents, runtime_incidents) = with_static_cleanup_runtime(|| {
            run_static_finalizer(crate::test_support::admit_task(), main_frame(), resolve)
        });

        assert_eq!(incidents.iter().flatten().count(), 1);
        assert!(runtime_incidents.is_none());

        assert!(
            super::super::state::initialize(bray_runtime_abi::NativeRuntimeConfiguration::new(
                16, 16,
            ))
            .is_success()
        );

        let (incidents, runtime_incidents) = with_static_cleanup_runtime(|| {
            run_static_finalizer(crate::test_support::admit_task(), main_frame(), resolve)
        });

        assert!(incidents.iter().all(Option::is_none));
        assert!(runtime_incidents.is_none());
        assert!(super::super::state::shutdown().is_success());
    }

    fn inactive_frame(
        affinity: NativeFrameAffinity,
        requirements: NativeLaneRequirements,
    ) -> NativeInactiveFrame {
        let context =
            usize::try_from(u64::from(affinity.code()) | (u64::from(requirements.bits()) << 32))
                .unwrap_or_else(|_| panic!("native frame state must fit the test target"));

        move_before_start(context)
    }

    extern "C" fn move_before_start(context: usize) -> NativeInactiveFrame {
        super::super::fixtures::admit(
            context,
            bray_runtime_abi::NativeActivationDescriptor {
                storage: bray_runtime_abi::NativeCleanupStorageShape::new([9; 32], 1, 1),
                header: 0,
                completion: 0,
                completion_bytes: 1,
                completion_align: 1,
                states: 1,
                state,
                resume,
                cancel,
                broadcast: ignore_broadcast,
                resolve: ignore_resolution,
                move_completion: ignore_completion_move,
                destroy: ignore_action,
            },
        )
    }

    extern "C" fn state(context: usize, _: u32, _: u32, _: usize) -> NativeFrameState {
        let context = u64::try_from(context)
            .unwrap_or_else(|_| panic!("test frame context must fit the native ABI"));

        let affinity = match context as u32 {
            0 => NativeFrameAffinity::MOVABLE,
            1 => NativeFrameAffinity::ORIGIN_THREAD,
            2 => NativeFrameAffinity::MAIN_THREAD,
            _ => panic!("test frame affinity must be known"),
        };

        NativeFrameState::new(
            affinity,
            NativeLaneRequirements::from_bits((context >> 32) as u32),
        )
    }

    extern "C-unwind" fn resume(destination: &mut NativeFrameProgress, _: usize) {
        *destination = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
    }

    extern "C-unwind" fn cancel(destination: &mut NativeFrameProgress, _: usize) {
        *destination = NativeFrameProgress::new(NativeFrameProgressKind::CANCELLED, 0, 0);
    }

    extern "C-unwind" fn ignore_broadcast(_: &mut NativeFrameProgress, _: usize) {}

    extern "C-unwind" fn ignore_action(_: usize) {}

    extern "C-unwind" fn ignore_resolution(
        _: &mut bray_runtime_abi::NativeFrameProgress,
        _: usize,
        _: NativeFrameExit,
    ) {
    }

    extern "C-unwind" fn ignore_completion_move(_: usize, _: usize) {}

    extern "C" fn resolve(
        _: usize,
        _: usize,
        _: &mut bray_runtime_abi::NativeRunOutcome,
    ) -> NativeStaticFinalizerStatus {
        NativeStaticFinalizerStatus::SUCCESS
    }
}
