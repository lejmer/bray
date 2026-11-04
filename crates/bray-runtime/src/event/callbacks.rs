use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, Weak};

use bray_runtime_abi::{NativeEventCallback, NativeRuntimeStatus};

use crate::native::NativeRuntimeCore;
use crate::{CancellationContext, TaskWakeHandle};

// Each native task admits one callback resource before its frame transfers.
// Source owns membership and descriptor copies. This table owns only native
// context/provider resources, which cannot be recovered from raw ABI addresses.
static CALLBACKS: LazyLock<Mutex<HashMap<usize, Binding>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

struct Resources {
    target: CancellationContext,
    wake: TaskWakeHandle,
    owner: Weak<NativeRuntimeCore>,
}

struct Binding {
    resources: Arc<Resources>,
    native_owner: bool,
    descriptors: usize,
    retained: Option<Arc<NativeRuntimeCore>>,
}

pub(crate) struct EventWake {
    identity: usize,
}

impl EventWake {
    pub(crate) fn admit(
        target: CancellationContext,
        wake: TaskWakeHandle,
        owner: &Arc<NativeRuntimeCore>,
    ) -> Result<Self, NativeRuntimeStatus> {
        let resources = Arc::new(Resources {
            target,
            wake,
            owner: Arc::downgrade(owner),
        });

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
    pub(super) fn retain(&self) -> NativeEventCallback {
        let mut callbacks = CALLBACKS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let binding = callbacks
            .get_mut(&self.identity)
            .expect("the native owner retains its event callback");

        if binding.descriptors == 0 {
            binding.retained = Some(
                binding
                    .resources
                    .owner
                    .upgrade()
                    .expect("event registration borrows its live runtime"),
            );
        }

        binding.descriptors = binding
            .descriptors
            .checked_add(1)
            .expect("retained event descriptors remain representable");

        NativeEventCallback {
            context: self.identity,
            call: invoke,
        }
    }
}

impl Drop for EventWake {
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
    let result = std::panic::catch_unwind(|| {
        if notify == 0 {
            release(context);
            return;
        }

        let (resources, retained) = {
            let callbacks = CALLBACKS
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            let binding = callbacks
                .get(&context)
                .expect("Source invokes a retained event descriptor");

            (
                Arc::clone(&binding.resources),
                Arc::clone(
                    binding
                        .retained
                        .as_ref()
                        .expect("delivery retains its runtime"),
                ),
            )
        };

        // Neither the collection lock nor the resource-table lock survives entry.
        let _target = (&resources.target, &retained);
        let _ = resources.wake.wake();
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
