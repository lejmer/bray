use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::{
    NativeActivationInvocation, NativeRuntimeStatus, NativeSchedulerLane, NativeTaskReference,
};
use bray_runtime_model::ProtectedFrameStateId;

use crate::event::{NativeCallback, Resources};
use crate::TaskExecutionContext;

/// Transfers one Source storage reference into its native callback owner.
pub extern "C" fn bray_runtime_execution_context_admission(
    reference: NativeTaskReference,
    root: u32,
) -> u32 {
    assert!(root <= 1, "Source admission identifies a root or child call");

    catch_unwind(AssertUnwindSafe(|| {
        let cancellation = crate::CancellationContext::native(reference);
        let output_context = if root != 0 {
            super::super::host::with_output(crate::context::current_task_output)
        } else {
            crate::context::current_task_output()
        };

        let callback = match NativeCallback::admit(cancellation, output_context) {
            Ok(callback) => callback,
            Err(status) => return status.code(),
        };

        if root != 0 {
            super::super::host::register_timeout(callback.root_cancellation());
        }

        callback.publish();

        NativeRuntimeStatus::SUCCESS.code()
    }))
    .unwrap_or(NativeRuntimeStatus::PANICKED.code())
}

/// Retires the native owner after Source has withdrawn its task-cell wait.
pub extern "C" fn bray_runtime_execution_context_release(context: usize) {
    NativeCallback::release_owner(context);
}

/// Borrows the complete producer invocation and its admitted native task owner.
pub extern "C" fn bray_runtime_execution_invocation(
    invocation: &mut NativeActivationInvocation<'_>,
    metadata: usize,
    lane: NativeSchedulerLane,
) {
    let owner = (metadata != 0).then(|| NativeCallback::resources(metadata));

    crate::context::with_independent_execution_context(|| {
        if invocation.operation.is_resumable() {
            let owner = owner.expect("resumable native callbacks retain their admitted task");
            let context = TaskExecutionContext::source(
                owner,
                ProtectedFrameStateId::new(invocation.execution.state),
                crate::scheduler::native::from_native(lane),
            );

            crate::context::with_task_execution_context(context, || {
                super::super::activation::invoke_producer(invocation);
            });
        } else if let Some(owner) = owner.as_ref() {
            with_disposal_resources(owner, || super::super::activation::invoke_producer(invocation));
        } else {
            super::super::host::with_output(|| super::super::activation::invoke_producer(invocation));
        }
    });
}

fn with_disposal_resources<T>(owner: &Resources, operation: impl FnOnce() -> T) -> T {
    crate::context::with_run_cancellation_context(owner.cancellation().clone(), || {
        #[cfg(feature = "test-output")]
        {
            crate::context::with_task_output(owner.output().clone(), operation)
        }

        #[cfg(not(feature = "test-output"))]
        {
            operation()
        }
    })
}
