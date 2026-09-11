use super::waiters::{JoinWaitKind, JoinWaitState};
use std::num::NonZeroU64;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use triomphe::Arc as TaskArc;

use bray_runtime_model::{ProtectedFrameDescriptor, ProtectedFrameStateId};

use crate::context::{TaskOutput, current_task_output, current_task_start_site};
use crate::root::is_propagated_cancellation;
use crate::{
    CancellationContext, FrameContext, FrameExecutionState, FrameExit, FrameProgress,
    FrameSuspension, ProtectedFrame, RunOutcome, RunOutcomeKind, RuntimePanic,
    SendableProtectedFrame, TaskSnapshot, TaskStartSite,
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
    pub(crate) const fn is_terminal(self) -> bool {
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
    /// Runtime infrastructure could not continue driving the frame.
    ExecutionInfrastructure,
    /// The frame reported a compiler/runtime contract violation.
    FrameContract,
}

/// Result of one successful task resume.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum TaskResumeStatus {
    /// The frame suspended in one checked state.
    Suspended(FrameSuspension, FrameExecutionState),
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
    /// The frame reported a compiler/runtime contract violation.
    RuntimeFailed(TaskFailureKind),
    /// Internal task state was poisoned by an unexpected runtime panic.
    SynchronizationPoisoned,
}

/// Failure to create stable task-owned storage.
#[derive(Debug)]
pub enum TaskStartError {
    /// Cancellation state could not be admitted.
    Cancellation(crate::CancellationAdmissionError),
    /// Stable task-owned storage could not be allocated.
    AllocationFailed,
    /// Process-local task identities were exhausted.
    IdentityExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TaskAdmissionKind {
    Independent,
    Continuation,
}

impl<T, F: ?Sized + ProtectedFrame<Output = T>> TaskControlBlock<T, F> {
    pub(crate) fn prepare(
        cancellation: CancellationContext,
        descriptor: ProtectedFrameDescriptor,
    ) -> Result<Self, TaskStartError> {
        let join_waiters = crate::allocation::allocate_shared(JoinWaitState::new())
            .map_err(|_| TaskStartError::AllocationFailed)?;

        // Checked frame descriptors always contain the entry state.
        let execution = FrameExecutionState::new(
            descriptor.frame(),
            descriptor
                .state(ProtectedFrameStateId::new(0))
                .expect("checked frame descriptor must contain state zero")
                .clone(),
        );

        Ok(Self {
            id: next_task_id()?,
            start_site: None,
            descriptor,
            data: Mutex::new(TaskData {
                frame: None,
                state: TaskState::Ready,
                execution,
                outcome: None,
            }),
            join_waiters,
            cancellation,
            output: TaskOutput::default(),
            resuming: AtomicBool::new(false),
        })
    }

    pub(crate) fn install_frame(&mut self, frame: Pin<Box<F>>) {
        self.start_site = current_task_start_site();
        self.output = current_task_output();

        self.data
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .frame = Some(frame);
    }
}

/// Failure to observe or register observation of one task outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskObservationError {
    /// The task has not reached a terminal state.
    Pending,
    /// The terminal outcome was already moved to its observer.
    AlreadyObserved,
    /// Completion-waiter identities cannot be represented.
    WaiterIdentityExhausted,
    /// Optional observer storage could not grow.
    WaiterAllocationFailed,
    /// The task already has a registered owning continuation.
    OwnerAlreadyWaiting,
    /// The task failed its compiler/runtime frame contract.
    RuntimeFailed(TaskFailureKind),
    /// Internal task state was poisoned by an unexpected runtime panic.
    SynchronizationPoisoned,
}

/// Infallible notification used when a task becomes observable.
pub trait JoinWake: Send + Sync + 'static {
    /// Makes the registered observer runnable for the identified terminal task.
    fn wake(&self, task: TaskId);
}

impl<F> JoinWake for F
where
    F: Fn() + Send + Sync + 'static,
{
    fn wake(&self, _: TaskId) {
        self();
    }
}

struct TaskData<T, F>
where
    F: ?Sized + ProtectedFrame<Output = T>,
{
    frame: Option<Pin<Box<F>>>,
    state: TaskState,
    execution: FrameExecutionState,
    outcome: Option<RunOutcome<T>>,
}

/// Stable runtime-owned storage for one independently executing task.
pub struct TaskControlBlock<
    T,
    F: ?Sized + ProtectedFrame<Output = T> = dyn SendableProtectedFrame<Output = T>,
