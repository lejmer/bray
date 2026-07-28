use std::num::NonZeroU64;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use bray_runtime_interface::{ProtectedFrameDescriptor, ProtectedFrameStateId};

use crate::frame::suspension_state;
use crate::{
    ErasedProtectedFrame, ErasedSendableProtectedFrame, FrameContext, FrameExit,
    FrameProgress, FrameSuspension, ProtectedFrame, RunOutcome, RunOutcomeKind,
    RuntimePanic, SendableProtectedFrame, erase_protected_frame,
    erase_sendable_protected_frame,
};

static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);

/// Process-local identity of one task-control block.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TaskId(NonZeroU64);

impl TaskId {
    /// Returns the process-local numeric identity.
    pub const fn raw(self) -> u64 {
        self.0.get()
    }
}

/// Observable task-control-block execution state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TaskState {
    /// The frame has not yet entered.
    Ready,
    /// The frame is currently executing.
    Running,
    /// The frame retained one checked suspension state.
    Suspended(ProtectedFrameStateId),
    /// The frame completed normally.
    Completed,
    /// The frame completed cancellation cleanup.
    Cancelled,
    /// The frame terminated through panic.
    Panicked,
    /// The compiler/runtime frame contract was violated.
    Failed(TaskFailureKind),
}

impl TaskState {
    const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Cancelled | Self::Panicked | Self::Failed(_)
        )
    }
}

/// Terminal runtime-contract failure retained by one task.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TaskFailureKind {
    /// The frame suspended with a state absent from its descriptor.
    UnknownSuspensionState(ProtectedFrameStateId),
    /// The task lost its executable frame before reaching a terminal state.
    MissingFrame,
}

/// Result of one successful task resume.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TaskResumeStatus {
    /// The frame suspended in one checked state.
    Suspended(FrameSuspension),
    /// The task published one terminal outcome.
    Terminal(RunOutcomeKind),
}

/// A task-control-block operation that cannot safely resume its frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskResumeError {
    /// Another thread is already resuming this task.
    AlreadyRunning,
    /// The task is not in a resumable state.
    NotResumable(TaskState),
    /// The frame suspended with a state absent from its descriptor.
    UnknownSuspensionState(ProtectedFrameStateId),
    /// Internal task state was poisoned by an unexpected runtime panic.
    SynchronizationPoisoned,
}

/// Failure to create stable task-owned storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskStartError {
    /// Process-local task identities were exhausted.
    IdentityExhausted,
}

/// Failure to observe or register observation of one task outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskObservationError {
    /// The task has not reached a terminal state.
    Pending,
    /// The terminal outcome was already moved to its observer.
    AlreadyObserved,
    /// The task failed its compiler/runtime frame contract.
    RuntimeFailed(TaskFailureKind),
    /// Internal task state was poisoned by an unexpected runtime panic.
    SynchronizationPoisoned,
}

/// Infallible notification used when a task becomes observable.
pub trait JoinWake: Send + Sync + 'static {
    /// Makes the registered observer runnable.
    fn wake(&self);
}

impl<F> JoinWake for F
where
    F: Fn() + Send + Sync + 'static,
{
    fn wake(&self) {
        self();
    }
}

struct TaskData<T, F>
where
    F: ?Sized + ProtectedFrame<Output = T>,
{
    frame: Option<Pin<Box<F>>>,
    state: TaskState,
    outcome: Option<RunOutcome<T>>,
    join_waiters: Vec<Arc<dyn JoinWake>>,
}

/// Stable runtime-owned storage for one independently executing task.
pub struct TaskControlBlock<
    T,
    F: ?Sized + ProtectedFrame<Output = T> = dyn SendableProtectedFrame<Output = T>,
> {
    id: TaskId,
    descriptor: ProtectedFrameDescriptor,
    data: Mutex<TaskData<T, F>>,
    cancellation_requested: AtomicBool,
    resuming: AtomicBool,
}

