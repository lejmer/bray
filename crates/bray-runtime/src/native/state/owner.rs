use bray_runtime_abi::{NativeRuntimeConfiguration, NativeRuntimeStatus};

#[expect(
    unsafe_code,
    reason = "native storage owns the complete Source execution owner until the consuming release"
)]
unsafe extern "C" {
    safe fn bray_runtime_execution_owner_current(output: &mut RetainedRuntime) -> NativeRuntimeStatus;
    safe fn bray_runtime_execution_owner_clone(input: &RetainedRuntime, output: &mut RetainedRuntime);
    safe fn bray_runtime_execution_owner_worker(input: &RetainedRuntime) -> u32;
    safe fn bray_runtime_execution_owner_detach(input: &RetainedRuntime);
    safe fn bray_runtime_execution_startup(configuration: NativeRuntimeConfiguration) -> NativeRuntimeStatus;
}

/// Owns the Source service claim and its storage pin through native callbacks.
/// Its word is private Source storage, never an observational runtime identity.
#[repr(C)]
pub(crate) struct RetainedRuntime {
    owner: usize,
}

impl RetainedRuntime {
    const fn empty() -> Self {
        Self { owner: 0 }
    }

    pub(crate) fn release(self) {
        drop(self);
    }

    pub(crate) fn owns_current_worker(&self) -> bool {
        bray_runtime_execution_owner_worker(self) != 0
    }

    pub(crate) fn detach_product_workers(&self, _product: usize) {
        bray_runtime_execution_owner_detach(self);
    }
}

impl Clone for RetainedRuntime {
    fn clone(&self) -> Self {
        let mut output = Self::empty();

        bray_runtime_execution_owner_clone(self, &mut output);

        assert_ne!(output.owner, 0, "Source retention initializes a complete execution owner");

        output
    }
}

impl Drop for RetainedRuntime {
    fn drop(&mut self) {
        if self.owner == 0 {
            return;
        }

        // The cleanup scope keeps the genuine current driver or installs an
        // Origin driver before the service claim can retire its own backing.
        let bridge = (!crate::bootstrap::in_thread_cleanup())
            .then(super::super::bridge::Scope::enter)
            .transpose()
            .expect("retained execution release keeps its native attachment available");
        let execution = super::super::scope::Scope::borrow_owner(self)
            .expect("retained execution release keeps its cleanup driver available");
        let incidents = execution.release_owner(self);

        // Source consumed the actual owner in this slot. The native carrier now
        // has no Source lifecycle obligation, including during Rust unwinding.
        self.owner = 0;

        let status = execution.finish();
        let attachment_incidents = bridge.map_or(0, super::super::bridge::Scope::finish);

        if incidents != 0 {
            super::super::host::record_cleanup_failure(incidents as usize);
        }

        if !status.is_success() || attachment_incidents != 0 {
            super::super::host::record_cleanup_failure(attachment_incidents.max(1));
        }
    }
}

pub(crate) fn retain_current_runtime() -> Result<Option<RetainedRuntime>, NativeRuntimeStatus> {
    let mut output = RetainedRuntime::empty();

    match bray_runtime_execution_owner_current(&mut output) {
        NativeRuntimeStatus::SUCCESS => {
            assert_ne!(output.owner, 0, "Source publishes an owner on successful retention");

            Ok(Some(output))
        }
        NativeRuntimeStatus::NOT_INITIALIZED => Ok(None),
        status => Err(status),
    }
}

pub(crate) fn retain_runtime() -> Result<RetainedRuntime, NativeRuntimeStatus> {
    if let Some(owner) = retain_current_runtime()? {
        return Ok(owner);
    }

    let bridge = super::super::bridge::Scope::enter()?;
    let execution = super::super::scope::Scope::enter(true)?;
    let configuration = native_configuration(NativeRuntimeConfiguration::new(usize::MAX, usize::MAX));
    let status = bray_runtime_execution_startup(configuration);

    if !status.is_success() {
        return Err(status);
    }

    let owner = retain_current_runtime()?.expect("Source startup installs its execution owner");
    let status = execution.finish();
    let incidents = bridge.finish();

    if !status.is_success() || incidents != 0 {
        return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
    }

    Ok(owner)
}

pub(in crate::native) fn native_configuration(configuration: NativeRuntimeConfiguration) -> NativeRuntimeConfiguration {
    configuration.with_native_callbacks(
        super::super::task::bray_runtime_execution_invocation,
        super::super::task::bray_runtime_execution_context_admission,
        super::super::task::bray_runtime_execution_context_release,
        super::super::bridge::enter_provider,
        super::super::bridge::finish_provider,
    )
}

pub(crate) fn with_cleanup_runtime<T>(
    retained: &RetainedRuntime,
    callback: impl FnOnce() -> T,
) -> Result<(T, NativeRuntimeStatus), NativeRuntimeStatus> {
    // A closing attachment already supplies the retained driver and forbids
    // another attachment acquisition. Source borrows that same context.
    let bridge = (!crate::bootstrap::in_thread_cleanup())
        .then(super::super::bridge::Scope::enter)
        .transpose()?;
    let execution = super::super::scope::Scope::borrow_owner(retained)?;
    let result = callback();
    let status = execution.finish();
    let incidents = bridge.map_or(0, super::super::bridge::Scope::finish);

    Ok((result, if incidents == 0 { status } else { NativeRuntimeStatus::RUNTIME_FAILURE }))
}
