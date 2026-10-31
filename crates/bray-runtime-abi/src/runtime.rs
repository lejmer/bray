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
    /// Process-local identities for admitted cleanup were exhausted.
    pub const IDENTITY_EXHAUSTED: Self = Self(8);
    /// Required outgoing cleanup storage could not be admitted.
    pub const OUTGOING_STORAGE_UNAVAILABLE: Self = Self(9);

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
    /// Creates explicit limits for simultaneous task execution and pending timers.
    pub const fn new(task_capacity: usize, timer_capacity: usize) -> Self {
        Self {
            task_capacity,
            timer_capacity,
        }
    }

    /// Returns the maximum simultaneously executing task count.
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
#[derive(Debug)]
pub struct NativeRunOutcome {
    state: NativeRunState,
    payload: usize,
    report: crate::NativePanicReport,
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

/// Invoke a synchronous source root using caller-owned outcome and admitted report records.
/// The callback publishes its body outcome, returns unused records, and contains native unwind.
/// Entry returns SUCCESS on clean retirement, PANICKED when a terminal report was already
/// transferred into the outcome or consumed, and another status for an unrepresented failure.
/// Terminal status never revokes a completed result transferred to the caller.
pub type NativeSynchronousRootCallback = extern "C" fn(
    context: usize,
    outcome: &mut NativeRunOutcome,
    records: &mut crate::NativeReportRecords,
);

/// Callback invoking one synchronous Bray operation with an initially completed outcome.
/// Successful source calls may leave this caller-owned failure header untouched.
pub type NativeThreadOperationCallback =
    extern "C-unwind" fn(context: usize, outcome: &mut NativeRunOutcome);

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
    /// Creates a non-panic result. Panic ownership is published with `panicked`.
    pub const fn new(state: NativeRunState, payload: usize) -> Self {
        Self {
            state,
            payload,
            report: crate::NativePanicReport::empty(),
        }
    }
    /// Transfers the report before publishing its terminal tag.
    pub const fn panicked(report: crate::NativePanicReport) -> Self {
        Self {
            state: NativeRunState::PANICKED,
            payload: 0,
            report,
        }
    }
    /// Returns the terminal state.
    pub const fn state(&self) -> NativeRunState {
        self.state
    }
    /// Returns the completion address or runtime failure code.
    pub const fn payload(&self) -> usize {
        self.payload
    }
    /// Moves report ownership out of this terminal destination.
    pub fn take_report(&mut self) -> crate::NativePanicReport {
        std::mem::replace(&mut self.report, crate::NativePanicReport::empty())
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
#[derive(Debug)]
pub struct NativeFrameProgress {
    kind: NativeFrameProgressKind,
    state: u32,
    payload: usize,
    report: crate::NativePanicReport,
}

impl NativeFrameProgress {
    /// Creates one frame progress record.
    pub const fn new(kind: NativeFrameProgressKind, state: u32, payload: usize) -> Self {
        Self {
            kind,
            state,
            payload,
            report: crate::NativePanicReport::empty(),
        }
    }

    /// Publishes an owned report in the caller's progress destination.
    pub const fn panicked(report: crate::NativePanicReport) -> Self {
        Self {
            kind: NativeFrameProgressKind::PANICKED,
            state: 0,
            payload: 0,
            report,
        }
    }
    /// Takes the report published by the frame.
    pub fn take_report(&mut self) -> crate::NativePanicReport {
        std::mem::replace(&mut self.report, crate::NativePanicReport::empty())
    }

    /// Returns the progress category.
    pub const fn kind(&self) -> NativeFrameProgressKind {
        self.kind
    }

    /// Returns the suspended state ordinal.
    pub const fn state(&self) -> u32 {
        self.state
    }

    /// Returns the compiler-owned completion or panic payload handle.
    pub const fn payload(&self) -> usize {
        self.payload
    }
}

/// Ownership disposition selected before a child enters its enclosing run.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NativeFrameComposition {
    /// Transfer the produced result and retire the child.
    Completion = 0,
    /// Resolve captures and retire the child without entering its body.
    Abandon = 1,
    /// Resolve child work while retaining capture storage for destruction.
    Quiesce = 2,
}

impl NativeFrameComposition {
    /// Decodes the composition boundary's ownership disposition.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Completion),
            1 => Some(Self::Abandon),
            2 => Some(Self::Quiesce),
            _ => None,
        }
    }

    /// Returns the native boundary code.
    pub const fn code(self) -> u8 {
        self as u8
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
    /// Resolve child work without executing or destroying inactive captures.
    pub const QUIESCENCE: Self = Self(4);
    /// Synchronously destroy captures whose child work has already quiesced.
    pub const RETAINED_DESTRUCTION: Self = Self(5);
    /// Resolve checked inactive captures synchronously without a task continuation.
    pub const SYNCHRONOUS_DISPOSAL: Self = Self(6);
}