impl<T: 'static> TaskControlBlock<T> {
    /// Moves a concrete inactive frame into stable task-owned storage.
    pub fn start<F>(frame: F) -> Result<Arc<Self>, TaskStartError>
    where
        F: SendableProtectedFrame<Output = T>,
    {
        Self::start_erased(erase_sendable_protected_frame(frame))
    }

    /// Moves an erased inactive frame into stable task-owned storage.
    pub fn start_erased(
        frame: ErasedSendableProtectedFrame<T>,
    ) -> Result<Arc<Self>, TaskStartError> {
        Self::start_frame(frame)
    }
}

impl<T: 'static> TaskControlBlock<T, dyn ProtectedFrame<Output = T>> {
    /// Moves a thread-affine inactive frame into local task-owned storage.
    pub fn start_local<F>(frame: F) -> Result<Arc<Self>, TaskStartError>
    where
        F: ProtectedFrame<Output = T>,
    {
        Self::start_local_erased(erase_protected_frame(frame))
    }

    /// Moves an erased thread-affine frame into local task-owned storage.
    pub fn start_local_erased(
        frame: ErasedProtectedFrame<T>,
    ) -> Result<Arc<Self>, TaskStartError> {
        Self::start_frame(frame)
    }
}

impl<T: 'static, F> TaskControlBlock<T, F>
where
    F: ?Sized + ProtectedFrame<Output = T>,
{
    fn start_frame(frame: Pin<Box<F>>) -> Result<Arc<Self>, TaskStartError> {
        let id = next_task_id()?;
        let descriptor = frame.descriptor().clone();

        Ok(Arc::new(Self {
            id,
            descriptor,
            data: Mutex::new(TaskData {
                frame: Some(frame),
                state: TaskState::Ready,
                outcome: None,
                join_waiters: Vec::new(),
            }),
            cancellation_requested: AtomicBool::new(false),
            resuming: AtomicBool::new(false),
        }))
    }

    /// Returns the process-local task identity.
    pub const fn id(&self) -> TaskId {
        self.id
    }

    /// Returns the immutable compiler-generated frame descriptor.
    pub const fn descriptor(&self) -> &ProtectedFrameDescriptor {
        &self.descriptor
    }

    /// Returns the current task state.
    pub fn state(&self) -> Result<TaskState, TaskResumeError> {
        self.data
            .lock()
            .map(|data| data.state)
            .map_err(|_| TaskResumeError::SynchronizationPoisoned)
    }

    /// Atomically records a cancellation request.
    ///
    /// Returns whether this call changed the request state.
    pub fn request_cancellation(&self) -> bool {
        !self.cancellation_requested.swap(true, Ordering::AcqRel)
    }

    /// Returns whether cancellation has been requested.
    pub fn cancellation_requested(&self) -> bool {
        self.cancellation_requested.load(Ordering::Acquire)
    }

    /// Enters or resumes the task without allowing concurrent execution.
    pub fn resume(&self) -> Result<TaskResumeStatus, TaskResumeError> {
        let Some(_resume) = ResumeGuard::acquire(&self.resuming) else {
            return Err(TaskResumeError::AlreadyRunning);
        };

        let mut data = self.lock_data()?;

        if !matches!(data.state, TaskState::Ready | TaskState::Suspended(_)) {
            return Err(TaskResumeError::NotResumable(data.state));
        }

        data.state = TaskState::Running;

        let context = FrameContext::new(self.cancellation_requested());

        let progress = {
            let Some(frame) = data.frame.as_mut() else {
                let failure = TaskFailureKind::MissingFrame;

                data.state = TaskState::Failed(failure);

                return Err(TaskResumeError::NotResumable(TaskState::Failed(
                    failure,
                )));
            };

            catch_unwind(AssertUnwindSafe(|| frame.as_mut().resume(context)))
                .unwrap_or_else(|payload| {
                    FrameProgress::Panicked(RuntimePanic::from_payload(payload))
                })
        };

        let (status, waiters) = match progress {
            FrameProgress::Suspended(suspension) => {
                if suspension_state(&self.descriptor, suspension).is_none() {
                    let failure =
                        TaskFailureKind::UnknownSuspensionState(suspension.state());

                    if let Some(frame) = data.frame.as_mut() {
                        fail_frame(frame.as_mut());
                    }

                    destroy_failed_frame(data.frame.take());
                    data.state = TaskState::Failed(failure);

                    let waiters = std::mem::take(&mut data.join_waiters);

                    drop(data);

                    wake_all(waiters);

                    return Err(TaskResumeError::UnknownSuspensionState(
                        suspension.state(),
                    ));
                }

                data.state = TaskState::Suspended(suspension.state());

                (TaskResumeStatus::Suspended(suspension), Vec::new())
            }
            terminal => {
                let Some(frame) = data.frame.as_mut() else {
                    let failure = TaskFailureKind::MissingFrame;

                    data.state = TaskState::Failed(failure);

                    return Err(TaskResumeError::NotResumable(TaskState::Failed(
                        failure,
                    )));
                };

                let outcome = finish_frame(frame.as_mut(), terminal);
                let outcome = destroy_frame(data.frame.take(), outcome);
                let kind = outcome.kind();

                data.state = task_state(kind);
                data.outcome = Some(outcome);

                let waiters = std::mem::take(&mut data.join_waiters);

                (TaskResumeStatus::Terminal(kind), waiters)
            }
        };

        drop(data);

        wake_all(waiters);

        Ok(status)
    }

    /// Registers an observer to wake when this task becomes terminal.
    pub fn register_join_waiter(
        &self,
        waiter: Arc<dyn JoinWake>,
    ) -> Result<(), TaskObservationError> {
        let wake_now = {
            let mut data = self
                .data
                .lock()
                .map_err(|_| TaskObservationError::SynchronizationPoisoned)?;

            if data.state.is_terminal() {
                Some(waiter)
            } else {
                data.join_waiters.push(waiter);

                None
            }
        };

        if let Some(waiter) = wake_now {
            waiter.wake();
        }

        Ok(())
    }

    /// Moves the terminal run outcome to its single observer.
    pub fn take_outcome(&self) -> Result<RunOutcome<T>, TaskObservationError> {
        let mut data = self
            .data
            .lock()
            .map_err(|_| TaskObservationError::SynchronizationPoisoned)?;

        match data.state {
            TaskState::Ready
            | TaskState::Running
            | TaskState::Suspended(_) => Err(TaskObservationError::Pending),
            TaskState::Failed(failure) => {
                Err(TaskObservationError::RuntimeFailed(failure))
            }
            TaskState::Completed | TaskState::Cancelled | TaskState::Panicked => data
                .outcome
                .take()
                .ok_or(TaskObservationError::AlreadyObserved),
        }
    }

    /// Resolves the terminal outcome when no ordinary join observer consumes it.
    pub fn resolve_unobserved(
        &self,
        resolver: impl FnOnce(RunOutcome<T>),
    ) -> Result<(), TaskObservationError> {
        let outcome = self.take_outcome()?;

        resolver(outcome);

        Ok(())
    }

    /// Returns whether a terminal outcome still belongs to this task storage.
    pub fn has_unobserved_outcome(&self) -> Result<bool, TaskObservationError> {
        self.data
            .lock()
            .map(|data| data.outcome.is_some())
            .map_err(|_| TaskObservationError::SynchronizationPoisoned)
    }

    fn lock_data(
        &self,
    ) -> Result<MutexGuard<'_, TaskData<T, F>>, TaskResumeError> {
        self.data
            .lock()
            .map_err(|_| TaskResumeError::SynchronizationPoisoned)
    }
}

