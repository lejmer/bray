//! Internal implementation surface consumed by link-isolated native adapters.

pub use super::callback::{
    bray_runtime_foreign_callback_execution_v1, bray_runtime_synchronous_root_execution_v1,
};
pub use super::export::{
    bray_runtime_awaited_frame_composition_v1, bray_runtime_cleanup_incident_reporting_v1,
    bray_runtime_compatible_lane_selection_v1,
    bray_runtime_current_run_cancellation_observation_v1,
    bray_runtime_current_run_cancellation_propagation_v1, bray_runtime_entry_failure_reporting_v1,
    bray_runtime_event_v1, bray_runtime_frame_completion_move_v1,
    bray_runtime_join_registration_v1, bray_runtime_main_thread_lane_drive_v1,
    bray_runtime_main_thread_lane_startup_v1, bray_runtime_panic_propagation_v1,
    bray_runtime_panic_report_construction_v1, bray_runtime_panic_reporting_v1,
    bray_runtime_product_host_control_v1, bray_runtime_root_cancellation_request_v1,
    bray_runtime_root_completion_resolution_v1, bray_runtime_root_execution_v1,
    bray_runtime_root_terminal_observation_v1, bray_runtime_structured_shutdown_v1,
    bray_runtime_suspension_registration_v1, bray_runtime_task_allocation_v1,
    bray_runtime_task_cancellation_request_v1, bray_runtime_task_start_v1,
    bray_runtime_task_destruction_v1, bray_runtime_task_observation_creation_v1,
    bray_runtime_task_resolution_v1,
    bray_runtime_terminal_publication_v1, bray_runtime_thread_attachment_identity_v1,
    bray_runtime_thread_static_cleanup_registration_v1, bray_runtime_wake_v1,
};
#[cfg(feature = "test-output")]
pub use super::host::{NativeHostCallbacks, register_host_callbacks};
