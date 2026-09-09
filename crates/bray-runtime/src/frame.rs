use std::any::Any;
use std::fmt;
use std::pin::Pin;

use bray_runtime_model::{
    ProtectedFrameDescriptor, ProtectedFrameStateDescriptor, ProtectedFrameStateId,
};

/// Runtime state supplied to one protected-frame resume operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameContext {
    cancellation_requested: bool,
}

impl FrameContext {
    /// Creates a resume context from the current run-cancellation state.
    pub const fn new(cancellation_requested: bool) -> Self {
        Self {
            cancellation_requested,
        }
    }

    /// Returns whether cancellation has been requested for the current run.
    pub const fn cancellation_requested(self) -> bool {
        self.cancellation_requested
    }
}

/// Checked protected-frame state retained while execution is suspended.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FrameSuspension {
    kind: FrameSuspensionKind,
    state: ProtectedFrameStateId,
    payload: Option<usize>,
}

/// Runtime action that caused a protected frame to suspend.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FrameSuspensionKind {
    /// The frame is waiting for a directly composed child frame.
    Awaited,
    /// The frame yielded so another ready task can run.
    Yield,
    /// The frame waits for one runtime task event.
    TaskEvent,
    /// The frame waits for the terminal state of an existing native task.
    TaskCompletion,
}

impl FrameSuspension {
    /// Creates a suspension at one descriptor-local state.
    pub const fn new(state: ProtectedFrameStateId) -> Self {
        Self {
            kind: FrameSuspensionKind::Awaited,
            state,
            payload: None,
        }
    }

    /// Creates a cooperative-yield suspension.
    pub const fn yielding(state: ProtectedFrameStateId) -> Self {
        Self {
            kind: FrameSuspensionKind::Yield,
            state,
            payload: None,
        }
    }

    /// Creates a runtime-task-event suspension.
    pub const fn task_event(state: ProtectedFrameStateId, event: usize) -> Self {
        Self {
            kind: FrameSuspensionKind::TaskEvent,
            state,
            payload: Some(event),
        }
    }

    /// Creates a suspension awaiting the terminal state of an existing native task.
    pub const fn task_completion(state: ProtectedFrameStateId, task: usize) -> Self {
        Self {
            kind: FrameSuspensionKind::TaskCompletion,
            state,
            payload: Some(task),
        }
    }

    /// Returns the runtime action that caused the suspension.
    pub const fn kind(self) -> FrameSuspensionKind {
        self.kind
    }

    /// Returns the descriptor-local suspended state.
    pub const fn state(self) -> ProtectedFrameStateId {
        self.state
    }

    /// Returns the opaque payload associated with this suspension.
    pub const fn payload(self) -> Option<usize> {
        self.payload
    }
}

/// Result of entering or resuming one protected frame.
#[derive(Debug)]
pub enum FrameProgress<T> {
    /// Execution suspended with initialized state retained by the frame.
    Suspended(FrameSuspension),
    /// Execution completed normally with its result moved out.
    Completed(T),
    /// Execution reached cancellation cleanup without a completion value.
    Cancelled,
    /// A panic crossed the protected frame boundary.
    Panicked(RuntimePanic),
    /// The frame violated its compiler/runtime execution contract.
    RuntimeFailure,
}

/// Terminal path whose retained lifecycle state must be resolved.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FrameExit {
    /// The frame completed normally.
    Completed,
    /// The frame completed cancellation cleanup.
    Cancelled,
    /// The frame terminated through panic propagation.
    Panicked,
    /// The frame violated its compiler/runtime contract.
    RuntimeFailure,
}

/// Opaque owned panic crossing a protected runtime boundary.
pub struct RuntimePanic {
    primary: RuntimePanicPayload,
    suppressed: Vec<Box<dyn Any + Send>>,
}

enum RuntimePanicPayload {
    Host(Box<dyn Any + Send>),
    NativeHandle(usize),
}

impl RuntimePanic {
    /// Creates a runtime panic from one owned payload.
    pub fn new(payload: impl Any + Send) -> Self {
        Self::from_payload(Box::new(payload))
    }

    /// Returns whether the primary panic payload has the requested Rust type.
    pub fn primary_is<T: Any>(&self) -> bool {
        let payload: &dyn Any = match &self.primary {
            RuntimePanicPayload::Host(payload) => payload.as_ref(),
            RuntimePanicPayload::NativeHandle(handle) => handle,
        };

        payload.is::<T>()
    }