struct ResumeGuard<'task>(&'task AtomicBool);

impl<'task> ResumeGuard<'task> {
    fn acquire(resuming: &'task AtomicBool) -> Option<Self> {
        resuming
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| Self(resuming))
    }
}

impl Drop for ResumeGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn next_task_id() -> Result<TaskId, TaskStartError> {
    let id = NEXT_TASK_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .map_err(|_| TaskStartError::IdentityExhausted)?;

    NonZeroU64::new(id)
        .map(TaskId)
        .ok_or(TaskStartError::IdentityExhausted)
}

fn finish_frame<T: 'static, F>(
    mut frame: Pin<&mut F>,
    progress: FrameProgress<T>,
) -> RunOutcome<T>
where
    F: ?Sized + ProtectedFrame<Output = T>,
{
    let (mut outcome, mut exit) = match progress {
        FrameProgress::Suspended(_) => {
            unreachable!("suspended frames are not terminalized")
        }
        FrameProgress::Completed(value) => {
            (RunOutcome::Completed(value), FrameExit::Completed)
        }
        FrameProgress::Cancelled => (RunOutcome::Cancelled, FrameExit::Cancelled),
        FrameProgress::Panicked(panic) => {
            (RunOutcome::Panicked(panic), FrameExit::Panicked)
        }
    };

    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| {
        frame.as_mut().broadcast_tasks();
    })) {
        merge_panic(&mut outcome, payload);
        exit = FrameExit::Panicked;
    }

    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| {
        frame.as_mut().resolve_lifecycle(exit);
    })) {
        merge_panic(&mut outcome, payload);
    }

    outcome
}

