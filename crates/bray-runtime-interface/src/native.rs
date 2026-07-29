/// Stable symbol initializing the distinguished main-thread runtime lane.
pub const MAIN_THREAD_LANE_STARTUP_SYMBOL: &str =
    "bray_runtime_main_thread_lane_startup_v1";

/// Stable symbol driving one callback on the distinguished main-thread runtime lane.
pub const MAIN_THREAD_LANE_DRIVE_SYMBOL: &str =
    "bray_runtime_main_thread_lane_drive_v1";

/// Stable symbol observing cancellation for the current run.
pub const CURRENT_RUN_CANCELLATION_OBSERVATION_SYMBOL: &str =
    "bray_runtime_current_run_cancellation_observation_v1";

/// Stable symbol shutting down the initialized runtime infrastructure.
pub const STRUCTURED_SHUTDOWN_SYMBOL: &str =
    "bray_runtime_structured_shutdown_v1";

/// Stable symbol allocating runtime-owned task storage.
pub const TASK_ALLOCATION_SYMBOL: &str = "bray_runtime_task_allocation_v1";

/// Stable symbol publishing an allocated task for execution.
pub const TASK_START_SYMBOL: &str = "bray_runtime_task_start_v1";

/// Stable symbol constructing a suspended frame result.
pub const SUSPENSION_REGISTRATION_SYMBOL: &str =
    "bray_runtime_suspension_registration_v1";

/// Stable symbol waking one suspended task state.
pub const WAKE_SYMBOL: &str = "bray_runtime_wake_v1";

/// Stable symbol requesting cancellation of one task.
pub const TASK_CANCELLATION_REQUEST_SYMBOL: &str =
    "bray_runtime_task_cancellation_request_v1";

/// Stable symbol registering an observer for one task terminal state.
pub const JOIN_REGISTRATION_SYMBOL: &str =
    "bray_runtime_join_registration_v1";

/// Stable symbol constructing a terminal frame result.
pub const TERMINAL_PUBLICATION_SYMBOL: &str =
    "bray_runtime_terminal_publication_v1";

/// Stable symbol entering one runtime callback root.
pub const RUNTIME_EVENT_SYMBOL: &str = "bray_runtime_event_v1";

/// Stable symbol selecting the lane for one task state.
pub const COMPATIBLE_LANE_SELECTION_SYMBOL: &str =
    "bray_runtime_compatible_lane_selection_v1";

/// Status returned by native runtime operations.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeRuntimeStatus(u32);

impl NativeRuntimeStatus {
    /// The operation completed successfully.
    pub const SUCCESS: Self = Self(0);
    /// Runtime infrastructure has not been initialized on this thread.
    pub const NOT_INITIALIZED: Self = Self(1);
    /// Runtime infrastructure is already initialized on this thread.
    pub const ALREADY_INITIALIZED: Self = Self(2);
    /// One ABI argument violates its declared contract.
    pub const INVALID_ARGUMENT: Self = Self(3);
    /// Runtime infrastructure could not preserve its execution contract.
    pub const RUNTIME_FAILURE: Self = Self(4);
    /// A runtime implementation panic was contained at the ABI boundary.
    pub const PANICKED: Self = Self(5);
    /// The requested task has not reached a terminal state.
    pub const PENDING: Self = Self(6);
    /// The supplied task handle does not identify owned runtime state.
    pub const UNKNOWN_TASK: Self = Self(7);

    /// Returns whether the operation completed successfully.
    pub const fn is_success(self) -> bool {
        self.0 == Self::SUCCESS.0
    }

    /// Returns the stable integer status code.
    pub const fn code(self) -> u32 {
        self.0
    }
}

/// Target-sized capacities selected when runtime infrastructure starts.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeRuntimeConfiguration {
    task_capacity: usize,
    timer_capacity: usize,
}

impl NativeRuntimeConfiguration {
    /// Creates explicit capacities for registered tasks and pending timers.
    pub const fn new(task_capacity: usize, timer_capacity: usize) -> Self {
        Self {
            task_capacity,
            timer_capacity,
        }
    }

