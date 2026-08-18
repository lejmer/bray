pub(crate) fn run_static_finalizer(
    frame: bray_runtime_abi::NativeInactiveFrame,
    resolve: bray_runtime_abi::NativeStaticFinalizerResolveCallback,
) -> Vec<crate::product::CleanupIncident> {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use bray_runtime_abi::{
        NativeRootHandle, NativeRunState, NativeRuntimeConfiguration, NativeRuntimeStatus,
    };

    let initialized =
        super::state::initialize(NativeRuntimeConfiguration::new(usize::MAX, usize::MAX));

    let temporary = initialized == NativeRuntimeStatus::SUCCESS;

    if !temporary && initialized != NativeRuntimeStatus::ALREADY_INITIALIZED {
        return vec![crate::product::CleanupIncident::runtime_failure()];
    }

    let mut incidents = super::state::with_runtime(|runtime| {
        let allocation = runtime.allocate();

        let Some(task) = allocation.task() else {
            return vec![crate::product::CleanupIncident::runtime_failure()];
        };

        if !runtime.start(task, frame.into_protected()).is_success() {
            return vec![crate::product::CleanupIncident::runtime_failure()];
        }

        let Some(root) = NativeRootHandle::new(task.raw()) else {
            return vec![crate::product::CleanupIncident::runtime_failure()];
        };

        let outcome = runtime.observe_root(root);

        let mut incidents = match outcome.state() {
            NativeRunState::COMPLETED => {
                let mut incident = crate::product::empty_native_incident();
                let destination = (&raw mut incident).addr();

                match catch_unwind(AssertUnwindSafe(|| resolve(outcome.payload(), destination))) {
                    Ok(status) => crate::product::incidents_from_status(status, incident),
                    Err(payload) => vec![crate::product::CleanupIncident::panic(payload)],
                }
            }
            NativeRunState::PANICKED => {
                let _ = super::export::bray_runtime_panic_reporting_v1(outcome.payload());

                vec![crate::product::CleanupIncident::runtime_failure()]
            }
            NativeRunState::CANCELLED
            | NativeRunState::PENDING
            | NativeRunState::RUNTIME_FAILURE => {
                vec![crate::product::CleanupIncident::runtime_failure()]
            }
            _ => vec![crate::product::CleanupIncident::runtime_failure()],
        };

        if !runtime.resolve_root_completion(root).is_success() {
            incidents.push(crate::product::CleanupIncident::runtime_failure());
        }

        incidents
    })
    .unwrap_or_else(|_| vec![crate::product::CleanupIncident::runtime_failure()]);

    if super::state::with_runtime(|runtime| runtime.report_cleanup_incidents())
        .map_or(true, |status| !status.is_success())
    {
        incidents.push(crate::product::CleanupIncident::runtime_failure());
    }

    if temporary && !super::state::shutdown().is_success() {
        incidents.push(crate::product::CleanupIncident::runtime_failure());
    }

    incidents
}
