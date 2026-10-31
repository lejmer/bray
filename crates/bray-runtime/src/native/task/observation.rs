use bray_runtime_abi::{NativeRuntimeStatus, NativeTaskHandle};

use super::super::state::{current_reference_address, with_runtime};

/// Borrows execution metadata for the protected observation producer. The active
/// native callback retains its cached service owner through every nested call.
/// Retirement only requests the existing Bray transition and transports its wake.
/// The bridge neither drives a frame nor changes source resolution ownership.
pub(in crate::native) fn observation_service(
    task: NativeTaskHandle,
    operation: u32,
    output: &mut u64,
) -> NativeRuntimeStatus {
    match operation {
        0 => with_runtime(|runtime| {
            runtime.retained_task(task).map_or_else(|status| status, |_| {
                *output = runtime.identity;

                NativeRuntimeStatus::SUCCESS
            })
        }).unwrap_or_else(|status| status),
        1 => {
            let Some(parent) = current_reference_address() else {
                return NativeRuntimeStatus::NOT_INITIALIZED;
            };

            // The TLS owner lends this reference until its enclosing producer
            // callback returns. Reading the address needs no additional Arc.
            *output = parent;

            NativeRuntimeStatus::SUCCESS
        }
        2 => with_runtime(|runtime| runtime.retire_task(task)).unwrap_or_else(|status| status),
        _ => panic!("unknown observation context operation {operation}"),
    }
}
