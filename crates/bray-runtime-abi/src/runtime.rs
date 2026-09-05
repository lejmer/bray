// rust-style: allow(module-too-large, reason = "native ABI symbols and wire records form one flat contract catalog")

/// Stable symbol observing one successful generated memory allocation.
pub const MEMORY_ALLOCATION_OBSERVATION_SYMBOL: &str = "bray_runtime_memory_allocation_observation";

/// Stable symbol observing one completed generated memory transfer.
pub const MEMORY_COPY_OBSERVATION_SYMBOL: &str = "bray_runtime_memory_copy_observation";

/// Stable symbol starting one generated memory-observation session.
pub const MEMORY_OBSERVATION_BEGIN_SYMBOL: &str = "bray_runtime_memory_observation_begin";

/// Stable symbol starting the measured Bray-controlled execution interval.
pub const PERFORMANCE_INTERVAL_BEGIN_SYMBOL: &str = "bray_runtime_performance_interval_begin";

/// Stable symbol ending and recording the Bray-controlled execution interval.
pub const PERFORMANCE_INTERVAL_END_SYMBOL: &str = "bray_runtime_performance_interval_end";

/// Per-process file selected for opt-in performance observations.
pub const PERFORMANCE_OBSERVATION_PATH_ENVIRONMENT: &str = "BRAY_PERFORMANCE_OBSERVATION_PATH";

/// Versioned fixed-record performance observation stream header.
pub const PERFORMANCE_OBSERVATION_HEADER: [u8; 8] = *b"BRAYPO01";

/// Maximum number of performance events retained by one observed run.
pub const MAX_PERFORMANCE_OBSERVATION_RECORDS: u64 = 1_000_000;

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

/// Structured cause retained by one native panic report.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativePanicCause(u32);

impl NativePanicCause {
    /// An explicit language `panic` expression.
    pub const MESSAGE: Self = Self(0);
    /// A failed built-in assertion.
    pub const ASSERTION: Self = Self(1);
    /// An explicit failure produced by `std.testing.fail`.
    pub const EXPLICIT_TEST_FAILURE: Self = Self(2);

    /// Returns whether this cause is defined by the current native ABI.
    pub const fn is_known(&self) -> bool {
        matches!(self.0, 0..=2)
    }

    /// Returns the stable native ABI code.
    pub const fn code(self) -> u32 {
        self.0
    }
}

/// Exact source occurrence retained by a native panic report.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeSourceAnchor {
    present: u32,
    source: u32,
    start: u32,
    end: u32,
    version: u64,
}

impl NativeSourceAnchor {
    /// Creates one source anchor from its stable scalar ABI fields.
    pub const fn new(source: u32, start: u32, end: u32, version: u64) -> Self {
        Self {
            present: 1,
            source,
            start,
            end,
            version,
        }
    }

    /// Creates an anchor for generated or imported code without local source coordinates.
    pub const fn unavailable() -> Self {
        Self {
            present: 0,
            source: 0,
            start: 0,
            end: 0,
            version: 0,
        }
    }

    /// Returns whether this anchor carries local source coordinates.
    pub const fn is_available(self) -> bool {
        self.present == 1
    }

    /// Returns the source snapshot identity.
    pub const fn source(self) -> u32 {
        self.source
    }

    /// Returns the inclusive UTF-8 byte start offset.
    pub const fn start(self) -> u32 {
        self.start
    }

    /// Returns the exclusive UTF-8 byte end offset.
    pub const fn end(self) -> u32 {
        self.end
    }

    /// Returns the logical source revision.
    pub const fn version(self) -> u64 {
        self.version
    }

    /// Returns whether the half-open source range is ordered.
    pub const fn is_valid(&self) -> bool {
        match self.present {
            0 => self.source == 0 && self.start == 0 && self.end == 0 && self.version == 0,
            1 => self.start <= self.end,
            _ => false,
        }
    }
}

