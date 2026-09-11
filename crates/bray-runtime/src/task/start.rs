use std::fmt;
use std::pin::Pin;

use bray_runtime_model::ProtectedFrameDescriptor;
use triomphe::{Arc as TaskArc, UniqueArc};

use crate::frame::reserve_frame_storage;
use crate::{
    CancellationContext, ErasedProtectedFrame, ErasedSendableProtectedFrame, ProtectedFrame,
    SendableProtectedFrame, TaskControlBlock, TaskStartError,
};

/// Rejected task admission retaining the original inactive frame for retry.
pub struct TaskStartFailure<F> {
    error: TaskStartError,
    frame: F,
}

impl<F> TaskStartFailure<F> {
    pub(crate) const fn new(error: TaskStartError, frame: F) -> Self {
        Self { error, frame }
    }

    /// Returns the specific admission failure.
    pub const fn error(&self) -> &TaskStartError {
        &self.error
    }

    /// Returns the failure and the original inactive frame.
    pub fn into_parts(self) -> (TaskStartError, F) {
        (self.error, self.frame)
    }
}

impl<F> fmt::Debug for TaskStartFailure<F> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskStartFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl<T: 'static> TaskControlBlock<T> {
    /// Admits a sendable frame, retaining it unchanged if admission fails.
    pub fn start<F>(frame: F) -> Result<TaskArc<Self>, TaskStartFailure<F>>
    where
        F: SendableProtectedFrame<Output = T>,
    {
        Self::start_concrete(frame, None, |frame| frame)
    }

    /// Admits a child frame, retaining it unchanged if admission fails.
    pub fn start_child<F>(
        frame: F,
        parent: &CancellationContext,
    ) -> Result<TaskArc<Self>, TaskStartFailure<F>>
    where
        F: SendableProtectedFrame<Output = T>,
    {
        Self::start_concrete(frame, Some(parent), |frame| frame)
    }

    /// Admits an erased frame, returning the same pinned storage on failure.
    pub fn start_erased(
        frame: ErasedSendableProtectedFrame<T>,
    ) -> Result<TaskArc<Self>, TaskStartFailure<ErasedSendableProtectedFrame<T>>> {
        Self::start_pinned(frame)
    }
}

impl<T: 'static> TaskControlBlock<T, dyn ProtectedFrame<Output = T>> {
    /// Admits a thread-affine frame, retaining it unchanged if admission fails.
    pub fn start_local<F>(frame: F) -> Result<TaskArc<Self>, TaskStartFailure<F>>
    where
        F: ProtectedFrame<Output = T>,
    {
        Self::start_concrete(frame, None, |frame| frame)
    }

    /// Admits a thread-affine child, retaining its frame unchanged on failure.
    pub fn start_local_child<F>(
        frame: F,
        parent: &CancellationContext,
    ) -> Result<TaskArc<Self>, TaskStartFailure<F>>
    where
        F: ProtectedFrame<Output = T>,
    {
        Self::start_concrete(frame, Some(parent), |frame| frame)
    }

    /// Admits an erased local frame, returning the same pinned storage on failure.
    pub fn start_local_erased(
        frame: ErasedProtectedFrame<T>,
    ) -> Result<TaskArc<Self>, TaskStartFailure<ErasedProtectedFrame<T>>> {
        Self::start_pinned(frame)
    }
}