    /// Returns the number of later panics retained behind the primary panic.
    pub fn suppressed_count(&self) -> usize {
        self.suppressed.len()
    }

    pub(crate) fn from_payload(payload: Box<dyn Any + Send>) -> Self {
        Self {
            primary: RuntimePanicPayload::Host(payload),
            suppressed: Vec::new(),
        }
    }

    pub(crate) fn from_native_handle(handle: usize) -> Self {
        // Native terminal storage retains the report's ownership. Rust only records its handle.
        Self {
            primary: RuntimePanicPayload::NativeHandle(handle),
            suppressed: Vec::new(),
        }
    }

    pub(crate) fn push_suppressed(&mut self, payload: Box<dyn Any + Send>) {
        self.suppressed.push(payload);
    }

    pub(crate) fn take_suppressed(&mut self) -> Vec<Box<dyn Any + Send>> {
        std::mem::take(&mut self.suppressed)
    }
}

impl fmt::Debug for RuntimePanic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimePanic")
            .field("suppressed_count", &self.suppressed.len())
            .finish_non_exhaustive()
    }
}

/// Safe in-process adapter over validated compiler-emitted frame operations.
///
/// [`ProtectedFrameDescriptor`] describes the frame's layout and scheduling requirements.
/// Implementations own their retained values and child computations while exposing
/// execution and cleanup through safe operations.
pub trait ProtectedFrame: 'static {
    /// Result moved out after normal completion.
    type Output;

    /// Returns the immutable compiler-generated descriptor.
    fn descriptor(&self) -> &ProtectedFrameDescriptor;

    /// Enters or resumes the pinned frame.
    fn resume(self: Pin<&mut Self>, context: FrameContext) -> FrameProgress<Self::Output>;

    /// Broadcasts cancellation to unresolved tasks owned by initialized state.
    fn broadcast_tasks(self: Pin<&mut Self>);

    /// Resolves retained lifecycle state after broadcast has completed.
    fn resolve_lifecycle(self: Pin<&mut Self>, exit: FrameExit);
}

/// A protected frame whose retained state may cross thread boundaries.
pub trait SendableProtectedFrame: ProtectedFrame + Send {}

impl<F> SendableProtectedFrame for F where F: ProtectedFrame + Send {}

/// Type-erased pinned protected frame with one known completion type.
pub type ErasedProtectedFrame<T> = Pin<Box<dyn ProtectedFrame<Output = T> + 'static>>;

/// Type-erased pinned frame whose retained state may cross threads.
pub type ErasedSendableProtectedFrame<T> =
    Pin<Box<dyn SendableProtectedFrame<Output = T> + 'static>>;

pub(crate) fn reserve_frame_storage<F>()
-> Result<Box<std::mem::MaybeUninit<F>>, crate::TaskStartError> {
    #[cfg(test)]
    if crate::test_support::allocation_should_fail() {
        return Err(crate::TaskStartError::AllocationFailed);
    }

    trybox::new(std::mem::MaybeUninit::uninit())
        .map_err(|_| crate::TaskStartError::AllocationFailed)
}

/// Admits stable erased storage, returning the inactive frame on allocation failure.
pub fn erase_protected_frame<F>(
    frame: F,
) -> Result<ErasedProtectedFrame<F::Output>, crate::TaskStartFailure<F>>
where
    F: ProtectedFrame,
{
    pin_protected_frame(frame).map(|frame| frame as ErasedProtectedFrame<F::Output>)
}

/// Admits sendable erased storage, returning the inactive frame on allocation failure.
pub fn erase_sendable_protected_frame<F>(
    frame: F,
) -> Result<ErasedSendableProtectedFrame<F::Output>, crate::TaskStartFailure<F>>
where
    F: SendableProtectedFrame,
{
    pin_protected_frame(frame).map(|frame| frame as ErasedSendableProtectedFrame<F::Output>)
}

fn pin_protected_frame<F: ProtectedFrame>(
    frame: F,
) -> Result<Pin<Box<F>>, crate::TaskStartFailure<F>> {
    match reserve_frame_storage() {
        Ok(storage) => Ok(Box::into_pin(Box::write(storage, frame))),
        Err(error) => Err(crate::TaskStartFailure::new(error, frame)),
    }
}