/// Borrowed UTF-8 message accepted by the panic-report construction ABI.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeStringView {
    data: *const u8,
    length: usize,
}

impl NativeStringView {
    /// Creates one borrowed byte view.
    pub const fn new(data: *const u8, length: usize) -> Self {
        Self { data, length }
    }

    /// Returns the borrowed byte address.
    pub const fn data(self) -> *const u8 {
        self.data
    }

    /// Returns the borrowed byte length.
    pub const fn length(self) -> usize {
        self.length
    }
}

/// Callback invoking one synchronous source root and writing its explicit terminal outcome.
pub type NativeSynchronousRootCallback =
    extern "C" fn(destination: usize, outcome: &mut NativeRunOutcome);

/// Outcome returned through the hidden context of one synchronous Bray callback.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeBrayCallOutcome(usize);

impl NativeBrayCallOutcome {
    /// Creates the value representing normal completion.
    pub const fn completed() -> Self {
        Self(0)
    }

    /// Creates the value representing propagated cancellation.
    pub const fn cancelled() -> Self {
        Self(1)
    }

    /// Creates the value representing one owned panic report.
    pub const fn panicked(report: usize) -> Option<Self> {
        if report <= Self::cancelled().raw() {
            return None;
        }

        Some(Self(report))
    }

    /// Returns whether the call completed normally.
    pub const fn is_completed(self) -> bool {
        self.0 == Self::completed().raw()
    }

    /// Returns whether the call propagated cancellation.
    pub const fn is_cancelled(self) -> bool {
        self.0 == Self::cancelled().raw()
    }

    /// Returns the owned panic report when the call panicked.
    pub const fn panic_report(self) -> Option<usize> {
        if self.0 <= Self::cancelled().raw() {
            return None;
        }

        Some(self.0)
    }

    /// Returns the target-sized ABI value.
    pub const fn raw(self) -> usize {
        self.0
    }
}

/// Callback invoking one synchronous Bray operation on a native thread.
pub type NativeThreadOperationCallback =
    extern "C" fn(context: usize, outcome: &mut NativeBrayCallOutcome);

/// Callback observing cancellation for one Bray-owned native thread.
pub type NativeThreadCancellationCallback = extern "C" fn(context: usize) -> u32;

/// Stable process-local handle for one host-owned executable root.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativeRootHandle(u64);

impl NativeRootHandle {
    /// Creates one nonzero native root handle.
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

/// Result of transferring one generated frame into a host-owned root run.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeRootStart {
    status: NativeRuntimeStatus,
    root: u64,
}

impl NativeRootStart {
    /// Creates one successful root transfer.
    pub const fn success(root: NativeRootHandle) -> Self {
        Self {
            status: NativeRuntimeStatus::SUCCESS,
            root: root.raw(),
        }
    }

    /// Creates one failed root transfer without a usable handle.
    pub const fn failure(status: NativeRuntimeStatus) -> Self {
        Self { status, root: 0 }
    }

    /// Returns the root-transfer status.
    pub const fn status(self) -> NativeRuntimeStatus {
        self.status
    }

