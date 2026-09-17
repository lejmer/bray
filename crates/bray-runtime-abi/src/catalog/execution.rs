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
                native: (ROOT_EXECUTION_SYMBOL = "bray_runtime_root_execution", [Usize, Configuration] -> RootStart),
                resident: (Execution root_execution: extern "C" fn(NativeProtectedFrameTransfer, NativeRuntimeConfiguration) -> NativeRootStart),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [EstablishRootRun, TransferFrame]
            }
            SynchronousRootExecution {
                "Execute one synchronous entry callback behind the product panic boundary.", "synchronous_root_execution",
                native: (SYNCHRONOUS_ROOT_EXECUTION_SYMBOL = "bray_runtime_synchronous_root_execution", [Pointer, Usize] -> RunOutcome),
                resident: (Host synchronous_root_execution: extern "C" fn(NativeSynchronousRootCallback, usize) -> NativeRunOutcome),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [EstablishRootRun, ExecuteCallbackRoot]
            }
            ForeignCallbackExecution {
                "Execute one foreign callback behind a thread-entry and panic boundary.", "foreign_callback_execution",
                native: (FOREIGN_CALLBACK_EXECUTION_SYMBOL = "bray_runtime_foreign_callback_execution", [Pointer, Usize] -> RunOutcome),
                resident: (Host foreign_callback_execution: extern "C" fn(NativeSynchronousRootCallback, usize) -> NativeRunOutcome),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Callback, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [EstablishRootRun, ExecuteCallbackRoot]
            }
            NativeThreadExecution {
                "Execute one Bray-owned native-thread root behind its runtime boundary.", "native_thread_execution",
                native: (NATIVE_THREAD_EXECUTION_SYMBOL = "bray_runtime_native_thread_execution", [Pointer, Usize, Pointer, Usize, PanicReport] -> U32),
                resident: (Host native_thread_execution: extern "C" fn(NativeThreadOperationCallback, usize, NativeThreadCancellationCallback, usize, &mut NativePanicReport) -> u32),
                call_hook: (NativeThreadExecution),
                compiler: C [Pointer, Usize, Pointer, Usize, Pointer] -> U32,
                owner: Callback, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [PublishTerminalState, ObserveCancellation, EstablishVisibility]
            }
            CurrentNativeThreadIdentity {
                "Read the process-wide identity of the current Bray native thread.", "current_native_thread_identity",
                native: (CURRENT_NATIVE_THREAD_IDENTITY_SYMBOL = "bray_runtime_current_native_thread_identity", [] -> U64),
                resident: (Host current_native_thread_identity: extern "C" fn() -> u64),
                call_hook: (CurrentNativeThreadIdentity),
                compiler: Bray [] -> U64,
                owner: Callback, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [ObserveThreadAttachment]
            }
            MainNativeThreadIdentity {
                "Read the process-wide identity of the distinguished initial native thread.", "main_native_thread_identity",
                native: (MAIN_NATIVE_THREAD_IDENTITY_SYMBOL = "bray_runtime_main_native_thread_identity", [] -> U64),
                resident: (Host main_native_thread_identity: extern "C" fn() -> u64),
                call_hook: (MainNativeThreadIdentity),
                compiler: Bray [] -> U64,
                owner: Callback, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [ObserveThreadAttachment]
            }
            TaskEventCreation {
                "Create one runtime-owned task event.", "task_event_creation",
                native: (TASK_EVENT_CREATION_SYMBOL = "bray_runtime_task_event_creation", [] -> Usize),
                resident: (Execution task_event_creation: extern "C" fn() -> usize),
                call_hook: (TaskEventCreation),
                compiler: Bray [] -> Usize,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [CreateTaskEvent]
            }
            TaskEventSignal {
                "Signal one runtime-owned task event.", "task_event_signal",
                native: (TASK_EVENT_SIGNAL_SYMBOL = "bray_runtime_task_event_signal", [Usize] -> U32),
                resident: (Execution task_event_signal: extern "C" fn(usize) -> NativeRuntimeStatus),
                call_hook: (TaskEventSignal),
                compiler: Bray [Usize] -> U32,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [SignalTaskEvent, EstablishVisibility]
            }
            TaskEventDestruction {
                "Release one runtime-owned task event.", "task_event_destruction",
                native: (TASK_EVENT_DESTRUCTION_SYMBOL = "bray_runtime_task_event_destruction", [Usize] -> U32),
                resident: (Execution task_event_destruction: extern "C" fn(usize) -> NativeRuntimeStatus),
                call_hook: (TaskEventDestruction),
                compiler: Bray [Usize] -> U32,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [ReleaseTaskEvent]
            }
            ThreadAttachmentIdentity {
                "Read the identity of the current exact Bray thread attachment.", "thread_attachment_identity",
                native: (THREAD_ATTACHMENT_IDENTITY_SYMBOL = "bray_runtime_thread_attachment_identity", [Pointer] -> U64),
                resident: (Host thread_attachment_identity: extern "C" fn(&'static NativeProductHostDescriptor) -> u64),
                call_hook: (),
                compiler: C [Pointer] -> U64,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [ObserveThreadAttachment]
            }
            ThreadStaticCleanupRegistration {
                "Register one static cleanup entry with the current exact thread attachment.", "thread_static_cleanup_registration",
                native: (THREAD_STATIC_CLEANUP_REGISTRATION_SYMBOL = "bray_runtime_thread_static_cleanup_registration", [Pointer] -> U32),
                resident: (Host thread_static_cleanup_registration: extern "C" fn(&NativeThreadStaticCleanupRegistration) -> NativeRuntimeStatus),
                call_hook: (),
                compiler: C [Pointer] -> U32,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [RegisterThreadCleanup]
            }
            ProductHostControl {
                "Control lifecycle and provider obligations for one loaded product host.", "product_host_control",
                native: (PRODUCT_HOST_CONTROL_RUNTIME_SYMBOL = "bray_runtime_product_host_control", [Pointer, U32] -> ProductObservation),
                resident: (Host product_host_control: extern "C" fn(&NativeProductHostDescriptor, NativeProductHostOperation) -> NativeProductHostObservation),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            RootCancellationRequest {
                "Request cancellation of the root run from its host.", "root_cancellation_request",
                native: (ROOT_CANCELLATION_REQUEST_SYMBOL = "bray_runtime_root_cancellation_request", [U64] -> U32),
                resident: (Execution root_cancellation_request: extern "C" fn(NativeRootHandle) -> NativeRuntimeStatus),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Cancellation, availability: All, bootstrap: (), host_control: true,
                capabilities: [],
                effects: [RequestCancellation]
            }
            TaskAllocation {
                "Allocate stable task-owned storage.", "task_allocation",
                native: (TASK_ALLOCATION_SYMBOL = "bray_runtime_task_allocation", [] -> TaskAllocation),
                resident: (Execution task_allocation: extern "C" fn() -> NativeTaskAllocation),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [AllocateTask]
            }
            TaskStart {
                "Publish a newly initialized task for execution.", "task_start",
                native: (TASK_START_SYMBOL = "bray_runtime_task_start", [U64, InactiveFrame] -> U32),
                resident: (Execution task_start: extern "C" fn(NativeTaskHandle, NativeInactiveFrame) -> NativeRuntimeStatus),
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
                resident: (Execution suspension_registration: extern "C" fn(u32) -> NativeFrameProgress),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [RegisterContinuation]
            }
            Wake {
                "Request another dispatch of a run.", "wake",
                native: (WAKE_SYMBOL = "bray_runtime_wake", [U64] -> U32),
                resident: (Execution wake: extern "C" fn(NativeTaskHandle) -> NativeRuntimeStatus),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [PublishWork, EstablishVisibility]
            }
            TaskCancellationRequest {
                "Request cancellation of a child task.", "task_cancellation_request",
                native: (TASK_CANCELLATION_REQUEST_SYMBOL = "bray_runtime_task_cancellation_request", [U64] -> U32),
                resident: (Execution task_cancellation_request: extern "C" fn(NativeTaskHandle) -> NativeRuntimeStatus),
                call_hook: (),
                compiler: Bray [U64] -> U32,
                owner: Cancellation, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [RequestCancellation]
            }
            CurrentRunCancellationObservation {
                "Observe cancellation requested for the current run.", "current_run_cancellation_observation",
                native: (CURRENT_RUN_CANCELLATION_OBSERVATION_SYMBOL = "bray_runtime_current_run_cancellation_observation", [] -> U8),
                resident: (Host current_run_cancellation_observation: extern "C" fn() -> u8),
                call_hook: (),
                compiler: Bray [] -> Bool,
                owner: Cancellation, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [ObserveCancellation]
            }
            CurrentRunCancellationPropagation {
                "Transfer current-run cancellation to the nearest run boundary.", "current_run_cancellation_propagation",
                native: (CURRENT_RUN_CANCELLATION_PROPAGATION_SYMBOL = "bray_runtime_current_run_cancellation_propagation", [] -> Never),
                resident: (Host current_run_cancellation_propagation: extern "C-unwind" fn() -> !),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Cancellation, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [PropagateCancellation]
            }
            CleanupShieldEnter {
                "Defer cancellation delivery during current-run cleanup.", "cleanup_shield_enter",
                native: (CLEANUP_SHIELD_ENTER_SYMBOL = "bray_runtime_cleanup_shield_enter", [] -> Void),
                resident: (Host cleanup_shield_enter: extern "C" fn()),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Cancellation, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: []
            }
            CleanupShieldLeave {
                "Restore current-run cancellation delivery after cleanup.", "cleanup_shield_leave",
                native: (CLEANUP_SHIELD_LEAVE_SYMBOL = "bray_runtime_cleanup_shield_leave", [] -> Void),
                resident: (Host cleanup_shield_leave: extern "C" fn()),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Cancellation, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: []
            }
            JoinRegistration {
                "Register one observer for a task terminal state.", "join_registration",
                native: (JOIN_REGISTRATION_SYMBOL = "bray_runtime_join_registration", [U64, Pointer, Usize] -> RunOutcome),
                resident: (Execution join_registration: extern "C" fn(NativeTaskHandle, NativeWakeCallback, usize) -> NativeRunOutcome),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [RegisterContinuation, AcquireTerminalState, EstablishVisibility]
            }
            TaskObservationCreation {
                "Create one lazy task-observation frame.", "task_observation_creation",
                native: (TASK_OBSERVATION_CREATION_SYMBOL = "bray_runtime_task_observation_creation", [U64, U8, Pointer, Pointer, Pointer] -> InactiveFrame),
                resident: (Execution task_observation_creation: extern "C-unwind" fn(NativeTaskHandle, u8, *const NativeRunResultLayout, Option<NativeValueCleanupCallback>, Option<NativeValueCleanupCallback>) -> NativeInactiveFrame),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [CreateFrame]
            }
            TaskResolution {
                "Resolve one task to its terminal result.", "task_resolution",
                native: (TASK_RESOLUTION_SYMBOL = "bray_runtime_task_resolution", [U64, Pointer, Pointer] -> U32),
                resident: (Execution task_resolution: extern "C" fn(NativeTaskHandle, *mut u8, *const NativeRunResultLayout) -> NativeRuntimeStatus),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [AcquireTerminalState, EstablishVisibility]
            }
            TerminalPublication {
                "Publish one terminal run outcome.", "terminal_publication",
                native: (),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Compiler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [PublishTerminalState, EstablishVisibility]
            }
            RuntimeEvent {
                "Integrate one runtime event source.", "runtime_event",
                native: (RUNTIME_EVENT_SYMBOL = "bray_runtime_event", [Pointer, Usize] -> U32),
                resident: (Execution runtime_event: extern "C" fn(NativeRuntimeEventCallback, usize) -> NativeRuntimeStatus),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Event, availability: All, bootstrap: (), host_control: false,
                capabilities: [Reactor],
                effects: [ExecuteCallbackRoot]
            }
            CompatibleLaneSelection {
                "Select a lane compatible with checked execution requirements.", "compatible_lane_selection",
                native: (COMPATIBLE_LANE_SELECTION_SYMBOL = "bray_runtime_compatible_lane_selection", [U64, U32] -> LaneResult),
                resident: (Execution compatible_lane_selection: extern "C" fn(NativeTaskHandle, u32) -> NativeExecutionLaneResult),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: []
            }
            CleanupIncidentTransfer {
                "Transfer ownership of one cleanup incident.", "cleanup_incident_transfer",
                native: (),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Compiler, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            CleanupIncidentReporting {
                "Report and destroy product-host cleanup incidents.", "cleanup_incident_reporting",
                native: (CLEANUP_INCIDENT_REPORTING_SYMBOL = "bray_runtime_cleanup_incident_reporting", [] -> U32),
                resident: (Host cleanup_incident_reporting: extern "C" fn() -> NativeRuntimeStatus),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: true,
                capabilities: [],
                effects: [ReportCleanupIncidents]
            }
            MainThreadLaneStartup {
                "Initialize the distinguished main-thread execution lane.", "main_thread_lane_startup",
                native: (MAIN_THREAD_LANE_STARTUP_SYMBOL = "bray_runtime_main_thread_lane_startup", [Configuration] -> U32),
                resident: (Execution main_thread_lane_startup: extern "C" fn(NativeRuntimeConfiguration) -> NativeRuntimeStatus),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution, MainThreadLane],
                effects: []
            }
            MainThreadLaneDrive {
                "Drive work assigned to the distinguished main-thread lane.", "main_thread_lane_drive",
                native: (MAIN_THREAD_LANE_DRIVE_SYMBOL = "bray_runtime_main_thread_lane_drive", [] -> U32),
                resident: (Execution main_thread_lane_drive: extern "C" fn() -> NativeRuntimeStatus),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution, MainThreadLane],
                effects: []
            }
            RootTerminalObservation {
                "Observe the root terminal record without creating a source task.", "root_terminal_observation",
                native: (ROOT_TERMINAL_OBSERVATION_SYMBOL = "bray_runtime_root_terminal_observation", [U64] -> RunOutcome),
                resident: (Execution root_terminal_observation: extern "C" fn(NativeRootHandle) -> NativeRunOutcome),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: true,
                capabilities: [],
                effects: [AcquireTerminalState, EstablishVisibility]
            }
            RootCompletionResolution {
                "Release runtime-owned root completion storage after host resolution.", "root_completion_resolution",
                native: (ROOT_COMPLETION_RESOLUTION_SYMBOL = "bray_runtime_root_completion_resolution", [U64] -> U32),
                resident: (Execution root_completion_resolution: extern "C" fn(NativeRootHandle) -> NativeRuntimeStatus),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: true,
                capabilities: [],
                effects: [ReleaseRootCompletion]
            }
            PanicReporting {
                "Report and resolve one root panic payload.", "panic_reporting",
                native: (PANIC_REPORTING_SYMBOL = "bray_runtime_panic_reporting", [PanicReport] -> U32),
                resident: (Host panic_reporting: extern "C" fn(&mut NativePanicReport) -> NativeRuntimeStatus),
                call_hook: (NativeThreadPanicReporting),
                compiler: Bray [PanicReport] -> U32,
                owner: Host, availability: All, bootstrap: (), host_control: true,
                capabilities: [],
                effects: [ReportPanic]
            }
            PanicReportDestruction {
                "Destroy one handled panic report without reporting it.", "panic_report_destruction",
                native: (PANIC_REPORT_DESTRUCTION_SYMBOL = "bray_runtime_panic_report_destruction", [PanicReport] -> U32),
                resident: (Host panic_report_destruction: extern "C" fn(&mut NativePanicReport) -> NativeRuntimeStatus),
                call_hook: (),
                compiler: Bray [PanicReport] -> U32,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [DestroyPanicReport]
            }
            OutgoingAdmission {
                "Reserve local outgoing records before accepting an owner.", "outgoing_admission",
                native: (OUTGOING_ADMISSION_SYMBOL = "bray_runtime_outgoing_admission", [Usize, Pointer] -> Void),
                resident: (Host outgoing_admission: extern "C" fn(usize, &mut NativeRunOutcome)),
                call_hook: (),
                compiler: C [Usize, Pointer] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [ConstructPanicReport]
            }
            OutgoingDischarge {
                "Discharge a consumed owner without reclaiming transferred records.", "outgoing_discharge",
                native: (OUTGOING_DISCHARGE_SYMBOL = "bray_runtime_outgoing_discharge", [Usize] -> Void),
                resident: (Host outgoing_discharge: extern "C" fn(usize)),
                call_hook: (),
                compiler: C [Usize] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [DestroyPanicReport]
            }
            OutgoingActivation {
                "Activate an admitted outgoing record before entering a local cleanup action.", "outgoing_activation",
                native: (OUTGOING_ACTIVATION_SYMBOL = "bray_runtime_outgoing_activation", [] -> Usize),
                resident: (Host outgoing_activation: extern "C" fn() -> usize),
                call_hook: (),
                compiler: C [] -> Usize,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            OutgoingRetirement {
                "Retire an action record or transfer it with its outgoing report.", "outgoing_retirement",
                native: (OUTGOING_RETIREMENT_SYMBOL = "bray_runtime_outgoing_retirement", [Usize, Pointer] -> Void),
                resident: (Host outgoing_retirement: extern "C" fn(usize, &mut NativeRunOutcome)),
                call_hook: (),
                compiler: C [Usize, Pointer] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            PanicReportSuppression {
                "Attach an owned cleanup incident to the primary panic report.", "panic_report_suppression",
                native: (PANIC_REPORT_SUPPRESSION_SYMBOL = "bray_runtime_panic_report_suppression", [PanicReport, PanicReport] -> PanicReport),
                resident: (Host panic_report_suppression: extern "C" fn(&mut NativePanicReport, &mut NativePanicReport) -> NativePanicReport),
                call_hook: (),
                compiler: Bray [PanicReport, PanicReport] -> PanicReport,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            EntryFailureReporting {
                "Report one recoverable entrypoint failure value before host resolution.", "entry_failure_reporting",
                native: (ENTRY_FAILURE_REPORTING_SYMBOL = "bray_runtime_entry_failure_reporting", [Usize, Usize] -> U32),
                resident: (Host entry_failure_reporting: extern "C" fn(usize, usize) -> NativeRuntimeStatus),
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
            FrameCompletionMove {
                "Move a frame's completed result into its destination.", "frame_completion_move",
                native: (FRAME_COMPLETION_MOVE_SYMBOL = "bray_runtime_frame_completion_move", [] -> Usize),
                resident: (Execution frame_completion_move: extern "C-unwind" fn() -> usize),
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
            GeneratorCleanupBroadcast {
                "Broadcast task cleanup through initialized generator elements.", "generator_cleanup_broadcast",
                native: (),
                call_hook: (),
                compiler: Bray [Pointer, Pointer] -> Void,
                owner: Compiler, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [BroadcastGeneratorCleanup]
            }
            GeneratorDestruction {
                "Destroy initialized generator elements and release accumulation storage.", "generator_destruction",
                native: (),
                call_hook: (),
                compiler: Bray [Pointer, Pointer, Pointer] -> Void,
                owner: Compiler, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [DestroyGenerator]
            }
            PanicReportConstruction {
                "Construct one owned panic report.", "panic_report_construction",
                native: (PANIC_REPORT_CONSTRUCTION_SYMBOL = "bray_runtime_panic_report_construction", [U32, U32, U32, U32, U32, U64, Pointer, Usize] -> PanicReport),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: ("panic_report_construction"), host_control: false,
                capabilities: [],
                effects: [ConstructPanicReport]
            }
            PanicPropagation {
                "Propagate one owned panic report to the nearest native run boundary.", "panic_propagation",
                native: (PANIC_PROPAGATION_SYMBOL = "bray_runtime_panic_propagation", [PanicReport] -> Never),
                resident: (Host panic_propagation: extern "C" fn(&mut NativePanicReport) -> !),
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
                native: (AWAITED_FRAME_COMPOSITION_SYMBOL = "bray_runtime_awaited_frame_composition", [InactiveFrame] -> Void),
                resident: (Execution awaited_frame_composition: extern "C-unwind" fn(NativeInactiveFrame)),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [ComposeAwaitedFrame]
            }
            TaskDestruction {
                "Infallibly destroy one terminal task control record.", "task_destruction",
                native: (TASK_DESTRUCTION_SYMBOL = "bray_runtime_task_destruction", [U64] -> U32),
                resident: (Execution task_destruction: extern "C" fn(NativeTaskHandle) -> NativeRuntimeStatus),
                call_hook: (),
                compiler: Bray [U64] -> U32,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [DestroyTask]
            }
        }
    };
}