/// Resumes a directly awaited frame without creating a task-control block.
pub fn resume_direct<F>(frame: Pin<&mut F>, context: FrameContext) -> FrameProgress<F::Output>
where
    F: ?Sized + ProtectedFrame,
{
    frame.resume(context)
}

pub(crate) fn suspension_state(
    descriptor: &ProtectedFrameDescriptor,
    suspension: FrameSuspension,
) -> Option<&ProtectedFrameStateDescriptor> {
    descriptor.state(suspension.state())
}

#[cfg(test)]
mod tests {
    use std::pin::{Pin, pin};

    use bray_runtime_model::ProtectedFrameDescriptor;

    use super::{
        ErasedProtectedFrame, FrameContext, FrameExit, FrameProgress, ProtectedFrame,
        erase_protected_frame, resume_direct,
    };
    use crate::test_support::TestFrame;

    #[test]
    fn erased_frame_allocation_preserves_the_inactive_input_for_retry() {
        assert_erasure_retry(super::erase_protected_frame);
        assert_erasure_retry(super::erase_sendable_protected_frame);
    }

    fn assert_erasure_retry<F: ?Sized + ProtectedFrame<Output = i32>>(
        erase: impl Fn(TestFrame) -> Result<Pin<Box<F>>, crate::TaskStartFailure<TestFrame>>,
    ) {
        let frame = TestFrame::completing(53);
        let rejected = crate::test_support::with_allocation_failure(|| erase(frame));

        let Err(failure) = rejected else {
            panic!("injected frame allocation must fail");
        };

        assert!(matches!(
            failure.error(),
            crate::TaskStartError::AllocationFailed
        ));

        let (_, frame) = failure.into_parts();

        let mut frame = erase(frame).unwrap();

        assert!(matches!(
            resume_direct(frame.as_mut(), FrameContext::new(false)),
            FrameProgress::Completed(53)
        ));
    }

    #[test]
    fn native_panic_handles_remain_inline_while_host_payloads_keep_their_type() {
        let native = super::RuntimePanic::from_native_handle(64);

        assert!(matches!(
            native.primary,
            super::RuntimePanicPayload::NativeHandle(64)
        ));

        assert!(native.primary_is::<usize>());
        assert!(!native.primary_is::<&'static str>());
        assert_eq!(native.suppressed_count(), 0);

        let host = super::RuntimePanic::new("host panic");

        assert!(host.primary_is::<&'static str>());
        assert!(!host.primary_is::<usize>());
    }

    #[test]
    fn direct_resume_needs_no_task_storage() {
        let mut frame = pin!(TestFrame::completing(17));

        let progress = resume_direct(frame.as_mut(), FrameContext::new(false));

        assert!(matches!(progress, FrameProgress::Completed(17)));

        assert_eq!(
            frame.descriptor().frame(),
            bray_runtime_model::ProtectedAsyncFrameId::new([7; 32])
        );
    }

    #[test]
    fn direct_await_can_compose_an_erased_child_without_a_child_task() {
        let child = erase_protected_frame(TestFrame::completing(23)).unwrap();
        let descriptor = child.descriptor().clone();
        let mut parent = pin!(ParentFrame { descriptor, child });

        let progress = resume_direct(parent.as_mut(), FrameContext::new(false));

        assert!(matches!(progress, FrameProgress::Completed(23)));
    }

    struct ParentFrame {
        descriptor: ProtectedFrameDescriptor,
        child: ErasedProtectedFrame<i32>,
    }

    impl ProtectedFrame for ParentFrame {
        type Output = i32;

        fn descriptor(&self) -> &ProtectedFrameDescriptor {
            &self.descriptor
        }

        fn resume(
            mut self: std::pin::Pin<&mut Self>,
            context: FrameContext,
        ) -> FrameProgress<Self::Output> {
            self.child.as_mut().resume(context)
        }

        fn broadcast_tasks(mut self: std::pin::Pin<&mut Self>) {
            self.child.as_mut().broadcast_tasks();
        }

        fn resolve_lifecycle(mut self: std::pin::Pin<&mut Self>, exit: FrameExit) {
            self.child.as_mut().resolve_lifecycle(exit);
        }
    }
}
