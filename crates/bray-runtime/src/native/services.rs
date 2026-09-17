use bray_runtime_abi::{NativeExecutionServices, NativeHostServices};

use super::implementation;

pub(crate) static HOST_SERVICES: NativeHostServices = NativeHostServices {
    synchronous_root_execution: implementation::bray_runtime_synchronous_root_execution,
    foreign_callback_execution: implementation::bray_runtime_foreign_callback_execution,
    native_thread_execution: implementation::bray_runtime_native_thread_execution,
    current_native_thread_identity: implementation::bray_runtime_current_native_thread_identity,
    main_native_thread_identity: implementation::bray_runtime_main_native_thread_identity,
    product_host_control: implementation::bray_runtime_product_host_control,
    thread_attachment_identity: implementation::resident_thread_attachment_identity,
    thread_static_cleanup_registration: implementation::resident_thread_static_cleanup_registration,
    cleanup_shield_enter: implementation::bray_runtime_cleanup_shield_enter,
    cleanup_shield_leave: implementation::bray_runtime_cleanup_shield_leave,
    current_run_cancellation_observation:
        implementation::bray_runtime_current_run_cancellation_observation,
    current_run_cancellation_propagation:
        implementation::bray_runtime_current_run_cancellation_propagation,
    cleanup_incident_reporting: implementation::bray_runtime_cleanup_incident_reporting,
    entry_failure_reporting: implementation::bray_runtime_entry_failure_reporting,
    panic_propagation: implementation::bray_runtime_panic_propagation,
    panic_reporting: implementation::bray_runtime_panic_reporting,
    panic_report_destruction: implementation::bray_runtime_panic_report_destruction,
    outgoing_admission: implementation::bray_runtime_outgoing_admission,
    outgoing_discharge: implementation::bray_runtime_outgoing_discharge,
    outgoing_activation: implementation::bray_runtime_outgoing_activation,
    outgoing_retirement: implementation::bray_runtime_outgoing_retirement,
    panic_report_suppression: implementation::bray_runtime_panic_report_suppression,
};

pub(crate) static EXECUTION_SERVICES: NativeExecutionServices = NativeExecutionServices {
    host: &HOST_SERVICES,
    root_execution: implementation::bray_runtime_root_execution,
    main_thread_lane_startup: implementation::bray_runtime_main_thread_lane_startup,
    main_thread_lane_drive: implementation::bray_runtime_main_thread_lane_drive,
    root_terminal_observation: implementation::bray_runtime_root_terminal_observation,
    root_completion_resolution: implementation::bray_runtime_root_completion_resolution,
    root_cancellation_request: implementation::bray_runtime_root_cancellation_request,
    task_allocation: implementation::bray_runtime_task_allocation,
    task_start: implementation::bray_runtime_task_start,
    task_observation_creation: implementation::bray_runtime_task_observation_creation,
    task_resolution: implementation::bray_runtime_task_resolution,
    task_destruction: implementation::bray_runtime_task_destruction,
    task_cancellation_request: implementation::bray_runtime_task_cancellation_request,
    awaited_frame_composition: implementation::bray_runtime_awaited_frame_composition,
    frame_completion_move: implementation::bray_runtime_frame_completion_move,
    task_event_creation: implementation::bray_runtime_task_event_creation,
    task_event_signal: implementation::bray_runtime_task_event_signal,
    task_event_destruction: implementation::bray_runtime_task_event_destruction,
    runtime_event: implementation::bray_runtime_event,
    suspension_registration: implementation::bray_runtime_suspension_registration,
    wake: implementation::bray_runtime_wake,
    join_registration: implementation::bray_runtime_join_registration,
    compatible_lane_selection: implementation::bray_runtime_compatible_lane_selection,
};

pub(crate) const fn host() -> &'static NativeHostServices {
    &HOST_SERVICES
}

pub(crate) const fn execution() -> &'static NativeExecutionServices {
    &EXECUTION_SERVICES
}