    /// Returns the maximum registered-task count.
    pub const fn task_capacity(self) -> usize {
        self.task_capacity
    }

    /// Returns the maximum pending-timer count.
    pub const fn timer_capacity(self) -> usize {
        self.timer_capacity
    }
}

/// Terminal state returned across the private native execution ABI.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeRunState(u32);

impl NativeRunState {
    /// The run completed normally.
    pub const COMPLETED: Self = Self(0);
    /// The run observed cooperative cancellation.
    pub const CANCELLED: Self = Self(1);
    /// The run terminated through panic propagation.
    pub const PANICKED: Self = Self(2);
    /// Runtime infrastructure could not execute the run.
    pub const RUNTIME_FAILURE: Self = Self(3);
    /// The observed run has not reached a terminal state.
    pub const PENDING: Self = Self(4);

    /// Returns the stable integer terminal-state code.
    pub const fn code(self) -> u32 {
        self.0
    }
}

/// ABI-safe terminal record for a compiler-generated root callback.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeRunOutcome {
    state: NativeRunState,
    payload: usize,
}

impl NativeRunOutcome {
    /// Creates one terminal record from its state and compiler-owned payload handle.
    pub const fn new(state: NativeRunState, payload: usize) -> Self {
        Self { state, payload }
    }

    /// Returns the terminal run state.
    pub const fn state(self) -> NativeRunState {
        self.state
    }

    /// Returns the opaque compiler-owned terminal payload handle.
    pub const fn payload(self) -> usize {
        self.payload
    }
}

/// Stable process-local handle for runtime-owned task storage.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativeTaskHandle(u64);

impl NativeTaskHandle {
    /// Creates one nonzero native task handle.
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 {
            return None;
        }

        Some(Self(value))
    }

    /// Returns the process-local handle value.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Result of allocating runtime-owned task storage.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeTaskAllocation {
    status: NativeRuntimeStatus,
    task: u64,
}

impl NativeTaskAllocation {
    /// Creates one successful allocation.
    pub const fn success(task: NativeTaskHandle) -> Self {
        Self {
            status: NativeRuntimeStatus::SUCCESS,
            task: task.raw(),
        }
    }

    /// Creates one failed allocation without a usable task handle.
    pub const fn failure(status: NativeRuntimeStatus) -> Self {
        Self { status, task: 0 }
    }

    /// Returns the operation status.
    pub const fn status(self) -> NativeRuntimeStatus {
        self.status
    }

    /// Returns the allocated task handle after success.
    pub const fn task(self) -> Option<NativeTaskHandle> {
        NativeTaskHandle::new(self.task)
    }
}

/// Thread-affinity requirement encoded by a generated frame state.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeFrameAffinity(u32);

impl NativeFrameAffinity {
    /// The state may move between compatible runtime workers.
    pub const MOVABLE: Self = Self(0);
    /// The state must resume on its originating runtime thread.
    pub const ORIGIN_THREAD: Self = Self(1);
    /// The state must resume on the distinguished main thread.
    pub const MAIN_THREAD: Self = Self(2);

    /// Returns the stable affinity code.
    pub const fn code(self) -> u32 {
        self.0
    }
}

/// Bitset of execution-lane requirements encoded by a generated frame state.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeLaneRequirements(u32);

impl NativeLaneRequirements {
    /// No additional lane requirement.
    pub const NONE: Self = Self(0);
    /// Blocking execution is required.
    pub const BLOCKING: Self = Self(1 << 0);
    /// Sustained compute execution is required.
    pub const COMPUTE: Self = Self(1 << 1);
    /// The distinguished main-thread lane is required.
    pub const MAIN_THREAD: Self = Self(1 << 2);

    /// Creates a requirement bitset from stable ABI bits.
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Returns the stable requirement bits.
    pub const fn bits(self) -> u32 {
        self.0
    }
}

/// Runtime-visible facts for one generated protected-frame state.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeFrameState {
    affinity: NativeFrameAffinity,
    lane_requirements: NativeLaneRequirements,
}