impl<T: 'static, F: ?Sized + ProtectedFrame<Output = T>> TaskControlBlock<T, F> {
    fn start_concrete<I: ProtectedFrame<Output = T>>(
        frame: I,
        parent: Option<&CancellationContext>,
        erase: impl FnOnce(Pin<Box<I>>) -> Pin<Box<F>>,
    ) -> Result<TaskArc<Self>, TaskStartFailure<I>> {
        let admission = (|| {
            let storage = reserve_frame_storage::<I>()?;
            let task = Self::reserve_task(frame.descriptor(), parent)?;

            Ok((storage, task))
        })();

        match admission {
            Ok((storage, mut task)) => {
                task.install_frame(erase(Box::into_pin(Box::write(storage, frame))));

                Ok(task.shareable())
            }
            Err(error) => Err(TaskStartFailure::new(error, frame)),
        }
    }

    fn start_pinned(frame: Pin<Box<F>>) -> Result<TaskArc<Self>, TaskStartFailure<Pin<Box<F>>>> {
        match Self::reserve_task(frame.descriptor(), None) {
            Ok(mut task) => {
                task.install_frame(frame);

                Ok(task.shareable())
            }
            Err(error) => Err(TaskStartFailure::new(error, frame)),
        }
    }

    fn reserve_task(
        descriptor: &ProtectedFrameDescriptor,
        parent: Option<&CancellationContext>,
    ) -> Result<UniqueArc<Self>, TaskStartError> {
        let cancellation = match parent {
            Some(parent) => parent.child(),
            None => CancellationContext::root(),
        }
        .map_err(TaskStartError::Cancellation)?;

        let prepared = Self::prepare(cancellation, descriptor.clone())?;

        #[cfg(test)]
        if crate::test_support::allocation_should_fail() {
            return Err(TaskStartError::AllocationFailed);
        }

        UniqueArc::try_new(prepared).map_err(|_| TaskStartError::AllocationFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::TaskStartFailure;
    use crate::test_support::{TestFrame, with_allocation_failure_after};
    use crate::{
        CancellationContext, ProtectedFrame, RunOutcome, TaskControlBlock, TaskResumeStatus,
        TaskStartError, erase_protected_frame, erase_sendable_protected_frame,
    };
    use triomphe::Arc;

    #[test]
    fn concrete_and_child_admission_preserve_rejected_frames_at_every_allocation() {
        assert_retries(
            || TestFrame::completing(37),
            |frame, _| TaskControlBlock::start(frame),
        );

        assert_retries(
            || TestFrame::completing(37),
            |frame, _| TaskControlBlock::start_local(frame),
        );

        assert_retries(
            || TestFrame::completing(37),
            |frame, parent| TaskControlBlock::start_child(frame, parent),
        );

        assert_retries(
            || TestFrame::completing(37),
            |frame, parent| TaskControlBlock::start_local_child(frame, parent),
        );
    }

    #[test]
    fn erased_admission_preserves_rejected_frames_at_every_allocation() {
        assert_retries(
            || erase_sendable_protected_frame(TestFrame::completing(37)).unwrap(),
            |frame, _| TaskControlBlock::start_erased(frame),
        );

        assert_retries(
            || erase_protected_frame(TestFrame::completing(37)).unwrap(),
            |frame, _| TaskControlBlock::start_local_erased(frame),
        );
    }

    fn assert_retries<I, F: ?Sized + ProtectedFrame<Output = i32>>(
        make_frame: impl Fn() -> I,
        start: impl Fn(
            I,
            &CancellationContext,
        ) -> Result<Arc<TaskControlBlock<i32, F>>, TaskStartFailure<I>>,
    ) {
        let mut failures = 0;

        for budget in 0..32 {
            let parent = CancellationContext::root().unwrap();
            let frame = make_frame();
            let result = with_allocation_failure_after(budget, || start(frame, &parent));

            let task = match result {
                Ok(task) => {
                    assert!(failures > 0);

                    assert!(matches!(
                        task.resume(),
                        Ok(TaskResumeStatus::Terminal(_, _))
                    ));

                    assert!(matches!(task.take_outcome(), Ok(RunOutcome::Completed(37))));

                    return;
                }
                Err(failure) => {
                    assert!(matches!(
                        failure.error(),
                        TaskStartError::AllocationFailed | TaskStartError::Cancellation(_)
                    ));

                    let (_, frame) = failure.into_parts();

                    failures += 1;

                    start(frame, &parent).unwrap()
                }
            };

            assert!(matches!(
                task.resume(),
                Ok(TaskResumeStatus::Terminal(_, _))
            ));

            assert!(matches!(task.take_outcome(), Ok(RunOutcome::Completed(37))));
        }

        panic!("task admission never succeeded after sweeping allocation failures");
    }
}
