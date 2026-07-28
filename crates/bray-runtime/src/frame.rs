use std::any::Any;
use std::fmt;
use std::pin::Pin;

use bray_runtime_interface::{
    ProtectedFrameDescriptor, ProtectedFrameStateDescriptor, ProtectedFrameStateId,
};

/// Runtime facts supplied to one protected-frame resume operation.
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
    state: ProtectedFrameStateId,
}

impl FrameSuspension {
    /// Creates a suspension at one descriptor-local state.
    pub const fn new(state: ProtectedFrameStateId) -> Self {
        Self { state }
    }

    /// Returns the descriptor-local suspended state.
    pub const fn state(self) -> ProtectedFrameStateId {
        self.state
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
    primary: Box<dyn Any + Send>,
    suppressed: Vec<Box<dyn Any + Send>>,
}

impl RuntimePanic {
    /// Creates a runtime panic from one owned payload.
    pub fn new(payload: impl Any + Send) -> Self {
        Self::from_payload(Box::new(payload))
    }

    /// Returns whether the primary panic payload has the requested Rust type.
    pub fn primary_is<T: Any>(&self) -> bool {
        self.primary.is::<T>()
    }

    /// Returns the number of later panics retained behind the primary panic.
    pub fn suppressed_count(&self) -> usize {
        self.suppressed.len()
    }

    pub(crate) fn from_payload(payload: Box<dyn Any + Send>) -> Self {
        Self {
            primary: payload,
            suppressed: Vec::new(),
        }
    }

    pub(crate) fn push_suppressed(&mut self, payload: Box<dyn Any + Send>) {
        self.suppressed.push(payload);
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
/// The binary operation identities and retained-state contract live in
/// [`ProtectedFrameDescriptor`]. Implementations own their retained values and
/// child computations while exposing those operations without unsafe code.
pub trait ProtectedFrame: 'static {
    /// Result moved out after normal completion.
    type Output;

    /// Returns the immutable compiler-generated descriptor.
    fn descriptor(&self) -> &ProtectedFrameDescriptor;

    /// Enters or resumes the pinned frame.
    fn resume(
        self: Pin<&mut Self>,
        context: FrameContext,
    ) -> FrameProgress<Self::Output>;

    /// Broadcasts cancellation to unresolved tasks owned by initialized state.
    fn broadcast_tasks(self: Pin<&mut Self>);

    /// Resolves retained lifecycle state after broadcast has completed.
    fn resolve_lifecycle(self: Pin<&mut Self>, exit: FrameExit);
}

/// A protected frame whose retained state may cross thread boundaries.
pub trait SendableProtectedFrame: ProtectedFrame + Send {}

impl<F> SendableProtectedFrame for F where F: ProtectedFrame + Send {}

/// Type-erased pinned protected frame with one known completion type.
pub type ErasedProtectedFrame<T> =
    Pin<Box<dyn ProtectedFrame<Output = T> + 'static>>;

/// Type-erased pinned frame whose retained state may cross threads.
pub type ErasedSendableProtectedFrame<T> =
    Pin<Box<dyn SendableProtectedFrame<Output = T> + 'static>>;

/// Moves a concrete inactive frame into stable erased storage.
pub fn erase_protected_frame<F>(frame: F) -> ErasedProtectedFrame<F::Output>
where
    F: ProtectedFrame,
{
    Box::pin(frame)
}

/// Moves a sendable inactive frame into stable erased storage.
pub fn erase_sendable_protected_frame<F>(
    frame: F,
) -> ErasedSendableProtectedFrame<F::Output>
where
    F: SendableProtectedFrame,
{
    Box::pin(frame)
}

/// Resumes a directly awaited frame without creating a task-control block.
pub fn resume_direct<F>(
    frame: Pin<&mut F>,
    context: FrameContext,
) -> FrameProgress<F::Output>
where
    F: ProtectedFrame,
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
    use std::pin::pin;

    use bray_runtime_interface::ProtectedFrameDescriptor;

    use super::{
        ErasedProtectedFrame, FrameContext, FrameExit, FrameProgress, ProtectedFrame,
        erase_protected_frame, resume_direct,
    };
    use crate::test_support::TestFrame;

    #[test]
    fn direct_resume_needs_no_task_storage() {
        let mut frame = pin!(TestFrame::completing(17));

        let progress = resume_direct(frame.as_mut(), FrameContext::new(false));

        assert!(matches!(progress, FrameProgress::Completed(17)));

        assert_eq!(
            frame.descriptor().frame(),
            bray_runtime_interface::ProtectedAsyncFrameId::new([7; 32])
        );
    }

    #[test]
    fn direct_await_can_compose_an_erased_child_without_a_child_task() {
        let child = erase_protected_frame(TestFrame::completing(23));
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

        fn resolve_lifecycle(
            mut self: std::pin::Pin<&mut Self>,
            exit: FrameExit,
        ) {
            self.child.as_mut().resolve_lifecycle(exit);
        }
    }
}