impl NativeFrameState {
    /// Creates one state contract.
    pub const fn new(
        affinity: NativeFrameAffinity,
        lane_requirements: NativeLaneRequirements,
    ) -> Self {
        Self {
            affinity,
            lane_requirements,
        }
    }

    /// Returns the state affinity.
    pub const fn affinity(self) -> NativeFrameAffinity {
        self.affinity
    }

    /// Returns the state lane requirements.
    pub const fn lane_requirements(self) -> NativeLaneRequirements {
        self.lane_requirements
    }
}

/// ABI-safe protected-frame progress category.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeFrameProgressKind(u32);

impl NativeFrameProgressKind {
    /// The frame suspended at the returned state.
    pub const SUSPENDED: Self = Self(0);
    /// The frame completed with the returned payload handle.
    pub const COMPLETED: Self = Self(1);
    /// The frame completed cancellation cleanup.
    pub const CANCELLED: Self = Self(2);
    /// The frame propagated the returned panic-report handle.
    pub const PANICKED: Self = Self(3);
    /// The generated frame violated its runtime contract.
    pub const RUNTIME_FAILURE: Self = Self(4);

    /// Creates a progress category from its stable ABI code.
    pub const fn from_code(code: u32) -> Self {
        Self(code)
    }

    /// Returns the stable progress code.
    pub const fn code(self) -> u32 {
        self.0
    }
}

/// ABI-safe result of entering one generated protected frame.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeFrameProgress {
    kind: NativeFrameProgressKind,
    state: u32,
    payload: usize,
}

impl NativeFrameProgress {
    /// Creates one frame progress record.
    pub const fn new(
        kind: NativeFrameProgressKind,
        state: u32,
        payload: usize,
    ) -> Self {
        Self {
            kind,
            state,
            payload,
        }
    }

    /// Returns the progress category.
    pub const fn kind(self) -> NativeFrameProgressKind {
        self.kind
    }

    /// Returns the suspended state ordinal.
    pub const fn state(self) -> u32 {
        self.state
    }

    /// Returns the compiler-owned completion or panic payload handle.
    pub const fn payload(self) -> usize {
        self.payload
    }
}

/// ABI-safe protected-frame lifecycle exit category.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeFrameExit(u32);

impl NativeFrameExit {
    /// The frame completed normally.
    pub const COMPLETED: Self = Self(0);
    /// The frame completed cancellation cleanup.
    pub const CANCELLED: Self = Self(1);
    /// The frame propagated a panic report.
    pub const PANICKED: Self = Self(2);
    /// Runtime infrastructure could not continue execution.
    pub const RUNTIME_FAILURE: Self = Self(3);
}

/// Callback returning runtime-visible facts for one frame-state ordinal.
pub type NativeFrameStateCallback =
    extern "C" fn(context: usize, state: u32) -> NativeFrameState;

/// Callback entering or resuming one compiler-generated frame.
pub type NativeFrameResumeCallback = extern "C" fn(
    context: usize,
    cancellation_requested: u8,
) -> NativeFrameProgress;

/// Callback performing one infallible generated frame action.
pub type NativeFrameActionCallback = extern "C" fn(context: usize);

/// Callback resolving generated frame lifecycle state for one terminal exit.
pub type NativeFrameResolveCallback =
    extern "C" fn(context: usize, exit: NativeFrameExit);

/// Complete ABI-safe adapter for one compiler-generated protected frame.
#[repr(C)]
#[derive(Debug)]
pub struct NativeProtectedFrame {
    context: usize,
    identity: [u8; 32],
    state_count: u32,
    size: usize,
    alignment: usize,
    completion_size: usize,
    completion_alignment: usize,
    state: NativeFrameStateCallback,
    resume: NativeFrameResumeCallback,
    broadcast_tasks: NativeFrameActionCallback,
    resolve_lifecycle: NativeFrameResolveCallback,
    destroy: NativeFrameActionCallback,
}

