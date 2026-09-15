use crate::incident::OwnedCleanupIncident;

pub(crate) fn run_static_finalizer(
    admission: crate::TaskAdmission,
    frame: bray_runtime_abi::NativeInactiveFrame,
    resolve: bray_runtime_abi::NativeStaticFinalizerResolveCallback,
) -> [Option<OwnedCleanupIncident>; 4] {
    use bray_runtime_abi::{NativeRootHandle, NativeRunState};

    let incidents = super::state::with_runtime(|runtime| {
        let allocation = runtime.allocate_admitted(admission);

        let Some(task) = allocation.task() else {
            return [
                Some(OwnedCleanupIncident::runtime_failure()),
                None,
                None,
                None,
            ];
        };

        if !runtime.start(task, frame.into_protected()).is_success() {
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
    });

    incidents
}

pub(crate) fn with_static_cleanup_runtime<T>(
    callback: impl FnOnce() -> T,
) -> (T, Option<OwnedCleanupIncident>) {
    with_selected_static_cleanup_runtime(None, callback)
}

pub(crate) fn with_retained_static_cleanup_runtime<T>(
    runtime: &super::state::RetainedRuntime,
    callback: impl FnOnce() -> T,
) -> (T, Option<OwnedCleanupIncident>) {
    with_selected_static_cleanup_runtime(Some(runtime), callback)
}

fn with_selected_static_cleanup_runtime<T>(
    runtime: Option<&super::state::RetainedRuntime>,
    callback: impl FnOnce() -> T,
) -> (T, Option<OwnedCleanupIncident>) {
    let mut callback = Some(callback);
    let mut result = None;

    let runtime = super::state::with_cleanup_runtime(runtime, || {
        let Some(callback) = callback.take() else {
            return false;
        };

        result = Some(callback());

        super::state::with_runtime(|runtime| runtime.report_cleanup_incidents())
            .is_ok_and(|status| status.is_success())
    });

    let runtime_succeeded = match runtime {
        Ok((reported, shutdown)) => reported && shutdown.is_success(),
        Err(_) => false,
    };

    if result.is_none()
        && let Some(callback) = callback.take()
    {
        result = Some(callback());
    }

    let incident = (!runtime_succeeded).then(OwnedCleanupIncident::runtime_failure);

    let Some(result) = result else {
        unreachable!("static cleanup callback must run exactly once")
    };

    (result, incident)
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::{
        NativeFrameAffinity, NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind,
        NativeFrameState, NativeInactiveFrame, NativeLaneRequirements, NativeProtectedFrame,
        NativeStaticFinalizerStatus,
    };

    use super::{run_static_finalizer, with_static_cleanup_runtime};

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
        let _attachment = bray_platform::RuntimeThreadScope::enter()
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

        NativeInactiveFrame::new(context, move_before_start)
    }

    extern "C" fn move_before_start(context: usize) -> NativeProtectedFrame {
        NativeProtectedFrame::new(
            context,
            [9; 32],
            1,
            1,
            1,
            1,
            1,
            state,
            resume,
            cancel,
            ignore_action,
            ignore_resolution,
            ignore_completion_move,
            ignore_action,
        )
    }

    extern "C" fn state(context: usize, _: u32) -> NativeFrameState {
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

    extern "C-unwind" fn ignore_action(_: usize) {}

    extern "C-unwind" fn ignore_resolution(
        _: &mut bray_runtime_abi::NativeFrameProgress,
        _: usize,
        _: NativeFrameExit,
    ) {
    }

    extern "C-unwind" fn ignore_completion_move(_: usize, _: usize) {}

    extern "C-unwind" fn resolve(
        _: usize,
        _: usize,
        _: &mut bray_runtime_abi::NativeRunOutcome,
    ) -> NativeStaticFinalizerStatus {
        NativeStaticFinalizerStatus::SUCCESS
    }
}
