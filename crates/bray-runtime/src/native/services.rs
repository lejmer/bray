use bray_runtime_abi::{NativeExecutionServices, NativeHostServices};

use super::implementation;

pub(crate) static HOST_SERVICES: NativeHostServices = NativeHostServices {
    version: 1,
    reserved: 0,
    synchronous_root_execution: implementation::bray_runtime_synchronous_root_execution,
    foreign_callback_execution: implementation::bray_runtime_foreign_callback_execution,
    native_thread_execution: implementation::bray_runtime_native_thread_execution,
    substrate_panic_reporting: implementation::bray_runtime_substrate_panic_reporting,
    native_thread_panic_report_recovery:
        implementation::bray_runtime_native_thread_panic_report_recovery,
    cleanup_incident_detail_reporting:
        implementation::bray_runtime_cleanup_incident_detail_reporting,
    cleanup_incident_transfer: implementation::bray_runtime_cleanup_incident_transfer,
    entry_failure_resolution: implementation::bray_runtime_entry_failure_resolution,
    cleanup_shield_enter: implementation::bray_runtime_cleanup_shield_enter,
    cleanup_shield_leave: implementation::bray_runtime_cleanup_shield_leave,
    current_run_cancellation_observation:
        implementation::bray_runtime_current_run_cancellation_observation,
    current_run_cancellation_propagation:
        implementation::bray_runtime_current_run_cancellation_propagation,
    current_native_thread_identity: implementation::bray_runtime_current_native_thread_identity,
    main_native_thread_identity: implementation::bray_runtime_main_native_thread_identity,
    product_host_control: implementation::bray_runtime_product_host_control,
    provider_retention: implementation::bray_runtime_provider_retention,
    thread_attachment_identity: implementation::bray_runtime_thread_attachment_identity,
    thread_static_cleanup_registration:
        implementation::bray_runtime_thread_static_cleanup_registration,
    cleanup_capacity_admission: implementation::bray_runtime_cleanup_capacity_admission,
    cleanup_capacity_discharge: implementation::bray_runtime_cleanup_capacity_discharge,
};

pub(super) static EXECUTION_SERVICES: NativeExecutionServices = NativeExecutionServices {
    version: 1,
    reserved: 0,
    host: &HOST_SERVICES,
    root_terminal_observation: implementation::bray_runtime_root_terminal_observation,
    root_completion_resolution: implementation::bray_runtime_root_completion_resolution,
    cleanup_incident_reporting: implementation::bray_runtime_cleanup_incident_reporting,
    substrate_initialization: implementation::bray_runtime_substrate_initialization,
    substrate_shutdown: implementation::bray_runtime_substrate_shutdown,
    frame_storage_admission: implementation::bray_runtime_frame_storage_admission,
    frame_storage_activation: implementation::bray_runtime_frame_storage_activation,
    frame_storage_release: implementation::bray_runtime_frame_storage_release,
    root_execution: implementation::bray_runtime_root_execution,
    main_thread_lane_startup: implementation::bray_runtime_main_thread_lane_startup,
    main_thread_lane_drive: implementation::bray_runtime_main_thread_lane_drive,
    task_allocation: implementation::bray_runtime_task_allocation,
    task_start: implementation::bray_runtime_task_start,
    task_completion_borrow: implementation::bray_runtime_task_completion_borrow,
    task_completion_borrow_release: implementation::bray_runtime_task_completion_borrow_release,
    task_resolution: implementation::bray_runtime_task_resolution,
    task_destruction: implementation::bray_runtime_task_destruction,
    awaited_frame_composition: implementation::bray_runtime_awaited_frame_composition,
    inactive_capture_destruction: implementation::bray_runtime_inactive_capture_destruction,
    awaited_frame_resolution: implementation::bray_runtime_awaited_frame_resolution,
    task_event_creation: implementation::bray_runtime_task_event_creation,
    task_event_signal: implementation::bray_runtime_task_event_signal,
    task_event_destruction: implementation::bray_runtime_task_event_destruction,
    wake: implementation::bray_runtime_wake,
    join_registration: implementation::bray_runtime_join_registration,
    compatible_lane_selection: implementation::bray_runtime_compatible_lane_selection,
    root_cancellation_request: implementation::bray_runtime_root_cancellation_request,
    task_cancellation_request: implementation::bray_runtime_task_cancellation_request,
    awaited_frame_cancellation_request:
        implementation::bray_runtime_awaited_frame_cancellation_request,
    asynchronous_product_host_control:
        implementation::bray_runtime_asynchronous_product_host_control,
    entry_result_admission: implementation::bray_runtime_entry_result_admission,
    entry_result_resolution: implementation::bray_runtime_entry_result_resolution,
};

/// Borrows the execution component from this resident runtime implementation.
/// Its host image must remain loaded through every dependent product and callback.
pub extern "C" fn bray_runtime_execution_services() -> &'static NativeExecutionServices {
    &EXECUTION_SERVICES
}