impl NativeProtectedFrame {
    /// Creates the complete generated-frame ABI adapter.
    #[expect(
        clippy::too_many_arguments,
        reason = "the frame ABI keeps independent layout and callback facts explicit"
    )]
    pub const fn new(
        context: usize,
        identity: [u8; 32],
        state_count: u32,
        size: usize,
        alignment: usize,
        completion_size: usize,
        completion_alignment: usize,
        state: NativeFrameStateCallback,
        resume: NativeFrameResumeCallback,
        broadcast_tasks: NativeFrameActionCallback,
        resolve_lifecycle: NativeFrameResolveCallback,
        destroy: NativeFrameActionCallback,
    ) -> Self {
        Self {
            context,
            identity,
            state_count,
            size,
            alignment,
            completion_size,
            completion_alignment,
            state,
            resume,
            broadcast_tasks,
            resolve_lifecycle,
            destroy,
        }
    }

    /// Returns the opaque generated-frame context.
    pub const fn context(&self) -> usize {
        self.context
    }

    /// Returns the stable frame representation identity.
    pub const fn identity(&self) -> [u8; 32] {
        self.identity
    }

    /// Returns the number of resumable frame states.
    pub const fn state_count(&self) -> u32 {
        self.state_count
    }

    /// Returns the frame storage size.
    pub const fn size(&self) -> usize {
        self.size
    }

    /// Returns the frame storage alignment.
    pub const fn alignment(&self) -> usize {
        self.alignment
    }

    /// Returns the completion storage size.
    pub const fn completion_size(&self) -> usize {
        self.completion_size
    }

    /// Returns the completion storage alignment.
    pub const fn completion_alignment(&self) -> usize {
        self.completion_alignment
    }

    /// Returns the state-description callback.
    pub const fn state(&self) -> NativeFrameStateCallback {
        self.state
    }

    /// Returns the frame-resume callback.
    pub const fn resume(&self) -> NativeFrameResumeCallback {
        self.resume
    }

    /// Returns the task-broadcast callback.
    pub const fn broadcast_tasks(&self) -> NativeFrameActionCallback {
        self.broadcast_tasks
    }

    /// Returns the lifecycle-resolution callback.
    pub const fn resolve_lifecycle(&self) -> NativeFrameResolveCallback {
        self.resolve_lifecycle
    }

    /// Returns the frame-destruction callback.
    pub const fn destroy(&self) -> NativeFrameActionCallback {
        self.destroy
    }
}

/// Callback waking a generated task observer.
pub type NativeWakeCallback = extern "C" fn(context: usize);

/// ABI-safe selected execution-lane category.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeExecutionLane(u32);

impl NativeExecutionLane {
    /// Cooperative work on a movable or pinned worker lane.
    pub const COOPERATIVE: Self = Self(0);
    /// Work on an origin-thread lane.
    pub const ORIGIN_THREAD: Self = Self(1);
    /// Work on the distinguished main-thread lane.
    pub const MAIN_THREAD: Self = Self(2);
    /// Work on a blocking lane.
    pub const BLOCKING: Self = Self(3);
    /// Work on a compute lane.
    pub const COMPUTE: Self = Self(4);
}

/// Result of selecting a compatible execution lane.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeExecutionLaneResult {
    status: NativeRuntimeStatus,
    lane: NativeExecutionLane,
}

impl NativeExecutionLaneResult {
    /// Creates one successful lane selection.
    pub const fn success(lane: NativeExecutionLane) -> Self {
        Self {
            status: NativeRuntimeStatus::SUCCESS,
            lane,
        }
    }

    /// Creates one failed lane selection.
    pub const fn failure(status: NativeRuntimeStatus) -> Self {
        Self {
            status,
            lane: NativeExecutionLane::COOPERATIVE,
        }
    }

    /// Returns the operation status.
    pub const fn status(self) -> NativeRuntimeStatus {
        self.status
    }

    /// Returns the selected lane after success.
    pub const fn lane(self) -> NativeExecutionLane {
        self.lane
    }
}

/// Runtime event callback accepted by the native artifact.
pub type NativeRuntimeEventCallback =
    extern "C" fn(context: usize) -> NativeRuntimeStatus;