> {
    id: TaskId,
    start_site: Option<TaskStartSite>,
    descriptor: ProtectedFrameDescriptor,
    data: Mutex<TaskData<T, F>>,
    join_waiters: TaskArc<JoinWaitState>,
    cancellation: CancellationContext,
    output: TaskOutput,
    resuming: AtomicBool,
}

impl<T, F> Drop for TaskControlBlock<T, F>
where
    F: ?Sized + ProtectedFrame<Output = T>,
{
    fn drop(&mut self) {
        drop(self.join_waiters.close());

        let data = self
            .data
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if data.frame.is_none() {
            return;
        }

        // This is the invariant fallback when an explicit owner failed to terminalize the task.
        let _ = resolve_failed_frame(&mut data.frame);
    }
}

impl<T: 'static, F> TaskControlBlock<T, F>
where
    F: ?Sized + ProtectedFrame<Output = T>,
{
    /// Returns the process-local task identity.
    pub const fn id(&self) -> TaskId {
        self.id
    }

    /// Returns the immutable root frame descriptor used to admit this task.
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

    /// Captures immutable task, frame, cancellation, and observation state.
    pub fn snapshot(&self) -> Result<TaskSnapshot, TaskObservationError> {
        let data = self
            .data
            .lock()
            .map_err(|_| TaskObservationError::SynchronizationPoisoned)?;

        let unobserved_outcome = data.outcome.as_ref().map(RunOutcome::kind);

        // Frame descriptors are immutable and clone only their shared tables.
        Ok(TaskSnapshot::new(
            self.id,
            self.start_site,
            self.descriptor.clone(),
            data.state,
            data.execution.clone(),
            self.cancellation.observation(),
            self.join_waiters.len(),
            unobserved_outcome,
        ))
    }

    /// Atomically records a cancellation request.
    ///
    /// Returns whether this call changed the request state.
    pub fn request_cancellation(&self) -> bool {
        self.cancellation.request()
    }

    /// Returns whether cancellation is currently observable by this task.
    pub fn cancellation_observable(&self) -> bool {
        self.cancellation.observation().observable()
    }

    /// Returns the structured cancellation context owned by this task.
    pub const fn cancellation_context(&self) -> &CancellationContext {
        &self.cancellation
    }

    pub(crate) const fn output_context(&self) -> &TaskOutput {
        &self.output
    }

    /// Enters or resumes the task without allowing concurrent execution.
    pub fn resume(&self) -> Result<TaskResumeStatus, TaskResumeError> {
        let Some(_resume) = ResumeGuard::acquire(&self.resuming) else {
            return Err(TaskResumeError::AlreadyRunning);
        };

        let mut frame = {
            let mut data = self.lock_data()?;

            if !matches!(data.state, TaskState::Ready | TaskState::Suspended(_)) {
                return Err(TaskResumeError::NotResumable(data.state));
            }

            let Some(frame) = data.frame.take() else {
                let failure = TaskFailureKind::MissingFrame;
                data.state = TaskState::Failed(failure);

                return Err(TaskResumeError::NotResumable(data.state));
            };

            data.state = TaskState::Running;

            frame
        };

        let context = FrameContext::new(self.cancellation_observable());

        let (progress, execution) = match catch_unwind(AssertUnwindSafe(|| {
            let progress = frame.as_mut().resume(context);

            let execution = match &progress {
                FrameProgress::Suspended(suspension) => frame.execution_state(suspension.state()),
                _ => None,
            };

            (progress, execution)
        })) {
            Ok(result) => result,
            Err(payload) if is_propagated_cancellation(payload.as_ref()) => {
                (FrameProgress::Cancelled, None)
            }
            Err(payload) => (
                FrameProgress::Panicked(RuntimePanic::from_payload(payload)),
                None,
            ),
        };

        let failure = match &progress {
            FrameProgress::Suspended(suspension)
                if execution
                    .as_ref()
                    .is_none_or(|execution| execution.state() != suspension.state()) =>
            {
                Some(TaskFailureKind::UnknownSuspensionState(suspension.state()))
            }
            FrameProgress::RuntimeFailure => Some(TaskFailureKind::FrameContract),
            _ => None,
        };

        if let Some(failure) = failure {
            let _ = resolve_failed_frame(&mut Some(frame));
            self.publish_failure(failure);

            return Err(match failure {
                TaskFailureKind::UnknownSuspensionState(state) => {
                    TaskResumeError::UnknownSuspensionState(state)
                }
                _ => TaskResumeError::RuntimeFailed(failure),
            });
        }

        if let FrameProgress::Suspended(suspension) = progress {
            let execution = execution.expect("suspended execution metadata was validated");

            let mut data = self
                .data
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            data.frame = Some(frame);
            data.state = TaskState::Suspended(suspension.state());

            // The task and its returned suspension each retain the immutable active state.
            data.execution = execution.clone();

            return Ok(TaskResumeStatus::Suspended(suspension, execution));
        }

        let outcome = finish_frame(frame.as_mut(), progress);
        let outcome = destroy_frame(Some(frame), outcome);
        let kind = outcome.kind();

        let waiters = {
            let mut data = self
                .data
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            data.state = task_state(kind);
            data.outcome = Some(outcome);

            self.join_waiters.close()
        };

        waiters.wake_all(self.id);

        Ok(TaskResumeStatus::Terminal(kind))
    }

    /// Registers an observer to wake when this task becomes terminal.
    pub fn register_join_waiter(
        &self,
        waiter: Arc<dyn JoinWake>,
    ) -> Result<JoinWaitRegistration, TaskObservationError> {
        self.register_waiter(waiter.into(), JoinWaitKind::Observer)
    }

    pub(crate) fn register_owner_waiter(
        &self,
        waiter: impl Into<super::JoinNotification>,
    ) -> Result<JoinWaitRegistration, TaskObservationError> {
        self.register_waiter(waiter.into(), JoinWaitKind::Owner)
    }

    fn register_waiter(
        &self,
        waiter: super::JoinNotification,
        kind: JoinWaitKind,
    ) -> Result<JoinWaitRegistration, TaskObservationError> {
        let identity = self.join_waiters.register(self.id, waiter, kind)?;

        Ok(JoinWaitRegistration {
            identity,
            state: TaskArc::clone(&self.join_waiters),
        })
    }

    /// Moves the terminal run outcome to its single observer.
    pub fn take_outcome(&self) -> Result<RunOutcome<T>, TaskObservationError> {
        let mut data = self
            .data
            .lock()
            .map_err(|_| TaskObservationError::SynchronizationPoisoned)?;

        match data.state {
            TaskState::Ready | TaskState::Running | TaskState::Suspended(_) => {
                Err(TaskObservationError::Pending)
            }
            TaskState::Failed(failure) => Err(TaskObservationError::RuntimeFailed(failure)),
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

    // The root driver calls this only after leaving its exclusive resume loop.
    pub(crate) fn resolve_runtime_failure(&self) -> Option<RuntimePanic> {
        let _resume = ResumeGuard::acquire(&self.resuming)
            .expect("runtime failure resolution requires exclusive task execution");

        let mut frame = {
            let mut data = self
                .data
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            let frame = data.frame.take()?;
            data.state = TaskState::Running;

            Some(frame)
        };

        let panic = resolve_failed_frame(&mut frame);

        self.publish_failure(TaskFailureKind::ExecutionInfrastructure);

        panic
    }

    fn publish_failure(&self, failure: TaskFailureKind) {
        let waiters = {
            let mut data = self
                .data
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            data.state = TaskState::Failed(failure);

            self.join_waiters.close()
        };

        waiters.wake_all(self.id);
    }

    pub(crate) fn execution_origin(&self) -> crate::CleanupIncidentOrigin {
        let data = self
            .data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        crate::CleanupIncidentOrigin::new(data.execution.frame(), data.execution.state())
    }

    fn lock_data(&self) -> Result<MutexGuard<'_, TaskData<T, F>>, TaskResumeError> {
        self.data
            .lock()
            .map_err(|_| TaskResumeError::SynchronizationPoisoned)
    }
}

/// Cancellation-safe ownership of one pending task-completion wait.
pub struct JoinWaitRegistration {
    identity: Option<u64>,
    state: TaskArc<JoinWaitState>,
}

impl Drop for JoinWaitRegistration {
    fn drop(&mut self) {
        let Some(identity) = self.identity.take() else {
            return;
        };

        self.state.remove(identity);
    }
}

impl JoinWaitRegistration {
    pub(crate) fn is_pending(&self) -> bool {
        let Some(identity) = self.identity else {
            return false;
        };

        self.state.contains(identity)
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
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .map_err(|_| TaskStartError::IdentityExhausted)?;

    NonZeroU64::new(id)
        .map(TaskId)
        .ok_or(TaskStartError::IdentityExhausted)
}

fn finish_frame<T: 'static, F>(mut frame: Pin<&mut F>, progress: FrameProgress<T>) -> RunOutcome<T>
where
    F: ?Sized + ProtectedFrame<Output = T>,
{
    let (mut outcome, mut exit) = match progress {
        FrameProgress::Suspended(_) => {
            unreachable!("suspended frames are not terminalized")
        }
        FrameProgress::Completed(value) => (RunOutcome::Completed(value), FrameExit::Completed),
        FrameProgress::Cancelled => (RunOutcome::Cancelled, FrameExit::Cancelled),
        FrameProgress::Panicked(panic) => (RunOutcome::Panicked(panic), FrameExit::Panicked),
        FrameProgress::RuntimeFailure => {
            unreachable!("failed frames are not terminalized")
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

fn resolve_failed_frame<T, F>(frame: &mut Option<Pin<Box<F>>>) -> Option<RuntimePanic>
where
    F: ?Sized + ProtectedFrame<Output = T>,
{
    let mut panic = None;

    if let Some(frame) = frame.as_mut() {
        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| {
            frame.as_mut().broadcast_tasks();
        })) {
            merge_cleanup_panic(&mut panic, payload);
        }

        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| {
            frame.as_mut().resolve_lifecycle(FrameExit::RuntimeFailure);
        })) {
            merge_cleanup_panic(&mut panic, payload);
        }
    }

    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(frame.take()))) {
        merge_cleanup_panic(&mut panic, payload);
    }

    panic
}