fn destroy_frame<T: 'static, F>(
    frame: Option<Pin<Box<F>>>,
    mut outcome: RunOutcome<T>,
) -> RunOutcome<T>
where
    F: ?Sized + ProtectedFrame<Output = T>,
{
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(frame))) {
        merge_panic(&mut outcome, payload);
    }

    outcome
}

fn fail_frame<T, F>(mut frame: Pin<&mut F>)
where
    F: ?Sized + ProtectedFrame<Output = T>,
{
    let _ = catch_unwind(AssertUnwindSafe(|| {
        frame.as_mut().broadcast_tasks();
    }));

    let _ = catch_unwind(AssertUnwindSafe(|| {
        frame.as_mut().resolve_lifecycle(FrameExit::RuntimeFailure);
    }));
}

fn destroy_failed_frame<T, F>(frame: Option<Pin<Box<F>>>)
where
    F: ?Sized + ProtectedFrame<Output = T>,
{
    let _ = catch_unwind(AssertUnwindSafe(|| drop(frame)));
}

fn merge_panic<T>(
    outcome: &mut RunOutcome<T>,
    payload: Box<dyn std::any::Any + Send>,
) {
    match outcome {
        RunOutcome::Panicked(panic) => panic.push_suppressed(payload),
        RunOutcome::Completed(_) | RunOutcome::Cancelled => {
            *outcome = RunOutcome::Panicked(RuntimePanic::from_payload(payload));
        }
    }
}

const fn task_state(kind: RunOutcomeKind) -> TaskState {
    match kind {
        RunOutcomeKind::Completed => TaskState::Completed,
        RunOutcomeKind::Cancelled => TaskState::Cancelled,
        RunOutcomeKind::Panicked => TaskState::Panicked,
    }
}

