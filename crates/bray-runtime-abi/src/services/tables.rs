//! Fixed native operations shared by providers bound to one resident implementation.
use crate::{
    NativeCleanupCapacityMetadata, NativeCleanupCapacityMetadataProvider, NativeCleanupIncident,
    NativeExecutionLaneResult, NativeFrameMetadata, NativeInactiveFrame,
    NativePanicMessageCopyCallback, NativeProductHostDescriptor, NativeProductHostObservation,
    NativeProductHostOperation, NativeProductServices, NativeProviderRetention,
    NativeRootConstructor, NativeRootHandle, NativeRootStart, NativeRunOutcome,
    NativeRunResultLayout, NativeRuntimeConfiguration, NativeRuntimeStatus, NativeSourceAnchor,
    NativeSynchronousRootCallback, NativeTaskAllocation, NativeTaskHandle,
    NativeTaskTerminalCleanup, NativeThreadCancellationCallback, NativeThreadOperationCallback,
    NativeThreadStaticCleanupRegistration, NativeTypeIdentity, NativeValueCleanup,
    NativeWakeCallback,
};

/// Resident host operations. The table and callbacks outlive every bound provider.
#[derive(Debug)]
#[repr(C)]
pub struct NativeHostServices {
    /// Current service ABI version, initialized to one.
    pub version: u32,
    /// Reserved, initialized to zero.
    pub reserved: u32,
    /// Routes synchronous root execution through the resident host service.
    pub synchronous_root_execution:
        extern "C" fn(NativeSynchronousRootCallback, usize) -> NativeRunOutcome,
    /// Routes foreign callback execution through the resident host service.
    pub foreign_callback_execution:
        extern "C" fn(NativeSynchronousRootCallback, usize) -> NativeRunOutcome,
    /// Routes native thread execution through the resident host service.
    pub native_thread_execution: extern "C" fn(
        NativeThreadOperationCallback,
        usize,
        NativeThreadCancellationCallback,
        usize,
        &mut usize,
    ) -> u32,
    /// Routes substrate panic reporting through the resident host service.
    pub substrate_panic_reporting: extern "C" fn(
        u32,
        u32,
        u32,
        u32,
        u32,
        u64,
        *const u8,
        usize,
        Option<NativePanicMessageCopyCallback>,
    ) -> NativeRuntimeStatus,
    /// Recover one owned panic report published by a native-thread boundary.
    pub native_thread_panic_report_recovery: extern "C" fn(usize) -> usize,
    /// Report the type and source of one retained cleanup error.
    pub cleanup_incident_detail_reporting:
        extern "C" fn(&NativeCleanupIncident) -> NativeRuntimeStatus,
    /// Consume one valid cleanup incident, retaining it in the current run or destroying it on rejection.
    pub cleanup_incident_transfer: extern "C" fn(&NativeCleanupIncident) -> NativeRuntimeStatus,
    /// Report an entrypoint failure and resolve its owned value before host completion.
    pub entry_failure_resolution: extern "C" fn(
        &NativeTypeIdentity,
        &NativeSourceAnchor,
        usize,
        Option<&NativeValueCleanup>,
        Option<&NativeValueCleanup>,
    ) -> NativeRuntimeStatus,
    /// Defer cancellation delivery during current-run cleanup.
    pub cleanup_shield_enter: extern "C" fn(),
    /// Restore current-run cancellation delivery after cleanup.
    pub cleanup_shield_leave: extern "C" fn(),
    /// Observe cancellation requested for the current run.
    pub current_run_cancellation_observation: extern "C" fn() -> u8,
    /// Transfer current-run cancellation to the nearest run boundary.
    pub current_run_cancellation_propagation: extern "C-unwind" fn() -> !,
    /// Read the process-wide identity of the current Bray native thread.
    pub current_native_thread_identity: extern "C" fn() -> u64,
    /// Read the process-wide identity of the distinguished initial native thread.
    pub main_native_thread_identity: extern "C" fn() -> u64,
    /// Routes product host control through the resident host service.
    pub product_host_control: extern "C" fn(
        &NativeProductHostDescriptor,
        NativeProductHostOperation,
        Option<&NativeProductServices>,
    ) -> NativeProductHostObservation,
    /// Retain an already live provider in an empty caller-owned destination without allocation.
    pub provider_retention: extern "C" fn(
        &NativeProductHostDescriptor,
        &mut NativeProviderRetention,
    ) -> NativeRuntimeStatus,
    /// Routes thread attachment identity through the resident host service.
    pub thread_attachment_identity: extern "C" fn(&'static NativeProductHostDescriptor) -> u64,
    /// Routes thread static cleanup registration through the resident host service.
    pub thread_static_cleanup_registration:
        extern "C" fn(&NativeThreadStaticCleanupRegistration) -> NativeRuntimeStatus,
    /// Secure a complete generated cleanup bundle before owner construction.
    pub cleanup_capacity_admission: extern "C" fn(
        &NativeProductHostDescriptor,
        usize,
        Option<NativeCleanupCapacityMetadataProvider>,
    ) -> NativeRuntimeStatus,
    /// Retire a generated cleanup capacity credit after ownership is resolved.
    pub cleanup_capacity_discharge: extern "C" fn(
        &NativeProductHostDescriptor,
        &NativeCleanupCapacityMetadata,
    ) -> NativeRuntimeStatus,
}

/// Optional resident execution operations; shares exactly the selected host service.
#[derive(Debug)]
#[repr(C)]
pub struct NativeExecutionServices {
    /// Current service ABI version, initialized to one.
    pub version: u32,
    /// Reserved, initialized to zero.
    pub reserved: u32,
    /// Identity of the resident host used by this execution implementation.
    pub host: &'static NativeHostServices,
    /// Observe the root terminal record without creating a source task.
    pub root_terminal_observation: extern "C" fn(NativeRootHandle) -> NativeRunOutcome,
    /// Release runtime-owned root completion storage after host resolution.
    pub root_completion_resolution: extern "C" fn(NativeRootHandle) -> NativeRuntimeStatus,
    /// Report and destroy product-host cleanup incidents.
    pub cleanup_incident_reporting: extern "C" fn() -> NativeRuntimeStatus,
    /// Routes substrate initialization through the resident execution service.
    pub substrate_initialization: extern "C" fn(usize, usize) -> NativeRuntimeStatus,
    /// Routes substrate shutdown through the resident execution service.
    pub substrate_shutdown: extern "C" fn() -> NativeRuntimeStatus,
    /// Create one inactive erased protected frame.
    pub frame_storage_admission: extern "C" fn(Option<&NativeFrameMetadata>) -> usize,
    /// Activate reserved generated cleanup storage without allocating.
    pub frame_storage_activation: extern "C" fn(Option<&NativeFrameMetadata>) -> usize,
    /// Release resolved generated frame storage and unused cleanup capacity.
    pub frame_storage_release: extern "C" fn(usize),
    /// Begin and own the executable root run.
    pub root_execution: extern "C" fn(
        Option<&NativeFrameMetadata>,
        NativeRootConstructor,
        NativeRuntimeConfiguration,
    ) -> NativeRootStart,
    /// Initialize the distinguished main-thread execution lane.
    pub main_thread_lane_startup: extern "C" fn(NativeRuntimeConfiguration) -> NativeRuntimeStatus,
    /// Drive work assigned to the distinguished main-thread lane.
    pub main_thread_lane_drive: extern "C" fn() -> NativeRuntimeStatus,
    /// Allocate stable task-owned storage.
    pub task_allocation: extern "C" fn() -> NativeTaskAllocation,
    /// Publish a task, consuming its inactive frame only on success.
    pub task_start: extern "C" fn(NativeTaskHandle, NativeInactiveFrame) -> NativeRuntimeStatus,
    /// Exclusively borrow an available completed task value until release.
    pub task_completion_borrow:
        extern "C" fn(NativeTaskHandle, Option<&mut usize>) -> NativeRuntimeStatus,
    /// Return an exclusively borrowed completion to its task owner.
    pub task_completion_borrow_release: extern "C" fn(NativeTaskHandle) -> NativeRuntimeStatus,
    /// Transfer one task's available terminal result.
    pub task_resolution: extern "C" fn(
        NativeTaskHandle,
        *mut u8,
        Option<&NativeRunResultLayout>,
    ) -> NativeRuntimeStatus,
    /// Infallibly destroy one terminal task control record.
    pub task_destruction: extern "C" fn(
        NativeTaskHandle,
        Option<&'static NativeTaskTerminalCleanup>,
    ) -> NativeRuntimeStatus,
    /// Move one inactive erased protected frame before first resume.
    pub awaited_frame_composition: extern "C-unwind" fn(NativeInactiveFrame, u8),
    /// Synchronously destroy quiescent inactive captures and return their Bray call outcome.
    pub inactive_capture_destruction: extern "C-unwind" fn(NativeInactiveFrame) -> usize,
    /// Broadcast cancellation to tasks reachable from one frame.
    pub awaited_frame_resolution:
        extern "C" fn(*mut u8, &NativeRunResultLayout) -> NativeRuntimeStatus,
    /// Create one runtime-owned task event.
    pub task_event_creation: extern "C" fn() -> usize,
    /// Signal one runtime-owned task event.
    pub task_event_signal: extern "C" fn(usize) -> NativeRuntimeStatus,
    /// Release one runtime-owned task event.
    pub task_event_destruction: extern "C" fn(usize) -> NativeRuntimeStatus,
    /// Request another dispatch of a run.
    pub wake: extern "C" fn(NativeTaskHandle) -> NativeRuntimeStatus,
    /// Register one observer for a task terminal state.
    pub join_registration:
        extern "C" fn(NativeTaskHandle, NativeWakeCallback, usize) -> NativeRunOutcome,
    /// Select a lane compatible with checked execution requirements.
    pub compatible_lane_selection:
        extern "C" fn(NativeTaskHandle, u32) -> NativeExecutionLaneResult,
    /// Request cancellation of the root run from its host.
    pub root_cancellation_request: extern "C" fn(NativeRootHandle) -> NativeRuntimeStatus,
    /// Request cancellation of a child task.
    pub task_cancellation_request: extern "C" fn(NativeTaskHandle) -> NativeRuntimeStatus,
    /// Request cancellation of the current frame's attached child.
    pub awaited_frame_cancellation_request: extern "C-unwind" fn(),
    /// Admit execution and control lifecycle for a product with asynchronous cleanup.
    pub asynchronous_product_host_control: extern "C" fn(
        &NativeProductHostDescriptor,
        NativeProductHostOperation,
        Option<&NativeProductServices>,
    ) -> NativeProductHostObservation,
    /// Admit entry result storage and asynchronous cleanup before invocation.
    pub entry_result_admission: extern "C" fn(
        Option<&NativeProductHostDescriptor>,
        usize,
        usize,
        usize,
        Option<&NativeValueCleanup>,
        Option<&NativeValueCleanup>,
        Option<&mut usize>,
    ) -> NativeTaskAllocation,
    /// Release admitted entry storage or resolve its returned error.
    pub entry_result_resolution: extern "C" fn(u64, u8) -> NativeRuntimeStatus,
}

impl NativeExecutionServices {
    /// Checks the execution ABI and its ownership by the selected resident host.
    pub fn is_compatible(&self, host: &NativeHostServices) -> bool {
        self.version == 1 && self.reserved == 0 && std::ptr::eq(self.host, host)
    }
}

#[cfg(test)]
mod tests {
    use super::{NativeExecutionServices, NativeHostServices};

    #[test]
    fn resident_service_tables_have_fixed_pointer_slots() {
        let word = std::mem::size_of::<usize>();

        assert_abi_layout!(NativeHostServices, size: 8 + 20 * word, align: word, fields: {
            version: 0,
            reserved: 4,
            synchronous_root_execution: 8,
            cleanup_capacity_discharge: 8 + 19 * word,
        });

        assert_abi_layout!(NativeExecutionServices, size: 8 + 33 * word, align: word, fields: {
            version: 0,
            reserved: 4,
            host: 8,
            root_terminal_observation: 8 + word,
            entry_result_resolution: 8 + 32 * word,
        });
    }
}
