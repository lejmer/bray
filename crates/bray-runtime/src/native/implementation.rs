//! Internal implementation surface consumed by link-isolated native adapters.

pub use super::callback::{
    bray_runtime_current_native_thread_identity, bray_runtime_foreign_callback_execution,
    bray_runtime_main_native_thread_identity, bray_runtime_native_thread_execution,
    bray_runtime_synchronous_root_execution,
};
pub use super::export::{
    bray_runtime_awaited_frame_composition, bray_runtime_cleanup_incident_reporting,
    bray_runtime_cleanup_shield_enter, bray_runtime_cleanup_shield_leave,
    bray_runtime_compatible_lane_selection, bray_runtime_current_run_cancellation_observation,
    bray_runtime_current_run_cancellation_propagation, bray_runtime_entry_failure_reporting,
    bray_runtime_event, bray_runtime_frame_completion_move, bray_runtime_join_registration,
    bray_runtime_main_thread_lane_drive, bray_runtime_main_thread_lane_startup,
    bray_runtime_panic_propagation, bray_runtime_panic_report_destruction,
    bray_runtime_panic_reporting, bray_runtime_product_host_control,
    bray_runtime_root_cancellation_request, bray_runtime_root_completion_resolution,
    bray_runtime_root_execution, bray_runtime_root_terminal_observation,
    bray_runtime_substrate_initialization, bray_runtime_substrate_report_primary,
    bray_runtime_substrate_shutdown, bray_runtime_suspension_registration,
    bray_runtime_task_allocation, bray_runtime_task_cancellation_request,
    bray_runtime_task_destruction, bray_runtime_task_event_creation,
    bray_runtime_task_event_destruction, bray_runtime_task_event_signal,
    bray_runtime_task_observation_creation, bray_runtime_task_resolution, bray_runtime_task_start,
    bray_runtime_wake, resident_thread_attachment_identity,
    resident_thread_static_cleanup_registration,
};
#[cfg(feature = "test-output")]
pub use super::host::{NativeHostCallbacks, register_host_callbacks};

/// Borrows the resident host callback table.
pub const fn resident_host_services() -> &'static bray_runtime_abi::NativeHostServices {
    super::services::host()
}

/// Borrows the resident execution callback table.
pub const fn resident_execution_services() -> &'static bray_runtime_abi::NativeExecutionServices {
    super::services::execution()
}
