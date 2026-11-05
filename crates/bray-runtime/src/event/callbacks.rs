use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use bray_runtime_abi::{NativeEventCallback, NativeRuntimeStatus};

use crate::native::RetainedRuntime;
use crate::{CancellationContext, RootCancellationHandle, TaskWakeCause, TaskWakeHandle};

// Each native task admits one callback resource before its frame transfers.
// Source owns membership and descriptor copies. This table owns only native
// context/provider resources, which cannot be recovered from raw ABI addresses.
static CALLBACKS: LazyLock<Mutex<HashMap<usize, Binding>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) enum Resources {
    Task {
        cancellation: CancellationContext,
        wake: TaskWakeHandle,
    },
    SourceTask {
        wake: TaskWakeHandle,
        output: crate::context::TaskOutput,
    },
    Timeout {
        cancellation: RootCancellationHandle,
        runtime: RetainedRuntime,
    },
}

impl std::fmt::Debug for Resources {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceTask { .. } => formatter
                .debug_tuple("SourceTask")
                .field(&self.reference().task())
                .finish(),
            Self::Task { .. } => formatter.write_str("Task"),
            Self::Timeout { .. } => formatter.write_str("Timeout"),
        }
    }
}

impl Resources {
    pub(crate) fn reference(&self) -> &bray_runtime_abi::NativeTaskReference {
        self.wake().native_cancellation().native_reference()
    }

    pub(crate) fn cancellation(&self) -> &CancellationContext {
        self.wake().native_cancellation()
    }

    pub(crate) fn wake(&self) -> &TaskWakeHandle {
        let Self::SourceTask { wake, .. } = self else {
            panic!("native dispatch borrows its Source task context");
        };

        wake
    }

    pub(crate) fn output(&self) -> &crate::context::TaskOutput {
        let Self::SourceTask { output, .. } = self else {
            panic!("native dispatch borrows its Source task output context");
        };

        output
    }
}

struct Binding {
    resources: Arc<Resources>,
    native_owner: bool,
    descriptors: usize,
}

pub(crate) struct NativeCallback {
    identity: usize,
}

impl NativeCallback {
    #[cfg(test)]
    pub(crate) fn task_count() -> usize {
        CALLBACKS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .filter(|binding| matches!(binding.resources.as_ref(), Resources::SourceTask { .. }))
            .count()
    }

    #[cfg(test)]
    pub(crate) fn task_owner(task: bray_runtime_abi::NativeTaskHandle) -> Option<Arc<Resources>> {
        let callbacks = CALLBACKS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        callbacks
            .values()
            .find(|binding| {
                matches!(binding.resources.as_ref(), Resources::SourceTask { .. })
                    && binding.resources.reference().task() == task
            })
            .map(|binding| Arc::clone(&binding.resources))
    }

    pub(crate) fn root_cancellation(&self) -> RootCancellationHandle {
        let owner = Self::resources(self.identity);

        RootCancellationHandle::new(owner.cancellation().clone())
    }

    pub(crate) fn admit(
        cancellation: CancellationContext,
        output: crate::context::TaskOutput,
    ) -> Result<Self, NativeRuntimeStatus> {
        Self::insert(Resources::SourceTask {
            wake: TaskWakeHandle::native(cancellation),
            output,
        })
    }

    pub(crate) fn task(
        cancellation: CancellationContext,
        wake: TaskWakeHandle,
    ) -> Result<Self, NativeRuntimeStatus> {
        Self::insert(Resources::Task { cancellation, wake })
    }

    pub(crate) fn timeout(
        cancellation: RootCancellationHandle,
        runtime: RetainedRuntime,
    ) -> Result<Self, NativeRuntimeStatus> {
        Self::insert(Resources::Timeout {
            cancellation,
            runtime,
        })
    }

    pub(crate) fn resources(identity: usize) -> Arc<Resources> {
        let callbacks = CALLBACKS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        Arc::clone(
            &callbacks
                .get(&identity)
                .expect("Source retains its admitted native context")
                .resources,
        )
    }

    pub(crate) fn publish(self) {
        let owner = Self::resources(self.identity);

        crate::report_provider::bray_runtime_task_native_context_publication(
            owner.reference(),
            self.identity,
        );

        // Source takes the native-owner obligation at publication and returns it
        // to release_owner after withdrawing this task's wait registration.
        std::mem::forget(self);
    }