    /// Returns the root handle after success.
    pub const fn root(self) -> Option<NativeRootHandle> {
        NativeRootHandle::new(self.root)
    }
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

/// Runtime-visible state for one generated protected frame.
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
    /// The frame yielded voluntarily at the returned state.
    pub const YIELDED: Self = Self(5);
    /// The frame awaits the task event identified by its payload.
    pub const TASK_EVENT: Self = Self(6);

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
    pub const fn new(kind: NativeFrameProgressKind, state: u32, payload: usize) -> Self {
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

/// Callback returning runtime-visible state for one frame-state ordinal.
pub type NativeFrameStateCallback = extern "C" fn(context: usize, state: u32) -> NativeFrameState;

/// Callback entering or resuming one compiler-generated frame.
pub type NativeFrameResumeCallback = extern "C-unwind" fn(context: usize) -> NativeFrameProgress;

/// Callback entering generated cancellation cleanup for one frame.
pub type NativeFrameCancellationCallback =
    extern "C-unwind" fn(context: usize) -> NativeFrameProgress;

/// Callback performing one infallible generated frame action.
pub type NativeFrameActionCallback = extern "C-unwind" fn(context: usize);

/// Callback moving a generated completion value into runtime-owned storage.
pub type NativeFrameCompletionMoveCallback =
    extern "C-unwind" fn(context: usize, destination: usize);

/// Callback consuming an inactive context into a protected-frame adapter.
pub type NativeFrameMoveBeforeStartCallback = extern "C" fn(context: usize) -> NativeProtectedFrame;

/// Callback resolving generated frame lifecycle state for one terminal exit.
pub type NativeFrameResolveCallback = extern "C-unwind" fn(context: usize, exit: NativeFrameExit);

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
    cancel: NativeFrameCancellationCallback,
    broadcast_tasks: NativeFrameActionCallback,
    resolve_lifecycle: NativeFrameResolveCallback,
    move_completion: NativeFrameCompletionMoveCallback,
    destroy: NativeFrameActionCallback,
}

impl NativeProtectedFrame {
    /// Creates the complete generated-frame ABI adapter.
    #[expect(
        clippy::too_many_arguments,
        reason = "the frame ABI keeps independent layout and callback state explicit"
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
        cancel: NativeFrameCancellationCallback,
        broadcast_tasks: NativeFrameActionCallback,
        resolve_lifecycle: NativeFrameResolveCallback,
        move_completion: NativeFrameCompletionMoveCallback,
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
            cancel,
            broadcast_tasks,
            resolve_lifecycle,
            move_completion,
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

    /// Returns the cancellation-entry callback.
    pub const fn cancel(&self) -> NativeFrameCancellationCallback {
        self.cancel
    }

    /// Returns the task-broadcast callback.
    pub const fn broadcast_tasks(&self) -> NativeFrameActionCallback {
        self.broadcast_tasks
    }

    /// Returns the lifecycle-resolution callback.
    pub const fn resolve_lifecycle(&self) -> NativeFrameResolveCallback {
        self.resolve_lifecycle
    }

    /// Returns the completion-move callback.
    pub const fn move_completion(&self) -> NativeFrameCompletionMoveCallback {
        self.move_completion
    }

    /// Returns the frame-destruction callback.
    pub const fn destroy(&self) -> NativeFrameActionCallback {
        self.destroy
    }
}

/// One-shot address transferring a protected frame into the native runtime.
///
/// The caller must keep the referenced descriptor alive until the runtime call
/// returns and must not use its generated-frame context after the call. The
/// runtime copies the descriptor immediately and owns destruction from that
/// point, including when validation or task publication fails.
#[repr(transparent)]
#[derive(Debug)]
pub struct NativeProtectedFrameTransfer(usize);

impl NativeProtectedFrameTransfer {
    /// Creates a transfer for a descriptor that remains live during the call.
    pub fn new(frame: &NativeProtectedFrame) -> Self {
        Self(frame as *const NativeProtectedFrame as usize)
    }

    /// Returns the address of the transferred descriptor.
    pub const fn address(&self) -> usize {
        self.0
    }
}

/// One inactive compiler-generated frame whose ownership has not entered the runtime.
#[repr(C)]
#[derive(Debug)]
pub struct NativeInactiveFrame {
    context: usize,
    move_before_start: NativeFrameMoveBeforeStartCallback,
}

impl NativeInactiveFrame {
    /// Creates an inactive frame ownership transfer.
    pub const fn new(
        context: usize,
        move_before_start: NativeFrameMoveBeforeStartCallback,
    ) -> Self {
        Self {
            context,
            move_before_start,
        }
    }

