use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::rc::Rc;

use bray_runtime_abi::NativeRuntimeStatus;

#[expect(
    unsafe_code,
    reason = "Source initializes and consumes its complete entry scope in native-owned storage"
)]
unsafe extern "C" {
    safe fn bray_runtime_execution_scope_enter(
        storage: *mut u8,
        bytes: usize,
        align: usize,
        isolate: u32,
    ) -> NativeRuntimeStatus;
    safe fn bray_runtime_execution_scope_finish(storage: *mut u8) -> NativeRuntimeStatus;
}

// The private boundary checks Source's actual layout before changing ownership.
// Native code never interprets these bytes. In particular, it does not mirror
// nullable Source owners or rebuild a context from a task and lane snapshot.
#[repr(C, align(16))]
struct Storage([MaybeUninit<u8>; 128]);

/// Owns the complete displaced Source context through a native call and unwind.
/// The attachment and its nesting order make this owner thread-bound.
pub(super) struct Scope {
    storage: Storage,
    active: bool,
    _thread: PhantomData<Rc<()>>,
}

impl Scope {
    pub(super) fn enter(isolate: bool) -> Result<Self, NativeRuntimeStatus> {
        let mut scope = Self {
            storage: Storage([MaybeUninit::uninit(); 128]),
            active: false,
            _thread: PhantomData,
        };

        let status = bray_runtime_execution_scope_enter(
            scope.storage.0.as_mut_ptr().cast(),
            size_of::<Storage>(),
            align_of::<Storage>(),
            u32::from(isolate),
        );

        // The entry always commits its scope, including an entry that could not
        // isolate. Retire that owner before returning a failed acquisition.
        scope.active = true;

        if !status.is_success() {
            let retired = scope.retire();

            if !retired.is_success() && retired != status {
                super::host::record_cleanup_failure(1);
            }

            return Err(status);
        }

        Ok(scope)
    }

    pub(super) fn finish(mut self) -> NativeRuntimeStatus {
        self.retire()
    }

    fn retire(&mut self) -> NativeRuntimeStatus {
        if !std::mem::replace(&mut self.active, false) {
            return NativeRuntimeStatus::SUCCESS;
        }

        bray_runtime_execution_scope_finish(self.storage.0.as_mut_ptr().cast())
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        if !self.retire().is_success() {
            super::host::record_cleanup_failure(1);
        }
    }
}