    pub(crate) fn release_owner(identity: usize) {
        // Source withdraws its task-cell wait before transferring this owner
        // back. Descriptors and active callbacks retain this same resource.
        drop(Self { identity });
    }

    fn insert(resources: Resources) -> Result<Self, NativeRuntimeStatus> {
        let resources = Arc::new(resources);

        let identity = Arc::as_ptr(&resources) as usize;

        let binding = Binding {
            resources,
            native_owner: true,
            descriptors: 0,
        };

        let mut prepared = HashMap::new();

        loop {
            let required = {
                let mut callbacks = CALLBACKS
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);

                if callbacks.len() < callbacks.capacity() {
                    assert!(
                        callbacks.insert(identity, binding).is_none(),
                        "a live callback allocation is unique"
                    );

                    return Ok(Self { identity });
                }

                if prepared.capacity() > callbacks.len() {
                    std::mem::swap(&mut *callbacks, &mut prepared);
                    callbacks.extend(prepared.drain());
                    continue;
                }

                callbacks
                    .len()
                    .checked_add(1)
                    .ok_or(NativeRuntimeStatus::RUNTIME_FAILURE)?
            };

            prepared
                .try_reserve(required)
                .map_err(|_| NativeRuntimeStatus::RUNTIME_FAILURE)?;
        }
    }

    // Retain resources for the descriptor transferred into a Source registration.
    // This only clones already admitted owners and never allocates.
    pub(crate) fn retain_timer(&self) -> NativeEventCallback {
        self.retain_with(invoke_timer)
    }

    pub(super) fn retain(&self) -> NativeEventCallback {
        self.retain_with(invoke)
    }

    fn retain_with(
        &self,
        call: extern "C" fn(usize, u32) -> NativeRuntimeStatus,
    ) -> NativeEventCallback {
        let mut callbacks = CALLBACKS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let binding = callbacks
            .get_mut(&self.identity)
            .expect("the native owner retains its event callback");

        binding.descriptors = binding
            .descriptors
            .checked_add(1)
            .expect("retained event descriptors remain representable");

        NativeEventCallback {
            context: self.identity,
            call,
        }
    }
}

impl Drop for NativeCallback {
    fn drop(&mut self) {
        let removed = {
            let mut callbacks = CALLBACKS
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            let binding = callbacks
                .get_mut(&self.identity)
                .expect("the native callback owner releases once");

            binding.native_owner = false;

            if binding.descriptors == 0 {
                callbacks.remove(&self.identity)
            } else {
                None
            }
        };

        drop(removed);
    }
}

pub(super) extern "C" fn invoke(context: usize, notify: u32) -> NativeRuntimeStatus {
    invoke_with(context, notify, TaskWakeCause::Explicit)
}

extern "C" fn invoke_timer(context: usize, notify: u32) -> NativeRuntimeStatus {
    invoke_with(context, notify, TaskWakeCause::Timer)
}

fn invoke_with(context: usize, notify: u32, cause: TaskWakeCause) -> NativeRuntimeStatus {
    let result = std::panic::catch_unwind(|| {
        if notify == 0 {
            release(context);
            return;
        }

        let resources = {
            let callbacks = CALLBACKS
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            let binding = callbacks
                .get(&context)
                .expect("Source invokes a retained callback descriptor");

            Arc::clone(&binding.resources)
        };

        // No resource-table lock survives callback entry.
        match resources.as_ref() {
            Resources::Task {
                cancellation, wake, ..
            } => {
                let _cancellation = cancellation;
                let _ = wake.wake_with_cause(cause);
            }
            Resources::SourceTask { wake, .. } => {
                let _ = wake.wake_with_cause(cause);
            }
            Resources::Timeout { cancellation, .. } => {
                cancellation.request_timeout();
            }
        }
    });

    if result.is_err() {
        NativeRuntimeStatus::PANICKED
    } else {
        NativeRuntimeStatus::SUCCESS
    }
}

fn release(context: usize) {
    let removed = {
        let mut callbacks = CALLBACKS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let binding = callbacks
            .get_mut(&context)
            .expect("Source releases a retained event descriptor");

        binding.descriptors = binding
            .descriptors
            .checked_sub(1)
            .expect("Source releases each descriptor once");

        if binding.descriptors != 0 {
            return;
        }

        let removed = if binding.native_owner {
            None
        } else {
            callbacks.remove(&context)
        };

        removed
    };

    drop(removed);
}
