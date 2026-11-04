use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, Weak};

use bray_runtime_abi::{NativeEventCallback, NativeRuntimeStatus};

use crate::native::NativeRuntimeCore;
use crate::native::RetainedRuntime;
use crate::{CancellationContext, RootCancellationHandle, TaskWakeCause, TaskWakeHandle};

// Each native task admits one callback resource before its frame transfers.
// Source owns membership and descriptor copies. This table owns only native
// context/provider resources, which cannot be recovered from raw ABI addresses.
static CALLBACKS: LazyLock<Mutex<HashMap<usize, Binding>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

enum Resources {
    Task {
        cancellation: Option<CancellationContext>,
        wake: TaskWakeHandle,
        runtime: Option<Weak<NativeRuntimeCore>>,
    },
    Timeout {
        cancellation: RootCancellationHandle,
        runtime: RetainedRuntime,
    },
}

impl Drop for Resources {
    fn drop(&mut self) {
        if let Self::Timeout { runtime, .. } = self {
            runtime.release();
        }
    }
}

struct Binding {
    resources: Arc<Resources>,
    native_owner: bool,
    descriptors: usize,
    retained: Option<Arc<NativeRuntimeCore>>,
}

pub(crate) struct NativeCallback {
    identity: usize,
}

impl NativeCallback {
    pub(crate) fn admit(
        target: CancellationContext,
        wake: TaskWakeHandle,
        owner: &Arc<NativeRuntimeCore>,
    ) -> Result<Self, NativeRuntimeStatus> {
        Self::insert(Resources::Task {
            cancellation: Some(target),
            wake,
            runtime: Some(Arc::downgrade(owner)),
        })
    }

    pub(crate) fn task(
        cancellation: Option<CancellationContext>,
        wake: TaskWakeHandle,
    ) -> Result<Self, NativeRuntimeStatus> {
        Self::insert(Resources::Task { cancellation, wake, runtime: None })
    }

    pub(crate) fn timeout(
        cancellation: RootCancellationHandle,
        runtime: RetainedRuntime,
    ) -> Result<Self, NativeRuntimeStatus> {
        Self::insert(Resources::Timeout { cancellation, runtime })
    }

    fn insert(resources: Resources) -> Result<Self, NativeRuntimeStatus> {
        let resources = Arc::new(resources);

        let identity = Arc::as_ptr(&resources) as usize;

        let binding = Binding {
            resources,
            native_owner: true,
            descriptors: 0,
            retained: None,
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

    fn retain_with(&self, call: extern "C" fn(usize, u32) -> NativeRuntimeStatus) -> NativeEventCallback {
        let mut callbacks = CALLBACKS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let binding = callbacks
            .get_mut(&self.identity)
            .expect("the native owner retains its event callback");

        if binding.descriptors == 0
            && let Resources::Task { runtime: Some(runtime), .. } = binding.resources.as_ref()
        {
            binding.retained = Some(runtime.upgrade().expect("registration borrows its live runtime"));
        }

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

        let (resources, retained) = {
            let callbacks = CALLBACKS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let binding = callbacks.get(&context).expect("Source invokes a retained callback descriptor");

            (Arc::clone(&binding.resources), binding.retained.clone())
        };

        // Neither the collection lock nor the resource-table lock survives entry.
        let _retained = retained;

        match resources.as_ref() {
            Resources::Task { cancellation, wake, .. } => {
                let _cancellation = cancellation;
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
    let (retained, removed) = {
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

        let retained = binding.retained.take();

        let removed = if binding.native_owner {
            None
        } else {
            callbacks.remove(&context)
        };

        (retained, removed)
    };

    drop((retained, removed));
}
