#[cfg(test)]
pub(crate) fn with_static_cleanup_runtime<T>(
    callback: impl FnOnce() -> T,
) -> (T, Vec<crate::incident::OwnedCleanupIncident>) {
    struct Release(super::state::RetainedRuntime);

    impl Drop for Release {
        fn drop(&mut self) {
            self.0.release();
        }
    }

    let runtime = match super::state::retain_runtime() {
        Ok(runtime) => runtime,
        Err(bray_runtime_abi::NativeRuntimeStatus::NOT_INITIALIZED) => {
            super::state::admit_cleanup_runtime().unwrap()
        }
        Err(status) => panic!("fixture execution must be admitted: {status:?}"),
    };

    let runtime = Release(runtime);

    with_retained_static_cleanup_runtime(&runtime.0, callback)
}

pub(super) fn with_retained_static_cleanup_runtime<T>(
    runtime: &super::state::RetainedRuntime,
    callback: impl FnOnce() -> T,
) -> (T, Vec<crate::incident::OwnedCleanupIncident>) {
    let mut callback = Some(callback);
    let mut result = None;
    let mut incidents = Vec::new();

    let runtime = super::state::with_cleanup_runtime(runtime, || {
        let Some(callback) = callback.take() else {
            return false;
        };

        result = Some(callback());

        super::state::with_runtime(|runtime| runtime.report_cleanup_incidents())
            .is_ok_and(|status| status.is_success())
    });

    let runtime_succeeded = match runtime {
        Ok(reported) => reported,
        Err(_) => false,
    };

    if result.is_none()
        && let Some(callback) = callback.take()
    {
        result = Some(callback());
    }

    if !runtime_succeeded {
        incidents.push(crate::incident::OwnedCleanupIncident::runtime_failure());
    }

    let Some(result) = result else {
        unreachable!("static cleanup callback must run exactly once")
    };

    (result, incidents)
}

#[cfg(test)]
mod tests {
    #[test]
    fn cleanup_runtime_reuses_an_active_foreign_thread_attachment() {
        let _attachment = bray_platform::RuntimeThreadScope::enter().unwrap();
        let before = bray_platform::current_runtime_thread();

        let (during, incidents) =
            super::with_static_cleanup_runtime(|| bray_platform::current_runtime_thread());

        assert_eq!(during, before);
        assert_eq!(bray_platform::current_runtime_thread(), before);
        assert!(incidents.is_empty());
    }
}