/// Requirements for one continuation and callback operation, using activation's
/// resume, cancel, broadcast, resolve, completion-move and destruction ordinals.
pub type NativeFrameStateCallback =
    extern "C" fn(context: usize, state: u32, operation: u32, argument: usize) -> NativeFrameState;

/// Callback entering or resuming one compiler-generated frame.
pub type NativeFrameResumeCallback =
    extern "C-unwind" fn(destination: &mut NativeFrameProgress, context: usize);

/// Callback entering generated cancellation cleanup for one frame.
pub type NativeFrameCancellationCallback =
    extern "C-unwind" fn(destination: &mut NativeFrameProgress, context: usize);

/// Callback performing one infallible generated frame action.
pub type NativeFrameActionCallback = extern "C-unwind" fn(context: usize);

/// Callback resolving a value into a caller-initialized successful outcome.
/// A later native unwind must preserve any report already written to that outcome.
pub type NativeValueCleanupCallback =
    extern "C-unwind" fn(payload: *mut u8, outcome: &mut NativeRunOutcome);

/// Callback moving a generated completion value into runtime-owned storage.
pub type NativeFrameCompletionMoveCallback =
    extern "C-unwind" fn(context: usize, destination: usize);

/// Callback resolving generated frame lifecycle state for one terminal exit.
pub type NativeFrameResolveCallback = extern "C-unwind" fn(
    destination: &mut NativeFrameProgress,
    context: usize,
    exit: NativeFrameExit,
);

/// Address of an inactive token retained by the caller through admission.
/// Success transfers its owner to the runtime. Failure leaves it with the caller.
#[repr(transparent)]
#[derive(Debug)]
pub struct NativeInactiveFrameTransfer(usize);

impl NativeInactiveFrameTransfer {
    /// Borrows the exact inactive token for one activation attempt.
    pub fn new(frame: &NativeInactiveFrame) -> Self {
        Self(std::ptr::from_ref(frame).addr())
    }

    /// Returns the address of the retained token slot.
    pub const fn address(&self) -> usize {
        self.0
    }
}

/// One inactive frame lease and its immutable producer description.
/// Copying the native representation does not copy ownership. Resident admission
/// checks the lease epoch and registered descriptor before activating its storage.
#[repr(C)]
#[derive(Debug)]
pub struct NativeInactiveFrame {
    lease: crate::NativeCleanupStorageLease,
    descriptor: usize,
}

impl NativeInactiveFrame {
    /// Creates an empty native output slot. It grants no activation authority.
    pub const fn empty() -> Self {
        Self {
            lease: crate::NativeCleanupStorageLease::empty(),
            descriptor: 0,
        }
    }

    /// Transfers the published lease while its producer retains this descriptor
    /// and all callback code until the frame's consuming destruction returns.
    pub fn new(
        lease: crate::NativeCleanupStorageLease,
        descriptor: &crate::NativeActivationDescriptor,
    ) -> Self {
        Self {
            lease,
            descriptor: descriptor as *const crate::NativeActivationDescriptor as usize,
        }
    }

    /// Returns the lease identity without granting access or dispatch authority.
    pub const fn lease(&self) -> crate::NativeCleanupStorageLease {
        self.lease
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
        NativeInactiveFrameTransfer, NativeLaneRequirements, NativeRootHandle, NativeRootStart,
        NativeRunOutcome, NativeRunState, NativeRuntimeConfiguration, NativeRuntimeStatus,
        NativeStringView, NativeTaskAllocation, NativeTaskHandle,
    };
    use crate::NativePanicCause;

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
        assert_abi_layout!(NativeInactiveFrameTransfer, size: 8, align: 8, fields: {});
        assert_abi_layout!(NativeExecutionLane, size: 4, align: 4, fields: {});
    }

    #[test]
    fn runtime_records_have_the_native_abi_layout() {
        assert_abi_layout!(NativeRuntimeConfiguration, size: 16, align: 8, fields: {
            task_capacity: 0,
            timer_capacity: 8,
        });

        assert_abi_layout!(NativeRunOutcome, size: 168, align: 8, fields: {
            state: 0,
            payload: 8,
            report: 16,
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

        assert_abi_layout!(NativeFrameProgress, size: 168, align: 8, fields: {
            kind: 0,
            state: 4,
            payload: 8,
            report: 16,
        });

        assert_abi_layout!(NativeInactiveFrame, size: 48, align: 8, fields: {
            lease: 0,
            descriptor: 40,
        });

        assert_abi_layout!(NativeExecutionLaneResult, size: 8, align: 4, fields: {
            status: 0,
            lane: 4,
        });
    }
}