    /// Consumes the inactive frame and transfers its context into the adapter.
    pub fn into_protected(self) -> NativeProtectedFrame {
        (self.move_before_start)(self.context)
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
pub type NativeRuntimeEventCallback = extern "C" fn(context: usize) -> NativeRuntimeStatus;

#[cfg(test)]
mod tests {
    use super::{
        NativeExecutionLane, NativeExecutionLaneResult, NativeFrameAffinity, NativeFrameExit,
        NativeFrameProgress, NativeFrameProgressKind, NativeFrameState, NativeInactiveFrame,
        NativeLaneRequirements, NativePanicCause, NativeProtectedFrame,
        NativeProtectedFrameTransfer, NativeRootHandle, NativeRootStart, NativeRunOutcome,
        NativeRunState, NativeRuntimeConfiguration, NativeRuntimeStatus, NativeSourceAnchor,
        NativeStringView, NativeTaskAllocation, NativeTaskHandle,
    };

    #[test]
    fn scalar_runtime_values_have_the_native_abi_layout() {
        assert_abi_layout!(NativeRuntimeStatus, size: 4, align: 4, fields: {});
        assert_abi_layout!(NativeRunState, size: 4, align: 4, fields: {});
        assert_abi_layout!(NativePanicCause, size: 4, align: 4, fields: {});
        assert_abi_layout!(NativeRootHandle, size: 8, align: 8, fields: {});
        assert_abi_layout!(NativeTaskHandle, size: 8, align: 8, fields: {});
        assert_abi_layout!(NativeFrameAffinity, size: 4, align: 4, fields: {});
        assert_abi_layout!(NativeLaneRequirements, size: 4, align: 4, fields: {});
        assert_abi_layout!(NativeFrameProgressKind, size: 4, align: 4, fields: {});
        assert_abi_layout!(NativeFrameExit, size: 4, align: 4, fields: {});
        assert_abi_layout!(NativeProtectedFrameTransfer, size: 8, align: 8, fields: {});
        assert_abi_layout!(NativeExecutionLane, size: 4, align: 4, fields: {});
    }

    #[test]
    fn runtime_records_have_the_native_abi_layout() {
        assert_abi_layout!(NativeRuntimeConfiguration, size: 16, align: 8, fields: {
            task_capacity: 0,
            timer_capacity: 8,
        });

        assert_abi_layout!(NativeRunOutcome, size: 16, align: 8, fields: {
            state: 0,
            payload: 8,
        });

        assert_abi_layout!(NativeSourceAnchor, size: 24, align: 8, fields: {
            present: 0,
            source: 4,
            start: 8,
            end: 12,
            version: 16,
        });

        assert_abi_layout!(NativeStringView, size: 16, align: 8, fields: {
            data: 0,
            length: 8,
        });

        assert_abi_layout!(NativeRootStart, size: 16, align: 8, fields: {
            status: 0,
            root: 8,
        });

        assert_abi_layout!(NativeTaskAllocation, size: 16, align: 8, fields: {
            status: 0,
            task: 8,
        });

        assert_abi_layout!(NativeFrameState, size: 8, align: 4, fields: {
            affinity: 0,
            lane_requirements: 4,
        });

        assert_abi_layout!(NativeFrameProgress, size: 16, align: 8, fields: {
            kind: 0,
            state: 4,
            payload: 8,
        });

        assert_abi_layout!(NativeInactiveFrame, size: 16, align: 8, fields: {
            context: 0,
            move_before_start: 8,
        });

        assert_abi_layout!(NativeExecutionLaneResult, size: 8, align: 4, fields: {
            status: 0,
            lane: 4,
        });
    }

    #[test]
    fn protected_frame_descriptor_has_the_native_abi_layout() {
        assert_abi_layout!(NativeProtectedFrame, size: 136, align: 8, fields: {
            context: 0,
            identity: 8,
            state_count: 40,
            size: 48,
            alignment: 56,
            completion_size: 64,
            completion_alignment: 72,
            state: 80,
            resume: 88,
            cancel: 96,
            broadcast_tasks: 104,
            resolve_lifecycle: 112,
            move_completion: 120,
            destroy: 128,
        });
    }
}
