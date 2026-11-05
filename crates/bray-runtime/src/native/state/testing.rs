use std::cell::Cell;
use std::sync::{Mutex, MutexGuard};

static ISOLATION: Mutex<()> = Mutex::new(());
thread_local! {
    static HOLDS_ISOLATION: Cell<bool> = const { Cell::new(false) };
}

pub(crate) struct TestRuntimeIsolation {
    _guard: MutexGuard<'static, ()>,
}

impl Drop for TestRuntimeIsolation {
    fn drop(&mut self) {
        HOLDS_ISOLATION.set(false);
    }
}

pub(crate) fn test_runtime_isolation() -> Option<TestRuntimeIsolation> {
    if HOLDS_ISOLATION.get() || crate::context::current_task_execution_lane().is_some() {
        return None;
    }

    let guard = ISOLATION.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

    HOLDS_ISOLATION.set(true);

    Some(TestRuntimeIsolation { _guard: guard })
}

#[expect(unsafe_code, reason = "tests initialize the same Source execution provider as native consumers")]
unsafe extern "C" {
    safe fn bray_runtime_execution_startup(configuration: bray_runtime_abi::NativeRuntimeConfiguration) -> bray_runtime_abi::NativeRuntimeStatus;
}

pub(in crate::native) fn initialize(configuration: bray_runtime_abi::NativeRuntimeConfiguration) -> bray_runtime_abi::NativeRuntimeStatus {
    bray_runtime_execution_startup(super::native_configuration(configuration))
}