fn merge_panic<T>(outcome: &mut RunOutcome<T>, payload: Box<dyn std::any::Any + Send>) {
    match outcome {
        RunOutcome::Panicked(panic) => panic.push_suppressed(payload),
        RunOutcome::Completed(_) | RunOutcome::Cancelled => {
            *outcome = RunOutcome::Panicked(RuntimePanic::from_payload(payload));
        }
    }
}

fn merge_cleanup_panic(panic: &mut Option<RuntimePanic>, payload: Box<dyn std::any::Any + Send>) {
    if let Some(panic) = panic {
        panic.push_suppressed(payload);
    } else {
        *panic = Some(RuntimePanic::from_payload(payload));
    }
}

const fn task_state(kind: RunOutcomeKind) -> TaskState {
    match kind {
        RunOutcomeKind::Completed => TaskState::Completed,
        RunOutcomeKind::Cancelled => TaskState::Cancelled,
        RunOutcomeKind::Panicked => TaskState::Panicked,
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::pin::Pin;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier, Mutex};
    use std::thread;

    use bray_platform::RuntimeThreadScope;
    use bray_runtime_model::{ProtectedFrameStateId, RuntimeCapability};

    use super::{
        TaskControlBlock, TaskFailureKind, TaskObservationError, TaskResumeError, TaskResumeStatus,
        TaskState,
    };
    use crate::context::with_task_execution_context;
    use crate::test_support::{TestFrame, register_task};
    use crate::{
        ExecutionLane, ExecutionLanePlacement, ExecutionWorkload, FrameContext, FrameExit,
        FrameProgress, ProtectedFrame, RunOutcome, Scheduler, SchedulerLimits,
        TaskExecutionContext, erase_sendable_protected_frame,
    };

    #[test]
    fn task_snapshots_retain_creation_and_cleanup_context() {
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = Scheduler::new(
            [RuntimeCapability::CooperativeExecution],
            runtime.runtime().id(),
            SchedulerLimits::new(nonzero(4), nonzero(4)),
        );

        let parent = TaskControlBlock::start(TestFrame::completing(1))
            .unwrap_or_else(|error| panic!("parent task must start: {error:?}"));

        let registration = register_task(&scheduler, &parent, runtime.runtime().id());

        let context = TaskExecutionContext::new(
            parent.id(),
            ProtectedFrameStateId::new(0),
            parent.cancellation_context().clone(),
            parent.output_context().clone(),
            ExecutionLane::new(
                ExecutionLanePlacement::PinnedWorker(runtime.runtime().id()),
                ExecutionWorkload::Cooperative,
            ),
            registration.wake_handle(),
        );

        let child_frame = TestFrame::retaining_state(2);

        let mut child = TaskControlBlock::<i32>::prepare(
            crate::CancellationContext::root().unwrap(),
            child_frame.descriptor().clone(),
        )
        .unwrap();

        assert!(child.snapshot().unwrap().start_site().is_none());

        // Storage reservation does not choose the parent or output context of later execution.
        with_task_execution_context(context, || child.install_frame(Box::pin(child_frame)));

        child
            .resume()
            .unwrap_or_else(|error| panic!("child task must suspend: {error:?}"));

        let snapshot = child
            .snapshot()
            .unwrap_or_else(|error| panic!("task snapshot must succeed: {error:?}"));

        let Some(start_site) = snapshot.start_site() else {
            panic!("child task must retain its creation site");
        };

        assert_eq!(start_site.parent(), parent.id());
        assert_eq!(start_site.state(), ProtectedFrameStateId::new(0));
        assert_eq!(snapshot.execution().state(), ProtectedFrameStateId::new(1));

        assert_eq!(
            snapshot.state(),
            TaskState::Suspended(ProtectedFrameStateId::new(1))
        );

        assert_eq!(snapshot.join_waiters(), 0);
        assert_eq!(snapshot.unobserved_outcome(), None);

        assert_eq!(
            snapshot.retained_storage(),
            &[bray_runtime_model::ProtectedFrameStorageId::new(4)]
        );

        assert_eq!(
            snapshot.cleanup_blockers(),
            &[bray_runtime_model::ProtectedFrameDependencyId::new(6)]
        );
    }

    #[test]
    fn starting_moves_the_frame_into_stable_task_owned_storage() {
        let task = TaskControlBlock::start(TestFrame::suspending_then_completing(29))
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        assert_eq!(task.state(), Ok(TaskState::Ready));

        assert!(matches!(
            task.resume(),
            Ok(TaskResumeStatus::Suspended(suspension, execution))
                if suspension.state() == bray_runtime_model::ProtectedFrameStateId::new(1)
                    && execution.state() == suspension.state()
        ));

        assert_eq!(
            task.state(),
            Ok(TaskState::Suspended(
                bray_runtime_model::ProtectedFrameStateId::new(1)
            ))
        );

        assert_eq!(
            task.resume(),
            Ok(TaskResumeStatus::Terminal(crate::RunOutcomeKind::Completed))
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
    fn pending_observers_do_not_keep_nonterminal_frame_storage_alive() {
        let broadcasts = Arc::new(AtomicUsize::new(0));
        let resolutions = Arc::new(AtomicUsize::new(0));

        let task = TaskControlBlock::start(TrackedFrame {
            frame: TestFrame::suspending_then_completing(1),
            broadcasts: Arc::clone(&broadcasts),
            resolutions: Arc::clone(&resolutions),
        })
        .unwrap_or_else(|error| panic!("tracked task must start: {error:?}"));

        assert!(matches!(
            task.resume(),
            Ok(TaskResumeStatus::Suspended(_, _))
        ));

        let observer = task.register_join_waiter(Arc::new(|| {})).unwrap();
        let owner = task.register_owner_waiter(Arc::new(|| {})).unwrap();
        assert!(observer.is_pending());
        assert!(owner.is_pending());

        drop(task);

        assert_eq!(broadcasts.load(Ordering::Relaxed), 1);
        assert_eq!(resolutions.load(Ordering::Relaxed), 1);
        assert!(!observer.is_pending());
        assert!(!owner.is_pending());
    }

    #[test]
    fn erased_inactive_frames_use_the_same_task_storage_contract() {
        let frame = erase_sendable_protected_frame(TestFrame::completing(41)).unwrap();

        let task = TaskControlBlock::start_erased(frame)
            .unwrap_or_else(|error| panic!("erased test task must start: {error:?}"));

        assert_eq!(
            task.resume(),
            Ok(TaskResumeStatus::Terminal(crate::RunOutcomeKind::Completed))
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
            Ok(TaskResumeStatus::Terminal(crate::RunOutcomeKind::Completed))
        );

        assert!(matches!(task.take_outcome(), Ok(RunOutcome::Completed(43))));
    }

    #[test]
    fn cancellation_is_observed_by_frames_and_terminal_waiters() {
        let task = TaskControlBlock::start(TestFrame::cancellation_aware())
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        let wake_count = std::sync::Arc::new(AtomicUsize::new(0));

        let waiter_count = std::sync::Arc::clone(&wake_count);

        let _registration = task
            .register_join_waiter(std::sync::Arc::new(move || {
                waiter_count.fetch_add(1, Ordering::Relaxed);
            }))
            .unwrap_or_else(|error| panic!("join waiter must register: {error:?}"));

        assert!(task.request_cancellation());
        assert!(!task.request_cancellation());

        assert_eq!(
            task.resume(),
            Ok(TaskResumeStatus::Terminal(crate::RunOutcomeKind::Cancelled))
        );

        assert_eq!(wake_count.load(Ordering::Relaxed), 1);
        assert!(matches!(task.take_outcome(), Ok(RunOutcome::Cancelled)));
    }

    #[test]
    fn dropping_join_registration_withdraws_the_waiter() {
        let task = TaskControlBlock::start(TestFrame::completing(9))
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        let wake_count = Arc::new(AtomicUsize::new(0));
        let waiter_count = Arc::clone(&wake_count);

        let registration = task
            .register_join_waiter(Arc::new(move || {
                waiter_count.fetch_add(1, Ordering::Relaxed);
            }))
            .unwrap_or_else(|error| panic!("join waiter must register: {error:?}"));

        drop(registration);

        assert_eq!(
            task.resume(),
            Ok(TaskResumeStatus::Terminal(crate::RunOutcomeKind::Completed))
        );

        assert_eq!(wake_count.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn owner_wait_is_withdrawable_and_terminal_notification_can_reenter_the_task() {
        let task = TaskControlBlock::start(TestFrame::completing(9)).unwrap();
        let wake_count = Arc::new(AtomicUsize::new(0));
        let retained_task = triomphe::Arc::clone(&task);
        let observed_count = Arc::clone(&wake_count);

        let wake: Arc<dyn crate::JoinWake> = Arc::new(move || {
            assert!(retained_task.has_unobserved_outcome().unwrap());
            observed_count.fetch_add(1, Ordering::Relaxed);
        });

        let observer = task.register_join_waiter(Arc::clone(&wake)).unwrap();
        let first_owner = task.register_owner_waiter(Arc::clone(&wake)).unwrap();

        assert!(matches!(
            task.register_owner_waiter(Arc::clone(&wake)),
            Err(TaskObservationError::OwnerAlreadyWaiting),
        ));

        assert!(first_owner.is_pending());

        drop(first_owner);

        let owner = task.register_owner_waiter(Arc::clone(&wake)).unwrap();

        assert_eq!(
            task.resume(),
            Ok(TaskResumeStatus::Terminal(crate::RunOutcomeKind::Completed))
        );

        assert_eq!(wake_count.load(Ordering::Relaxed), 2);
        assert!(!owner.is_pending());
        assert!(!observer.is_pending());

        let terminal_owner = task.register_owner_waiter(wake).unwrap();

        assert!(!terminal_owner.is_pending());
        assert_eq!(wake_count.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn parent_cancellation_reaches_child_task_frames() {
        let parent = crate::CancellationContext::root().unwrap();

        let task = TaskControlBlock::start_child(TestFrame::cancellation_aware(), &parent)
            .unwrap_or_else(|error| panic!("child test task must start: {error:?}"));

        parent.request();

        assert!(task.cancellation_observable());

        assert_eq!(
            task.resume(),
            Ok(TaskResumeStatus::Terminal(crate::RunOutcomeKind::Cancelled))
        );
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

        let worker_task = triomphe::Arc::clone(&task);

        let worker = thread::spawn(move || worker_task.resume());

        entered.wait();

        assert_eq!(task.resume(), Err(TaskResumeError::AlreadyRunning));

        release.wait();

        assert_eq!(
            worker
                .join()
                .unwrap_or_else(|_| panic!("resume worker must not panic")),
            Ok(TaskResumeStatus::Terminal(crate::RunOutcomeKind::Completed))
        );
    }

    #[test]
    fn panics_are_captured_and_later_cleanup_panics_are_suppressed() {
        let task = TaskControlBlock::start(TestFrame::panicking_with_cleanup_panic())
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        assert_eq!(
            task.resume(),
            Ok(TaskResumeStatus::Terminal(crate::RunOutcomeKind::Panicked))
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
    fn propagated_cancellation_reaches_task_cleanup_as_cancellation() {
        let task = TaskControlBlock::start(TestFrame::propagating_cancellation(false))
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        assert_eq!(
            task.resume(),
            Ok(TaskResumeStatus::Terminal(crate::RunOutcomeKind::Cancelled))
        );

        assert!(matches!(task.take_outcome(), Ok(RunOutcome::Cancelled)));
    }

    #[test]
    fn propagated_cancellation_preserves_a_real_cleanup_panic() {
        let task = TaskControlBlock::start(TestFrame::propagating_cancellation(true))
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        assert_eq!(
            task.resume(),
            Ok(TaskResumeStatus::Terminal(crate::RunOutcomeKind::Panicked))
        );

        let Ok(RunOutcome::Panicked(panic)) = task.take_outcome() else {
            panic!("cleanup panic must remain observable");
        };

        assert!(panic.primary_is::<&'static str>());
        assert_eq!(panic.suppressed_count(), 0);
    }

    #[test]
    fn suspended_execution_retains_active_frame_identity_and_validates_local_state() {
        for (returned_state, panics) in [(9, false), (8, false), (9, true)] {
            let execution = crate::FrameExecutionState::new(
                bray_runtime_model::ProtectedAsyncFrameId::new([42; 32]),
                bray_runtime_model::ProtectedFrameStateDescriptor::new(
                    ProtectedFrameStateId::new(returned_state),
                    [],
                    [],
                    [],
                    bray_runtime_model::ProtectedFrameAffinity::OriginThread,
                ),
            );

            let frame = ActiveStateFrame {
                inner: TestFrame::invalid_suspension(),
                execution: execution.clone(),
                panics,
            };

            let task = TaskControlBlock::start(frame).unwrap();
            let root = task.descriptor().frame();

            assert!(
                task.descriptor()
                    .state(ProtectedFrameStateId::new(9))
                    .is_none()
            );

            let resumed = task.resume();

            if panics {
                assert!(matches!(
                    resumed,
                    Ok(TaskResumeStatus::Terminal(crate::RunOutcomeKind::Panicked))
                ));
            } else if returned_state == 9 {
                assert!(
                    matches!(resumed, Ok(TaskResumeStatus::Suspended(_, active)) if active == execution)
                );

                let snapshot = task.snapshot().unwrap();
                assert_eq!(snapshot.descriptor().frame(), root);
                assert_eq!(snapshot.execution(), &execution);

                assert_eq!(
                    task.execution_origin(),
                    crate::CleanupIncidentOrigin::new(execution.frame(), execution.state())
                );
            } else {
                assert_eq!(
                    resumed,
                    Err(TaskResumeError::UnknownSuspensionState(
                        ProtectedFrameStateId::new(9)
                    ))
                );
            }
        }
    }

    struct ActiveStateFrame {
        inner: TestFrame,
        execution: crate::FrameExecutionState,
        panics: bool,
    }

    impl ProtectedFrame for ActiveStateFrame {
        type Output = i32;

        fn descriptor(&self) -> &bray_runtime_model::ProtectedFrameDescriptor {
            self.inner.descriptor()
        }

        fn execution_state(&self, _: ProtectedFrameStateId) -> Option<crate::FrameExecutionState> {
            assert!(!self.panics, "execution metadata callback failed");

            Some(self.execution.clone())
        }

        fn resume(self: Pin<&mut Self>, context: FrameContext) -> FrameProgress<i32> {
            Pin::new(&mut self.get_mut().inner).resume(context)
        }

        fn broadcast_tasks(self: Pin<&mut Self>) {
            Pin::new(&mut self.get_mut().inner).broadcast_tasks();
        }

        fn resolve_lifecycle(self: Pin<&mut Self>, exit: FrameExit) {
            Pin::new(&mut self.get_mut().inner).resolve_lifecycle(exit);
        }
    }

    #[test]
    fn invalid_suspension_state_fails_without_resuming_again() {
        let task = TaskControlBlock::start(TestFrame::invalid_suspension())
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        let wake_count = Arc::new(AtomicUsize::new(0));
        let waiter_count = Arc::clone(&wake_count);

        let _registration = task
            .register_join_waiter(Arc::new(move || {
                waiter_count.fetch_add(1, Ordering::Relaxed);
            }))
            .unwrap_or_else(|error| panic!("join waiter must register: {error:?}"));

        let failure = TaskFailureKind::UnknownSuspensionState(
            bray_runtime_model::ProtectedFrameStateId::new(9),
        );

        assert_eq!(
            task.resume(),
            Err(TaskResumeError::UnknownSuspensionState(
                bray_runtime_model::ProtectedFrameStateId::new(9)
            ))
        );

        assert_eq!(task.state(), Ok(TaskState::Failed(failure)));
        assert_eq!(wake_count.load(Ordering::Relaxed), 1);

        assert_eq!(
            task.resume(),
            Err(TaskResumeError::NotResumable(TaskState::Failed(failure)))
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

    #[test]
    fn frame_callbacks_run_without_the_task_data_lock() {
        for runtime_failure in [false, true] {
            let callback = Arc::new(Mutex::new(None));
            let checks = Arc::new(AtomicUsize::new(0));

            let task = TaskControlBlock::start(LockCheckingFrame {
                inner: TestFrame::completing(47),
                callback: Arc::clone(&callback),
            })
            .unwrap();

            let observed_task = task.clone();
            let observed_checks = Arc::clone(&checks);

            *callback.lock().unwrap() = Some(Box::new(move || {
                let data = observed_task
                    .data
                    .try_lock()
                    .expect("callback must not hold task data");

                assert_eq!(data.state, TaskState::Running);
                assert!(data.frame.is_none());
                drop(data);
                assert_eq!(observed_task.resume(), Err(TaskResumeError::AlreadyRunning));
                observed_checks.fetch_add(1, Ordering::Relaxed);
            }) as Box<dyn Fn() + Send + Sync>);

            if runtime_failure {
                assert!(task.resolve_runtime_failure().is_none());
                assert_eq!(checks.load(Ordering::Relaxed), 3);

                assert_eq!(
                    task.state().unwrap(),
                    TaskState::Failed(TaskFailureKind::ExecutionInfrastructure)
                );
            } else {
                assert!(matches!(task.resume(), Ok(TaskResumeStatus::Terminal(_))));
                assert!(matches!(task.take_outcome(), Ok(RunOutcome::Completed(47))));
                assert_eq!(checks.load(Ordering::Relaxed), 4);
            }

            // Release the callback's task ownership after the frame has been destroyed.
            callback.lock().unwrap().take();
        }
    }

    struct LockCheckingFrame {
        inner: TestFrame,
        callback: Arc<Mutex<Option<Box<dyn Fn() + Send + Sync>>>>,
    }

    impl LockCheckingFrame {
        fn check(&self) {
            if let Some(callback) = self.callback.lock().unwrap().as_ref() {
                callback();
            }
        }
    }

    impl ProtectedFrame for LockCheckingFrame {
        type Output = i32;

        fn descriptor(&self) -> &bray_runtime_model::ProtectedFrameDescriptor {
            self.inner.descriptor()
        }

        fn resume(self: Pin<&mut Self>, context: FrameContext) -> FrameProgress<i32> {
            self.check();

            Pin::new(&mut self.get_mut().inner).resume(context)
        }

        fn broadcast_tasks(self: Pin<&mut Self>) {
            self.check();
        }

        fn resolve_lifecycle(self: Pin<&mut Self>, _exit: FrameExit) {
            self.check();
        }
    }

    impl Drop for LockCheckingFrame {
        fn drop(&mut self) {
            self.check();
        }
    }

    fn nonzero(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value)
            .unwrap_or_else(|| panic!("test scheduler capacity must be nonzero"))
    }

    struct LocalFrame {
        inner: TestFrame,
        _thread_affinity: Rc<()>,
    }

    struct TrackedFrame {
        frame: TestFrame,
        broadcasts: Arc<AtomicUsize>,
        resolutions: Arc<AtomicUsize>,
    }

    impl ProtectedFrame for TrackedFrame {
        type Output = i32;

        fn descriptor(&self) -> &bray_runtime_model::ProtectedFrameDescriptor {
            self.frame.descriptor()
        }

        fn resume(self: Pin<&mut Self>, context: FrameContext) -> FrameProgress<Self::Output> {
            Pin::new(&mut self.get_mut().frame).resume(context)
        }

        fn broadcast_tasks(self: Pin<&mut Self>) {
            self.broadcasts.fetch_add(1, Ordering::Relaxed);
        }

        fn resolve_lifecycle(self: Pin<&mut Self>, exit: FrameExit) {
            assert_eq!(exit, FrameExit::RuntimeFailure);

            self.resolutions.fetch_add(1, Ordering::Relaxed);
        }
    }

    impl ProtectedFrame for LocalFrame {
        type Output = i32;

        fn descriptor(&self) -> &bray_runtime_model::ProtectedFrameDescriptor {
            self.inner.descriptor()
        }

        fn resume(self: Pin<&mut Self>, context: FrameContext) -> FrameProgress<Self::Output> {
            let frame = self.get_mut();

            Pin::new(&mut frame.inner).resume(context)
        }

        fn broadcast_tasks(self: Pin<&mut Self>) {
            let frame = self.get_mut();

            Pin::new(&mut frame.inner).broadcast_tasks();
        }

        fn resolve_lifecycle(self: Pin<&mut Self>, exit: FrameExit) {
            let frame = self.get_mut();

            Pin::new(&mut frame.inner).resolve_lifecycle(exit);
        }
    }
}