fn wake_all(waiters: Vec<Arc<dyn JoinWake>>) {
    for waiter in waiters {
        waiter.wake();
    }
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;
    use std::rc::Rc;
    use std::sync::{Arc, Barrier};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    use super::{
        TaskControlBlock, TaskFailureKind, TaskObservationError,
        TaskResumeError, TaskResumeStatus, TaskState,
    };
    use crate::test_support::TestFrame;
    use crate::{
        FrameContext, FrameExit, FrameProgress, ProtectedFrame, RunOutcome,
        erase_sendable_protected_frame,
    };

    #[test]
    fn starting_moves_the_frame_into_stable_task_owned_storage() {
        let task = TaskControlBlock::start(TestFrame::suspending_then_completing(29))
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        assert_eq!(task.state(), Ok(TaskState::Ready));

        assert_eq!(
            task.resume(),
            Ok(TaskResumeStatus::Suspended(
                crate::FrameSuspension::new(
                    bray_runtime_interface::ProtectedFrameStateId::new(1)
                )
            ))
        );

        assert_eq!(
            task.state(),
            Ok(TaskState::Suspended(
                bray_runtime_interface::ProtectedFrameStateId::new(1)
            ))
        );

        assert_eq!(
            task.resume(),
            Ok(TaskResumeStatus::Terminal(
                crate::RunOutcomeKind::Completed
            ))
        );

        assert!(task.has_unobserved_outcome().unwrap_or(false));

        let outcome = task
            .take_outcome()
            .unwrap_or_else(|error| panic!("terminal outcome must be observable: {error:?}"));

        assert!(matches!(outcome, RunOutcome::Completed(29)));

        assert!(matches!(
            task.take_outcome(),
            Err(TaskObservationError::AlreadyObserved)
        ));
    }

    #[test]
    fn erased_inactive_frames_use_the_same_task_storage_contract() {
        let frame = erase_sendable_protected_frame(TestFrame::completing(41));

        let task = TaskControlBlock::start_erased(frame)
            .unwrap_or_else(|error| panic!("erased test task must start: {error:?}"));

        assert_eq!(
            task.resume(),
            Ok(TaskResumeStatus::Terminal(
                crate::RunOutcomeKind::Completed
            ))
        );

        assert!(matches!(task.take_outcome(), Ok(RunOutcome::Completed(41))));
    }

    #[test]
    fn local_task_storage_accepts_thread_affine_frames() {
        let frame = LocalFrame {
            inner: TestFrame::completing(43),
            _thread_affinity: Rc::new(()),
        };

        let task = TaskControlBlock::start_local(frame)
            .unwrap_or_else(|error| panic!("local test task must start: {error:?}"));

        assert_eq!(
            task.resume(),
            Ok(TaskResumeStatus::Terminal(
                crate::RunOutcomeKind::Completed
            ))
        );

        assert!(matches!(task.take_outcome(), Ok(RunOutcome::Completed(43))));
    }

    #[test]
    fn cancellation_is_observed_by_frames_and_terminal_waiters() {
        let task = TaskControlBlock::start(TestFrame::cancellation_aware())
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        let wake_count = std::sync::Arc::new(AtomicUsize::new(0));

        let waiter_count = std::sync::Arc::clone(&wake_count);

        task.register_join_waiter(std::sync::Arc::new(move || {
            waiter_count.fetch_add(1, Ordering::Relaxed);
        }))
        .unwrap_or_else(|error| panic!("join waiter must register: {error:?}"));

        assert!(task.request_cancellation());
        assert!(!task.request_cancellation());

        assert_eq!(
            task.resume(),
            Ok(TaskResumeStatus::Terminal(
                crate::RunOutcomeKind::Cancelled
            ))
        );

        assert_eq!(wake_count.load(Ordering::Relaxed), 1);
        assert!(matches!(task.take_outcome(), Ok(RunOutcome::Cancelled)));
    }

    #[test]
    fn one_task_is_never_resumed_concurrently() {
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));

        let task = TaskControlBlock::start(TestFrame::blocking(
            Arc::clone(&entered),
            Arc::clone(&release),
            5,
        ))
        .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        let worker_task = Arc::clone(&task);

        let worker = thread::spawn(move || worker_task.resume());

        entered.wait();

        assert_eq!(task.resume(), Err(TaskResumeError::AlreadyRunning));

        release.wait();

        assert_eq!(
            worker
                .join()
                .unwrap_or_else(|_| panic!("resume worker must not panic")),
            Ok(TaskResumeStatus::Terminal(
                crate::RunOutcomeKind::Completed
            ))
        );
    }

    #[test]
    fn panics_are_captured_and_later_cleanup_panics_are_suppressed() {
        let task = TaskControlBlock::start(TestFrame::panicking_with_cleanup_panic())
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        assert_eq!(
            task.resume(),
            Ok(TaskResumeStatus::Terminal(
                crate::RunOutcomeKind::Panicked
            ))
        );

        let outcome = task
            .take_outcome()
            .unwrap_or_else(|error| panic!("panic outcome must be observable: {error:?}"));

        let RunOutcome::Panicked(panic) = outcome else {
            panic!("task must preserve a panic outcome");
        };

        assert!(panic.primary_is::<&'static str>());
        assert_eq!(panic.suppressed_count(), 1);
    }

    #[test]
    fn invalid_suspension_state_fails_without_resuming_again() {
        let task = TaskControlBlock::start(TestFrame::invalid_suspension())
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        let wake_count = Arc::new(AtomicUsize::new(0));
        let waiter_count = Arc::clone(&wake_count);

        task.register_join_waiter(Arc::new(move || {
            waiter_count.fetch_add(1, Ordering::Relaxed);
        }))
        .unwrap_or_else(|error| panic!("join waiter must register: {error:?}"));

        let failure = TaskFailureKind::UnknownSuspensionState(
            bray_runtime_interface::ProtectedFrameStateId::new(9),
        );

        assert_eq!(
            task.resume(),
            Err(TaskResumeError::UnknownSuspensionState(
                bray_runtime_interface::ProtectedFrameStateId::new(9)
            ))
        );

        assert_eq!(task.state(), Ok(TaskState::Failed(failure)));
        assert_eq!(wake_count.load(Ordering::Relaxed), 1);

        assert_eq!(
            task.resume(),
            Err(TaskResumeError::NotResumable(TaskState::Failed(
                failure
            )))
        );

        assert!(matches!(
            task.take_outcome(),
            Err(TaskObservationError::RuntimeFailed(found)) if found == failure
        ));
    }

    #[test]
    fn observation_before_terminal_state_is_rejected() {
        let task = TaskControlBlock::start(TestFrame::completing(1))
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        assert!(matches!(
            task.take_outcome(),
            Err(TaskObservationError::Pending)
        ));
    }

    #[test]
    fn unobserved_outcomes_follow_the_explicit_resolution_path() {
        let task = TaskControlBlock::start(TestFrame::completing(47))
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        task.resume()
            .unwrap_or_else(|error| panic!("test task must complete: {error:?}"));

        let resolved = std::cell::Cell::new(None);

        task.resolve_unobserved(|outcome| {
            let RunOutcome::Completed(value) = outcome else {
                panic!("test outcome must complete");
            };

            resolved.set(Some(value));
        })
            .unwrap_or_else(|error| panic!("test outcome must resolve: {error:?}"));

        assert_eq!(resolved.get(), Some(47));

        assert!(matches!(
            task.take_outcome(),
            Err(TaskObservationError::AlreadyObserved)
        ));
    }

    struct LocalFrame {
        inner: TestFrame,
        _thread_affinity: Rc<()>,
    }

    impl ProtectedFrame for LocalFrame {
        type Output = i32;

        fn descriptor(
            &self,
        ) -> &bray_runtime_interface::ProtectedFrameDescriptor {
            self.inner.descriptor()
        }

        fn resume(
            self: Pin<&mut Self>,
            context: FrameContext,
        ) -> FrameProgress<Self::Output> {
            let frame = self.get_mut();

            Pin::new(&mut frame.inner).resume(context)
        }

        fn broadcast_tasks(self: Pin<&mut Self>) {
            let frame = self.get_mut();

            Pin::new(&mut frame.inner).broadcast_tasks();
        }

        fn resolve_lifecycle(
            self: Pin<&mut Self>,
            exit: FrameExit,
        ) {
            let frame = self.get_mut();

            Pin::new(&mut frame.inner).resolve_lifecycle(exit);
        }
    }
}
