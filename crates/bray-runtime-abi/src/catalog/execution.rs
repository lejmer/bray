/// Expands the complete execution-runtime role catalog for one representation.
#[macro_export]
macro_rules! runtime_role_catalog {
    ($projection:ident) => {
        $projection! {
            RuntimeInitialization {
                "Initialize one loaded runtime artifact instance before product entry.", "runtime_initialization",
                native: (RUNTIME_INITIALIZATION_SYMBOL = "bray_runtime_initialization", [Usize, Usize] -> U32),
                call_hook: (),
                compiler: C [Usize, Usize] -> U32,
                owner: Host, availability: All, bootstrap: ("runtime_initialization"), host_control: false,
                capabilities: [],
                effects: [InitializeRuntime]
            }
            RootExecution {
                "Begin and own the executable root run.", "root_execution",
                native: (ROOT_EXECUTION_SYMBOL = "bray_runtime_root_execution", [Pointer, Pointer, Configuration] -> RootStart),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [EstablishRootRun, TransferFrame]
            }
            SynchronousRootExecution {
                "Execute one synchronous entry callback behind the product panic boundary.", "synchronous_root_execution",
                native: (SYNCHRONOUS_ROOT_EXECUTION_SYMBOL = "bray_runtime_synchronous_root_execution", [Pointer, Usize] -> RunOutcome),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: ("synchronous_root_execution"), host_control: false,
                capabilities: [],
                effects: [EstablishRootRun, ExecuteCallbackRoot]
            }
            ForeignCallbackExecution {
                "Execute one foreign callback behind a thread-entry and panic boundary.", "foreign_callback_execution",
                native: (FOREIGN_CALLBACK_EXECUTION_SYMBOL = "bray_runtime_foreign_callback_execution", [Pointer, Usize] -> RunOutcome),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Callback, availability: All, bootstrap: ("foreign_callback_execution"), host_control: false,
                capabilities: [],
                effects: [EstablishRootRun, ExecuteCallbackRoot]
            }
            NativeThreadExecution {
                "Execute one Bray-owned native-thread root behind its runtime boundary.", "native_thread_execution",
                native: (NATIVE_THREAD_EXECUTION_SYMBOL = "bray_runtime_native_thread_execution", [Pointer, Usize, Pointer, Usize, PointerUsize] -> U32),
                call_hook: (NativeThreadExecution),
                compiler: C [Pointer, Usize, Pointer, Usize, Pointer] -> U32,
                owner: Callback, availability: All, bootstrap: ("native_thread_execution"), host_control: false,
                capabilities: [],
                effects: [PublishTerminalState, ObserveCancellation, EstablishVisibility]
            }
            CurrentNativeThreadIdentity {
                "Read the process-wide identity of the current Bray native thread.", "current_native_thread_identity",
                native: (CURRENT_NATIVE_THREAD_IDENTITY_SYMBOL = "bray_runtime_current_native_thread_identity", [] -> U64),
                call_hook: (CurrentNativeThreadIdentity),
                compiler: Bray [] -> U64,
                owner: Callback, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [ObserveThreadAttachment]
            }
            MainNativeThreadIdentity {
                "Read the process-wide identity of the distinguished initial native thread.", "main_native_thread_identity",
                native: (MAIN_NATIVE_THREAD_IDENTITY_SYMBOL = "bray_runtime_main_native_thread_identity", [] -> U64),
                call_hook: (MainNativeThreadIdentity),
                compiler: Bray [] -> U64,
                owner: Callback, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [ObserveThreadAttachment]
            }
            NativeThreadPanicReportRecovery {
                "Recover one owned panic report published by a native-thread boundary.", "native_thread_panic_report_recovery",
                native: (NATIVE_THREAD_PANIC_REPORT_RECOVERY_SYMBOL = "bray_runtime_native_thread_panic_report_recovery", [Usize] -> Usize),
                call_hook: (NativeThreadPanicReportRecovery),
                compiler: Bray [Usize] -> PanicReport,
                owner: Callback, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [AcquireTerminalState]
            }
            TaskEventCreation {
                "Create one runtime-owned task event.", "task_event_creation",
                native: (TASK_EVENT_CREATION_SYMBOL = "bray_runtime_task_event_creation", [] -> Usize),
                call_hook: (TaskEventCreation),
                compiler: Bray [] -> Usize,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [CreateTaskEvent]
            }
            TaskEventSignal {
                "Signal one runtime-owned task event.", "task_event_signal",
                native: (TASK_EVENT_SIGNAL_SYMBOL = "bray_runtime_task_event_signal", [Usize] -> U32),
                call_hook: (TaskEventSignal),
                compiler: Bray [Usize] -> U32,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [SignalTaskEvent, EstablishVisibility]
            }
            TaskEventDestruction {
                "Release one runtime-owned task event.", "task_event_destruction",
                native: (TASK_EVENT_DESTRUCTION_SYMBOL = "bray_runtime_task_event_destruction", [Usize] -> U32),
                call_hook: (TaskEventDestruction),
                compiler: Bray [Usize] -> U32,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [ReleaseTaskEvent]
            }
            ThreadAttachmentIdentity {
                "Read the identity of the current exact Bray thread attachment.", "thread_attachment_identity",
                native: (THREAD_ATTACHMENT_IDENTITY_SYMBOL = "bray_runtime_thread_attachment_identity", [Pointer] -> U64),
                call_hook: (),
                compiler: C [Pointer] -> U64,
                owner: Host, availability: All, bootstrap: ("thread_attachment_identity"), host_control: false,
                capabilities: [],
                effects: [ObserveThreadAttachment]
            }
            ThreadStaticCleanupRegistration {
                "Register one static cleanup entry with the current exact thread attachment.", "thread_static_cleanup_registration",
                native: (THREAD_STATIC_CLEANUP_REGISTRATION_SYMBOL = "bray_runtime_thread_static_cleanup_registration", [Pointer] -> U32),
                call_hook: (),
                compiler: C [Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("register_thread_cleanup"), host_control: false,
                capabilities: [],
                effects: [RegisterThreadCleanup]
            }
            ProductHostControl {
                "Control lifecycle and provider obligations for one loaded product host.", "product_host_control",
                native: (PRODUCT_HOST_CONTROL_RUNTIME_SYMBOL = "bray_runtime_product_host_control", [Pointer, U32, Pointer] -> ProductObservation),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: ("product_host_control"), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            AsynchronousProductHostControl {
                "Admit execution and control lifecycle for a product with asynchronous cleanup.", "asynchronous_product_host_control",
                native: (ASYNCHRONOUS_PRODUCT_HOST_CONTROL_SYMBOL = "bray_runtime_asynchronous_product_host_control", [Pointer, U32, Pointer] -> ProductObservation),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [ControlProductHost, InitializeRuntime]
            }
            ProviderRetention {
                "Retain an already live provider in an empty caller-owned destination without allocation.", "provider_retention",
                native: (PROVIDER_RETENTION_SYMBOL = "bray_runtime_provider_retention", [Pointer, Pointer] -> U32),
                call_hook: (),
                compiler: C [Pointer, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            RootCancellationRequest {
                "Request cancellation of the root run from its host.", "root_cancellation_request",
                native: (ROOT_CANCELLATION_REQUEST_SYMBOL = "bray_runtime_root_cancellation_request", [U64] -> U32),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Cancellation, availability: All, bootstrap: (), host_control: true,
                capabilities: [],
                effects: [RequestCancellation]
            }
            TaskAllocation {
                "Allocate stable task-owned storage.", "task_allocation",
                native: (TASK_ALLOCATION_SYMBOL = "bray_runtime_task_allocation", [] -> TaskAllocation),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [AllocateTask]
            }
            TaskStart {
                "Publish a task, consuming its inactive frame only on success.", "task_start",
                native: (TASK_START_SYMBOL = "bray_runtime_task_start", [U64, InactiveFrame] -> U32),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [TransferFrame, PublishWork]
            }
            FrameResume {
                "Enter or resume one protected async frame.", "frame_resume",
                native: (),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Compiler, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [ExecuteCallbackRoot]
            }
            SuspensionRegistration {
                "Register a suspended frame with an event source.", "suspension_registration",
                native: (SUSPENSION_REGISTRATION_SYMBOL = "bray_runtime_suspension_registration", [U32] -> FrameProgress),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [RegisterContinuation]
            }
            Wake {
                "Request another dispatch of a run.", "wake",
                native: (WAKE_SYMBOL = "bray_runtime_wake", [U64] -> U32),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [PublishWork, EstablishVisibility]
            }
            TaskCancellationRequest {
                "Request cancellation of a child task.", "task_cancellation_request",
                native: (TASK_CANCELLATION_REQUEST_SYMBOL = "bray_runtime_task_cancellation_request", [U64] -> U32),
                call_hook: (),
                compiler: Bray [U64] -> U32,
                owner: Cancellation, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [RequestCancellation]
            }
            AwaitedFrameCancellationRequest {
                "Request cancellation of the current frame's attached child.", "awaited_frame_cancellation_request",
                native: (AWAITED_FRAME_CANCELLATION_REQUEST_SYMBOL = "bray_runtime_awaited_frame_cancellation_request", [] -> Void),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Cancellation, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [RequestCancellation]
            }
            CurrentRunCancellationObservation {
                "Observe cancellation requested for the current run.", "current_run_cancellation_observation",
                native: (CURRENT_RUN_CANCELLATION_OBSERVATION_SYMBOL = "bray_runtime_current_run_cancellation_observation", [] -> U8),
                call_hook: (),
                compiler: Bray [] -> Bool,
                owner: Cancellation, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [ObserveCancellation]
            }
            CurrentRunCancellationPropagation {
                "Transfer current-run cancellation to the nearest run boundary.", "current_run_cancellation_propagation",
                native: (CURRENT_RUN_CANCELLATION_PROPAGATION_SYMBOL = "bray_runtime_current_run_cancellation_propagation", [] -> Never),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Cancellation, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [PropagateCancellation]
            }
            CleanupShieldEnter {
                "Defer cancellation delivery during current-run cleanup.", "cleanup_shield_enter",
                native: (CLEANUP_SHIELD_ENTER_SYMBOL = "bray_runtime_cleanup_shield_enter", [] -> Void),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Cancellation, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: []
            }
            CleanupShieldLeave {
                "Restore current-run cancellation delivery after cleanup.", "cleanup_shield_leave",
                native: (CLEANUP_SHIELD_LEAVE_SYMBOL = "bray_runtime_cleanup_shield_leave", [] -> Void),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Cancellation, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: []
            }
            JoinRegistration {
                "Register one observer for a task terminal state.", "join_registration",
                native: (JOIN_REGISTRATION_SYMBOL = "bray_runtime_join_registration", [U64, Pointer, Usize] -> RunOutcome),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [RegisterContinuation, AcquireTerminalState, EstablishVisibility]
            }
            TaskResolution {
                "Transfer one task's available terminal result.", "task_resolution",
                native: (TASK_RESOLUTION_SYMBOL = "bray_runtime_task_resolution", [U64, Pointer, Pointer] -> U32),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [AcquireTerminalState, EstablishVisibility]
            }
            TaskCompletionBorrow {
                "Exclusively borrow an available completed task value until release.", "task_completion_borrow",
                native: (TASK_COMPLETION_BORROW_SYMBOL = "bray_runtime_task_completion_borrow", [U64, PointerUsize] -> U32),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [AcquireTerminalState, EstablishVisibility]
            }
            TaskCompletionBorrowRelease {
                "Return an exclusively borrowed completion to its task owner.", "task_completion_borrow_release",
                native: (TASK_COMPLETION_BORROW_RELEASE_SYMBOL = "bray_runtime_task_completion_borrow_release", [U64] -> U32),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [EstablishVisibility]
            }
            TerminalPublication {
                "Publish one terminal run outcome.", "terminal_publication",
                native: (TERMINAL_PUBLICATION_SYMBOL = "bray_runtime_terminal_publication", [U32, Usize] -> FrameProgress),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [PublishTerminalState, EstablishVisibility]
            }
            RuntimeEvent {
                "Integrate one runtime event source.", "runtime_event",
                native: (RUNTIME_EVENT_SYMBOL = "bray_runtime_event", [Pointer, Usize] -> U32),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Event, availability: All, bootstrap: (), host_control: false,
                capabilities: [Reactor],
                effects: [ExecuteCallbackRoot]
            }
            CompatibleLaneSelection {
                "Select a lane compatible with checked execution requirements.", "compatible_lane_selection",
                native: (COMPATIBLE_LANE_SELECTION_SYMBOL = "bray_runtime_compatible_lane_selection", [U64, U32] -> LaneResult),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: []
            }
            CleanupIncidentConstruction {
                "Retain an owned cleanup error in a report with typed payload callbacks.", "cleanup_incident_construction",
                native: (CLEANUP_INCIDENT_CONSTRUCTION_SYMBOL = "bray_runtime_cleanup_incident_construction", [Pointer] -> Usize),
                call_hook: (),
                compiler: Bray [] -> PanicReport,
                owner: Host, availability: All, bootstrap: ("cleanup_incident_construction"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            CleanupIncidentTransfer {
                "Consume one valid cleanup incident, retaining it in the current run or destroying it on rejection.", "cleanup_incident_transfer",
                native: (CLEANUP_INCIDENT_TRANSFER_SYMBOL = "bray_runtime_cleanup_incident_transfer", [Pointer] -> U32),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            CleanupIncidentDetailReporting {
                "Report the type and source of one retained cleanup error.", "cleanup_incident_detail_reporting",
                native: (CLEANUP_INCIDENT_DETAIL_REPORTING_SYMBOL = "bray_runtime_cleanup_incident_detail_reporting", [Pointer] -> U32),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [ReportCleanupIncidents]
            }
            CleanupIncidentReporting {
                "Report and destroy product-host cleanup incidents.", "cleanup_incident_reporting",
                native: (CLEANUP_INCIDENT_REPORTING_SYMBOL = "bray_runtime_cleanup_incident_reporting", [] -> U32),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: true,
                capabilities: [],
                effects: [ReportCleanupIncidents]
            }
            MainThreadLaneStartup {
                "Initialize the distinguished main-thread execution lane.", "main_thread_lane_startup",
                native: (MAIN_THREAD_LANE_STARTUP_SYMBOL = "bray_runtime_main_thread_lane_startup", [Configuration] -> U32),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution, MainThreadLane],
                effects: []
            }
            MainThreadLaneDrive {
                "Drive work assigned to the distinguished main-thread lane.", "main_thread_lane_drive",
                native: (MAIN_THREAD_LANE_DRIVE_SYMBOL = "bray_runtime_main_thread_lane_drive", [] -> U32),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution, MainThreadLane],
                effects: []
            }
            RootTerminalObservation {
                "Observe the root terminal record without creating a source task.", "root_terminal_observation",
                native: (ROOT_TERMINAL_OBSERVATION_SYMBOL = "bray_runtime_root_terminal_observation", [U64] -> RunOutcome),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: true,
                capabilities: [],
                effects: [AcquireTerminalState, EstablishVisibility]
            }
            RootCompletionResolution {
                "Release runtime-owned root completion storage after host resolution.", "root_completion_resolution",
                native: (ROOT_COMPLETION_RESOLUTION_SYMBOL = "bray_runtime_root_completion_resolution", [U64] -> U32),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: true,
                capabilities: [],
                effects: [ReleaseRootCompletion]
            }
            PanicReporting {
                "Report and resolve one root panic payload.", "panic_reporting",
                native: (PANIC_REPORTING_SYMBOL = "bray_runtime_panic_reporting", [Usize] -> U32),
                call_hook: (NativeThreadPanicReporting),
                compiler: Bray [PanicReport] -> U32,
                owner: Host, availability: All, bootstrap: ("panic_reporting"), host_control: true,
                capabilities: [],
                effects: [ReportPanic]
            }
            PanicReportObservation {
                "Report a borrowed panic payload while its owner retains destruction responsibility.", "panic_report_observation",
                native: (PANIC_REPORT_OBSERVATION_SYMBOL = "bray_runtime_panic_report_observation", [Usize] -> U32),
                call_hook: (),
                compiler: Bray [Usize] -> U32,
                owner: Host, availability: All, bootstrap: ("panic_report_observation"), host_control: false,
                capabilities: [],
                effects: [ReportPanic]
            }
            PanicReportDestruction {
                "Destroy one handled panic report without reporting it.", "panic_report_destruction",
                native: (PANIC_REPORT_DESTRUCTION_SYMBOL = "bray_runtime_panic_report_destruction", [Usize] -> U32),
                call_hook: (),
                compiler: Bray [PanicReport] -> U32,
                owner: Host, availability: All, bootstrap: ("panic_report_destruction"), host_control: false,
                capabilities: [],
                effects: [DestroyPanicReport]
            }
            PanicReportSuppression {
                "Attach an owned cleanup incident to the primary panic report.", "panic_report_suppression",
                native: (PANIC_REPORT_SUPPRESSION_SYMBOL = "bray_runtime_panic_report_suppression", [Usize, Usize] -> Usize),
                call_hook: (),
                compiler: Bray [PanicReport, PanicReport] -> PanicReport,
                owner: Host, availability: All, bootstrap: ("panic_report_suppression"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            EntryResultAdmission {
                "Admit entry result storage and asynchronous cleanup before invocation.", "entry_result_admission",
                native: (ENTRY_RESULT_ADMISSION_SYMBOL = "bray_runtime_entry_result_admission", [Pointer, Usize, Usize, Usize, Pointer, Pointer, PointerUsize] -> TaskAllocation),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [AllocateTask]
            }
            EntryResultResolution {
                "Release admitted entry storage or resolve its returned error.", "entry_result_resolution",
                native: (ENTRY_RESULT_RESOLUTION_SYMBOL = "bray_runtime_entry_result_resolution", [U64, U8] -> U32),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [ResolveFrameLifecycle]
            }
            EntryFailureResolution {
                "Report an entrypoint failure and resolve its owned value before host completion.", "entry_failure_resolution",
                native: (ENTRY_FAILURE_RESOLUTION_SYMBOL = "bray_runtime_entry_failure_resolution", [Pointer, Pointer, Usize, Pointer, Pointer] -> U32),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: true,
                capabilities: [],
                effects: [ReportEntryFailure]
            }
            TestEntrySelection {
                "Select the catalog entry admitted by the test runner.", "test_entry_selection",
                native: (TEST_ENTRY_SELECTION_SYMBOL = "bray_runtime_test_entry_selection", [U32] -> U8),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: TestHost, availability: Test, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [SelectTestEntry]
            }
            StructuredShutdown {
                "Shut runtime and product-host infrastructure down in checked order.", "structured_shutdown",
                native: (STRUCTURED_SHUTDOWN_SYMBOL = "bray_runtime_structured_shutdown", [] -> U32),
                call_hook: (),
                compiler: C [] -> U32,
                owner: Host, availability: All, bootstrap: ("structured_shutdown"), host_control: true,
                capabilities: [],
                effects: [StructuredShutdown]
            }
            FrameTaskBroadcast {
                "Broadcast cancellation to tasks reachable from one frame.", "frame_task_broadcast",
                native: (),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Compiler, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [BroadcastFrameTasks]
            }
            FrameLifecycleResolution {
                "Resolve async and ordinary lifecycle state retained by one frame.", "frame_lifecycle_resolution",
                native: (),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Compiler, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [ResolveFrameLifecycle]
            }
            AwaitedFrameResolution {
                "Move the awaited frame's terminal run outcome into its destination.", "awaited_frame_resolution",
                native: (AWAITED_FRAME_RESOLUTION_SYMBOL = "bray_runtime_awaited_frame_resolution", [Pointer, Pointer] -> U32),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [MoveCompletion]
            }
            FrameDestruction {
                "Infallibly destroy terminal frame storage.", "frame_destruction",
                native: (),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Compiler, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [DestroyFrame]
            }
            GeneratorBegin {
                "Initialize one generator accumulation.", "generator_begin",
                native: (),
                call_hook: (),
                compiler: Bray [Pointer, Usize, Usize, Usize, Usize, Bool] -> Void,
                owner: Compiler, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [InitializeGenerator]
            }
            GeneratorPush {
                "Append one value to generator accumulation.", "generator_push",
                native: (),
                call_hook: (),
                compiler: Bray [Pointer, Pointer] -> Void,
                owner: Compiler, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [AppendGeneratorValue]
            }
            GeneratorFinish {
                "Finish generator accumulation and publish its value.", "generator_finish",
                native: (),
                call_hook: (),
                compiler: Bray [Pointer] -> Void,
                owner: Compiler, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [FinishGenerator]
            }
            PanicReportConstruction {
                "Construct one owned panic report.", "panic_report_construction",
                native: (PANIC_REPORT_CONSTRUCTION_SYMBOL = "bray_runtime_panic_report_construction", [U32, U32, U32, U32, U32, U64, Pointer, Usize] -> Usize),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: ("panic_report_construction"), host_control: false,
                capabilities: [],
                effects: [ConstructPanicReport]
            }
            PanicPropagation {
                "Propagate one owned panic report to the nearest native run boundary.", "panic_propagation",
                native: (PANIC_PROPAGATION_SYMBOL = "bray_runtime_panic_propagation", [Usize] -> Never),
                call_hook: (),
                compiler: Bray [PanicReport] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [PropagatePanic]
            }
            FrameCreation {
                "Create one inactive erased protected frame.", "frame_creation",
                native: (),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Compiler, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [CreateFrame]
            }
            FrameStorageAdmission {
                "Reserve generated frame storage and cleanup capacity before capture ownership.", "frame_storage_admission",
                native: (FRAME_STORAGE_ADMISSION_SYMBOL = "bray_runtime_frame_storage_admission", [Pointer] -> Usize),
                call_hook: (),
                compiler: C [Pointer] -> Usize,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [CreateFrame]
            }
            ExecutionServices {
                "Describe the resident execution services supplied by the scheduler.", "execution_services",
                native: (EXECUTION_SERVICES_SYMBOL = "bray_runtime_execution_services", [] -> Pointer),
                call_hook: (),
                compiler: C [] -> Pointer,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            ProductServicesFormation {
                "Create explicitly shared product services before provider formation.", "product_services_formation",
                native: (PRODUCT_SERVICES_FORMATION_SYMBOL = "bray_runtime_product_services_formation", [Pointer, Pointer] -> U32),
                call_hook: (),
                compiler: C [Pointer, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            ProductServicesRelease {
                "Release one product services binding after handing it to providers.", "product_services_release",
                native: (PRODUCT_SERVICES_RELEASE_SYMBOL = "bray_runtime_product_services_release", [Pointer] -> Void),
                call_hook: (),
                compiler: C [Pointer] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            CleanupCapacityAdmission {
                "Secure a complete generated cleanup bundle before owner construction.", "cleanup_capacity_admission",
                native: (CLEANUP_CAPACITY_ADMISSION_SYMBOL = "bray_runtime_cleanup_capacity_admission", [Pointer, Usize, Pointer] -> U32),
                call_hook: (),
                compiler: C [Pointer, Usize, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            CleanupCapacityDischarge {
                "Retire a generated cleanup capacity credit after ownership is resolved.", "cleanup_capacity_discharge",
                native: (CLEANUP_CAPACITY_DISCHARGE_SYMBOL = "bray_runtime_cleanup_capacity_discharge", [Pointer, Pointer] -> U32),
                call_hook: (),
                compiler: C [Pointer, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            FrameStorageActivation {
                "Activate reserved generated cleanup storage without allocating.", "frame_storage_activation",
                native: (FRAME_STORAGE_ACTIVATION_SYMBOL = "bray_runtime_frame_storage_activation", [Pointer] -> Usize),
                call_hook: (),
                compiler: C [Pointer] -> Usize,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [CreateFrame]
            }
            FrameStorageRelease {
                "Release resolved generated frame storage and unused cleanup capacity.", "frame_storage_release",
                native: (FRAME_STORAGE_RELEASE_SYMBOL = "bray_runtime_frame_storage_release", [Usize] -> Void),
                call_hook: (),
                compiler: C [Usize] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [DestroyFrame]
            }
            InactiveFrameMove {
                "Move one inactive erased protected frame before first resume.", "inactive_frame_move",
                native: (),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Compiler, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [MoveFrame]
            }
            AwaitedFrameComposition {
                "Compose one erased directly awaited frame into its parent.", "awaited_frame_composition",
                native: (AWAITED_FRAME_COMPOSITION_SYMBOL = "bray_runtime_awaited_frame_composition", [InactiveFrame, U8] -> Void),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [ComposeAwaitedFrame]
            }
            TaskDestruction {
                "Infallibly destroy one terminal task control record.", "task_destruction",
                native: (TASK_DESTRUCTION_SYMBOL = "bray_runtime_task_destruction", [U64, Pointer] -> U32),
                call_hook: (),
                compiler: Bray [U64] -> U32,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [DestroyTask]
            }
            InactiveCaptureDestruction {
                "Synchronously destroy quiescent inactive captures and return their Bray call outcome.", "inactive_capture_destruction",
                native: (INACTIVE_CAPTURE_DESTRUCTION_SYMBOL = "bray_runtime_inactive_capture_destruction", [InactiveFrame] -> Usize),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [DestroyFrame]
            }
        }
    };
}
