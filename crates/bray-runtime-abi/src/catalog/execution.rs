/// Expands the complete execution-runtime role catalog for one representation.
#[macro_export]
macro_rules! runtime_role_catalog {
    ($projection:ident) => {
        $projection! {
            AtomicWordWait {
                "Wait on one native atomic word. Return may be spurious.", "atomic_word_wait",
                native: (ATOMIC_WORD_WAIT_SYMBOL = "bray_runtime_atomic_word_wait", [Pointer, U32] -> Void),
                call_hook: (),
                compiler: C [Pointer, U32] -> Void,
                owner: Bootstrap, availability: All, bootstrap: ("atomic.wait"), host_control: false,
                capabilities: [],
                effects: []
            }
            AtomicWordNotifyOne {
                "Notify waiters on one native atomic word without changing its contents.", "atomic_word_notify_one",
                native: (ATOMIC_WORD_NOTIFY_ONE_SYMBOL = "bray_runtime_atomic_word_notify_one", [Pointer] -> Void),
                call_hook: (),
                compiler: C [Pointer] -> Void,
                owner: Bootstrap, availability: All, bootstrap: ("atomic.notify_one"), host_control: false,
                capabilities: [],
                effects: []
            }
            AtomicWordNotifyAll {
                "Notify waiters on one native atomic word without changing its contents.", "atomic_word_notify_all",
                native: (ATOMIC_WORD_NOTIFY_ALL_SYMBOL = "bray_runtime_atomic_word_notify_all", [Pointer] -> Void),
                call_hook: (),
                compiler: C [Pointer] -> Void,
                owner: Bootstrap, availability: All, bootstrap: ("atomic.notify_all"), host_control: false,
                capabilities: [],
                effects: []
            }
            RealPower {
                "Compute the real power in the selected IEEE format.", "real_power",
                native: (REAL_POWER_SYMBOL = "bray_runtime_real_power", [U32, Pointer, Pointer, Pointer] -> Void),
                call_hook: (),
                compiler: C [U32, Pointer, Pointer, Pointer] -> Void,
                owner: Bootstrap, availability: All, bootstrap: ("numeric.real_power"), host_control: false,
                capabilities: [],
                effects: []
            }
            ComplexMultiply {
                "Multiply complex IEEE components in the selected format.", "complex_multiply",
                native: (COMPLEX_MULTIPLY_SYMBOL = "bray_runtime_complex_multiply", [U32, Pointer, Pointer, Pointer] -> Void),
                call_hook: (),
                compiler: C [U32, Pointer, Pointer, Pointer] -> Void,
                owner: Bootstrap, availability: All, bootstrap: ("numeric.complex_multiply"), host_control: false,
                capabilities: [],
                effects: []
            }
            ComplexDivide {
                "Divide complex IEEE components in the selected format.", "complex_divide",
                native: (COMPLEX_DIVIDE_SYMBOL = "bray_runtime_complex_divide", [U32, Pointer, Pointer, Pointer] -> Void),
                call_hook: (),
                compiler: C [U32, Pointer, Pointer, Pointer] -> Void,
                owner: Bootstrap, availability: All, bootstrap: ("numeric.complex_divide"), host_control: false,
                capabilities: [],
                effects: []
            }
            ComplexPower {
                "Compute the principal complex power in the selected format.", "complex_power",
                native: (COMPLEX_POWER_SYMBOL = "bray_runtime_complex_power", [U32, Pointer, Pointer, Pointer] -> Void),
                call_hook: (),
                compiler: C [U32, Pointer, Pointer, Pointer] -> Void,
                owner: Bootstrap, availability: All, bootstrap: ("numeric.complex_power"), host_control: false,
                capabilities: [],
                effects: []
            }
            ReportRecordAdmission {
                "Reserve records before a Rust producer accepts ownership.", "report_records_admit",
                native: (REPORT_RECORDS_ADMIT_SYMBOL = "bray_runtime_report_records_admit", [Usize, Pointer] -> U32),
                call_hook: (),
                compiler: C [Usize, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("report_records_admit"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            ReportRecordTake {
                "Detach an admitted prefix without allocating.", "report_records_take",
                native: (REPORT_RECORDS_TAKE_SYMBOL = "bray_runtime_report_records_take", [Pointer, Usize, Pointer] -> Void),
                call_hook: (),
                compiler: C [Pointer, Usize, Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("report_records_take"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            ReportRecordExchange {
                "Exchange one owned record primary without releasing its payload.", "report_record_exchange",
                native: (REPORT_RECORD_EXCHANGE_SYMBOL = "bray_runtime_report_record_exchange", [Usize, Pointer] -> Void),
                call_hook: (),
                compiler: C [Usize, Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("report_record_exchange"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            ReportRecordAppend {
                "Splice exclusively owned chains in constant time.", "report_records_append",
                native: (REPORT_RECORDS_APPEND_SYMBOL = "bray_runtime_report_records_append", [Pointer, Pointer] -> Void),
                call_hook: (),
                compiler: C [Pointer, Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("report_records_append"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            ReportSegmentMark {
                "Attach cleanup context to an exclusively owned report segment.", "report_segment_mark",
                native: (REPORT_SEGMENT_MARK_SYMBOL = "bray_runtime_report_segment_mark", [Pointer, Pointer] -> Void),
                call_hook: (),
                compiler: C [Pointer, Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("report_segment_mark"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            ReportSegmentTake {
                "Detach an ordered cleanup segment and its context.", "report_segment_take",
                native: (REPORT_SEGMENT_TAKE_SYMBOL = "bray_runtime_report_segment_take", [Pointer, Pointer, Pointer] -> Void),
                call_hook: (),
                compiler: C [Pointer, Pointer, Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("report_segment_take"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            ReportSinkTransfer {
                "Update an exclusively borrowed cleanup sink.", "report_sink_transfer",
                native: (REPORT_SINK_TRANSFER_SYMBOL = "bray_runtime_report_sink_transfer", [Pointer, Pointer, Pointer] -> Void),
                call_hook: (),
                compiler: C [Pointer, Pointer, Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("report.sink.transfer"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            ReportSinkBegin {
                "Update an exclusively borrowed cleanup sink.", "report_sink_begin",
                native: (REPORT_SINK_BEGIN_SYMBOL = "bray_runtime_report_sink_begin", [Pointer] -> U32),
                call_hook: (),
                compiler: C [Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("report.sink.begin"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            ReportSinkTake {
                "Update an exclusively borrowed cleanup sink.", "report_sink_take",
                native: (REPORT_SINK_TAKE_SYMBOL = "bray_runtime_report_sink_take", [Pointer, Pointer] -> U32),
                call_hook: (),
                compiler: C [Pointer, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("report.sink.take"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            ReportSinkFinish {
                "Update an exclusively borrowed cleanup sink.", "report_sink_finish",
                native: (REPORT_SINK_FINISH_SYMBOL = "bray_runtime_report_sink_finish", [Pointer] -> Void),
                call_hook: (),
                compiler: C [Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("report.sink.finish"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            ReportSegmentConsumer {
                "Report and consume an owned cleanup segment with its origin.", "report_segment_consumer",
                native: (REPORT_SEGMENT_CONSUMER_SYMBOL = "bray_runtime_report_segment_consumer", [Pointer, Pointer] -> U32),
                call_hook: (),
                compiler: C [Pointer, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("consume_segment"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            CleanupIncidentConsumer {
                "Consume an exclusively transferred typed cleanup incident.", "cleanup_incident_consumer",
                native: (CLEANUP_INCIDENT_CONSUMER_SYMBOL = "bray_runtime_cleanup_incident_consumer", [Pointer, U32] -> U32),
                call_hook: (),
                compiler: C [Pointer, U32] -> U32,
                owner: Host, availability: All, bootstrap: ("report.incident.dispose"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            ReportConsumer {
                "Consume an exclusively transferred report without fresh entry admission.", "report_consumer",
                native: (REPORT_CONSUMER_SYMBOL = "bray_runtime_report_consumer", [Pointer, U32] -> U32),
                call_hook: (),
                compiler: C [Pointer, U32] -> U32,
                owner: Host, availability: All, bootstrap: ("consume_report"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            ReportMessageCopy {
                "Copy retained report bytes under the native report ownership contract.", "report_message_copy",
                native: (REPORT_MESSAGE_COPY_SYMBOL = "bray_runtime_report_message_copy", [Usize, Usize, Pointer, Usize] -> U32),
                call_hook: (),
                compiler: C [Usize, Usize, Pointer, Usize] -> U32,
                owner: Host, availability: All, bootstrap: ("panic_message_copy"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            ReportMessageRelease {
                "Release retained report backing without fresh entry admission.", "report_message_release",
                native: (REPORT_MESSAGE_RELEASE_SYMBOL = "bray_runtime_report_message_release", [Usize, Usize, Pointer] -> Void),
                call_hook: (),
                compiler: C [Usize, Usize, Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("panic_message_release"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            RuntimeInitialization {
                "Initialize one loaded runtime artifact instance before product entry.", "runtime_initialization",
                native: (RUNTIME_INITIALIZATION_SYMBOL = "bray_runtime_initialization", [Usize, Usize] -> U32),
                call_hook: (),
                compiler: C [Usize, Usize] -> U32,
                owner: Host, availability: All, bootstrap: ("runtime_initialization"), host_control: false,
                capabilities: [],
                effects: [InitializeRuntime]
            }
            MemoryObservationBegin {
                "Start one generated memory-observation session.", "memory_observation_begin",
                native: (MEMORY_OBSERVATION_BEGIN_SYMBOL = "bray_runtime_memory_observation_begin", [] -> Void),
                call_hook: (),
                compiler: C [] -> Void,
                owner: Observation, availability: All, bootstrap: ("memory_observation_begin"), host_control: false,
                capabilities: [PerformanceObservation],
                effects: []
            }
            MemoryAllocationObservation {
                "Observe one successful generated memory allocation.", "memory_allocation_observation",
                native: (MEMORY_ALLOCATION_OBSERVATION_SYMBOL = "bray_runtime_memory_allocation_observation", [Usize] -> Void),
                call_hook: (),
                compiler: C [Usize] -> Void,
                owner: Observation, availability: All, bootstrap: ("memory_allocation_observation"), host_control: false,
                capabilities: [PerformanceObservation],
                effects: []
            }
            MemoryCopyObservation {
                "Observe one completed generated memory transfer.", "memory_copy_observation",
                native: (MEMORY_COPY_OBSERVATION_SYMBOL = "bray_runtime_memory_copy_observation", [Usize] -> Void),
                call_hook: (),
                compiler: C [Usize] -> Void,
                owner: Observation, availability: All, bootstrap: ("memory_copy_observation"), host_control: false,
                capabilities: [PerformanceObservation],
                effects: []
            }
            PerformanceIntervalBegin {
                "Start the measured Bray-controlled execution interval.", "performance_interval_begin",
                native: (PERFORMANCE_INTERVAL_BEGIN_SYMBOL = "bray_runtime_performance_interval_begin", [] -> Void),
                call_hook: (),
                compiler: C [] -> Void,
                owner: Observation, availability: All, bootstrap: ("performance_interval_begin"), host_control: false,
                capabilities: [PerformanceObservation],
                effects: []
            }
            PerformanceIntervalEnd {
                "End and record the Bray-controlled execution interval.", "performance_interval_end",
                native: (PERFORMANCE_INTERVAL_END_SYMBOL = "bray_runtime_performance_interval_end", [] -> Void),
                call_hook: (),
                compiler: C [] -> Void,
                owner: Observation, availability: All, bootstrap: ("performance_interval_end"), host_control: false,
                capabilities: [PerformanceObservation],
                effects: []
            }
            RootExecution {
                "Begin and own the executable root run.", "root_execution",
                native: (ROOT_EXECUTION_SYMBOL = "bray_runtime_root_execution", [Usize, Configuration] -> RootStart),
                service: Execution,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [EstablishRootRun, TransferFrame]
            }
            EntryContextRegistration {
                "Exchange a scoped Rust context bridge under an owned thread attachment.", "entry_context_registration",
                native: (ENTRY_CONTEXT_REGISTRATION_SYMBOL = "bray_runtime_entry_context_registration", [Pointer, Pointer] -> U32),
                call_hook: (),
                compiler: C [Pointer, Pointer] -> U32,
                owner: Bootstrap, availability: All, bootstrap: ("entry.bridge.install"), host_control: false,
                capabilities: [],
                effects: []
            }
            EntryContinuation {
                "Continue an admitted native entry inside its scoped context bridge.", "entry_continuation",
                native: (ENTRY_CONTINUATION_SYMBOL = "bray_runtime_entry_continuation", [Pointer, Usize, U32, Pointer] -> U32),
                call_hook: (),
                compiler: C [Pointer, Usize, U32, Pointer] -> U32,
                owner: Bootstrap, availability: All, bootstrap: ("entry.invoke"), host_control: false,
                capabilities: [],
                effects: []
            }
            SynchronousRootExecution {
                "Execute one synchronous entry callback behind the product panic boundary.", "synchronous_root_execution",
                native: (SYNCHRONOUS_ROOT_EXECUTION_SYMBOL = "bray_runtime_synchronous_root_execution", [Pointer, Usize, Pointer] -> U32),
                service: Host,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Bootstrap, availability: All, bootstrap: ("entry.root"), host_control: false,
                capabilities: [],
                effects: [EstablishRootRun, ExecuteCallbackRoot]
            }
            ForeignCallbackExecution {
                "Execute one foreign callback behind a thread-entry and panic boundary.", "foreign_callback_execution",
                native: (FOREIGN_CALLBACK_EXECUTION_SYMBOL = "bray_runtime_foreign_callback_execution", [Pointer, Usize, Pointer] -> U32),
                service: Host,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Bootstrap, availability: All, bootstrap: ("entry.foreign"), host_control: false,
                capabilities: [],
                effects: [EstablishRootRun, ExecuteCallbackRoot]
            }
            NativeThreadExecution {
                "Execute one Bray-owned native-thread root behind its runtime boundary.", "native_thread_execution",
                native: (NATIVE_THREAD_EXECUTION_SYMBOL = "bray_runtime_native_thread_execution", [Pointer, Usize, Pointer, Usize, PanicReportPointer] -> U32),
                service: Host,
                call_hook: (NativeThreadExecution),
                compiler: C [Pointer, Usize, Pointer, Usize, Pointer] -> U32,
                owner: Callback, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [PublishTerminalState, ObserveCancellation, EstablishVisibility]
            }
            ThreadAttachmentEnter {
                "Acquire primitive exact-thread attachment ownership.", "thread_attachment_enter",
                native: (THREAD_ATTACHMENT_ENTER_SYMBOL = "bray_runtime_thread_attachment_enter", [] -> U32),
                call_hook: (),
                compiler: C [] -> U32,
                owner: Host, availability: All, bootstrap: ("host.thread_attachment_enter"), host_control: false,
                capabilities: [],
                effects: [ObserveThreadAttachment]
            }
            ThreadAttachmentLeave {
                "Release primitive exact-thread attachment ownership.", "thread_attachment_leave",
                native: (THREAD_ATTACHMENT_LEAVE_SYMBOL = "bray_runtime_thread_attachment_leave", [] -> U32),
                call_hook: (),
                compiler: C [] -> U32,
                owner: Host, availability: All, bootstrap: ("host.thread_attachment_leave"), host_control: false,
                capabilities: [],
                effects: [ObserveThreadAttachment]
            }
            ThreadAttachmentMarkMain {
                "Select the current initialized attachment as the main thread.", "thread_attachment_mark_main",
                native: (THREAD_ATTACHMENT_MARK_MAIN_SYMBOL = "bray_runtime_thread_attachment_mark_main", [] -> U32),
                call_hook: (),
                compiler: C [] -> U32,
                owner: Host, availability: All, bootstrap: ("host.thread_attachment_mark_main"), host_control: false,
                capabilities: [],
                effects: [ObserveThreadAttachment]
            }
            ThreadAttachmentCleanup {
                "Bind existing exact-thread local cleanup to its primitive owner.", "thread_attachment_cleanup",
                native: (THREAD_ATTACHMENT_CLEANUP_SYMBOL = "bray_runtime_thread_attachment_cleanup", [Pointer] -> U32),
                call_hook: (),
                compiler: C [Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("host.thread_attachment_cleanup"), host_control: false,
                capabilities: [],
                effects: [ObserveThreadAttachment]
            }
            ThreadStorageExit {
                "Resolve the cleared target TLS slot on its exact native thread.", "thread_storage_exit",
                native: (THREAD_STORAGE_EXIT_SYMBOL = "bray_runtime_thread_storage_exit", [Pointer] -> Void),
                call_hook: (),
                compiler: C [Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("thread.destruction.thread_storage_exit"), host_control: false,
                capabilities: [],
                effects: [ObserveThreadAttachment]
            }
            CurrentNativeThreadIdentity {
                "Read the process-wide identity of the current Bray native thread.", "current_native_thread_identity",
                native: (CURRENT_NATIVE_THREAD_IDENTITY_SYMBOL = "bray_runtime_current_native_thread_identity", [] -> U64),
                service: Host,
                call_hook: (CurrentNativeThreadIdentity),
                compiler: Bray [] -> U64,
                owner: Callback, availability: All, bootstrap: ("host.current_native_thread_identity"), host_control: false,
                capabilities: [],
                effects: [ObserveThreadAttachment]
            }
            MainNativeThreadIdentity {
                "Read the process-wide identity of the distinguished initial native thread.", "main_native_thread_identity",
                native: (MAIN_NATIVE_THREAD_IDENTITY_SYMBOL = "bray_runtime_main_native_thread_identity", [] -> U64),
                service: Host,
                call_hook: (MainNativeThreadIdentity),
                compiler: Bray [] -> U64,
                owner: Callback, availability: All, bootstrap: ("host.main_native_thread_identity"), host_control: false,
                capabilities: [],
                effects: [ObserveThreadAttachment]
            }
            TaskEventCreation {
                "Create one runtime-owned task event.", "task_event_creation",
                native: (TASK_EVENT_CREATION_SYMBOL = "bray_runtime_task_event_creation", [] -> Usize),
                service: Execution,
                call_hook: (TaskEventCreation),
                compiler: Bray [] -> Usize,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [CreateTaskEvent]
            }
            TaskEventSignal {
                "Signal one runtime-owned task event.", "task_event_signal",
                native: (TASK_EVENT_SIGNAL_SYMBOL = "bray_runtime_task_event_signal", [Usize] -> U32),
                service: Execution,
                call_hook: (TaskEventSignal),
                compiler: Bray [Usize] -> U32,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [SignalTaskEvent, EstablishVisibility]
            }
            TaskEventDestruction {
                "Release one runtime-owned task event.", "task_event_destruction",
                native: (TASK_EVENT_DESTRUCTION_SYMBOL = "bray_runtime_task_event_destruction", [Usize] -> U32),
                service: Execution,
                call_hook: (TaskEventDestruction),
                compiler: Bray [Usize] -> U32,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [ReleaseTaskEvent]
            }
            ThreadAttachmentIdentity {
                "Read the identity of the current exact Bray thread attachment.", "thread_attachment_identity",
                native: (THREAD_ATTACHMENT_IDENTITY_SYMBOL = "bray_runtime_thread_attachment_identity", [Pointer] -> U64),
                service: Host,
                call_hook: (),
                compiler: C [Pointer] -> U64,
                owner: Host, availability: All, bootstrap: ("host.product.thread_attachment_identity"), host_control: false,
                capabilities: [],
                effects: [ObserveThreadAttachment]
            }
            ThreadStaticCleanupRegistration {
                "Register one static cleanup entry with the current exact thread attachment.", "thread_static_cleanup_registration",
                native: (THREAD_STATIC_CLEANUP_REGISTRATION_SYMBOL = "bray_runtime_thread_static_cleanup_registration", [Pointer] -> U32),
                service: Host,
                call_hook: (),
                compiler: C [Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("host.product.thread_static_cleanup_registration"), host_control: false,
                capabilities: [],
                effects: [RegisterThreadCleanup]
            }
            ProductHostControl {
                "Control lifecycle and provider obligations for one loaded product host.", "product_host_control",
                native: (PRODUCT_HOST_CONTROL_RUNTIME_SYMBOL = "bray_runtime_product_host_control", [Usize, U32, Pointer] -> U32),
                service: Host,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: ("host.product.control"), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            ProviderDomainAccess {
                "Export the resident host storage and provider operations.", "provider_domain_access",
                native: (PROVIDER_DOMAIN_ACCESS_SYMBOL = "bray_runtime_provider_domain_access", [Pointer] -> U32),
                service: Host,
                call_hook: (),
                compiler: C [Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("domain.domain_access"), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            ProductProviderFormation {
                "Admit and form one independently loaded provider.", "product_provider_formation",
                native: (PRODUCT_PROVIDER_FORMATION_SYMBOL = "bray_runtime_product_provider_formation", [Usize, Pointer, Pointer] -> U32),
                service: Host,
                call_hook: (),
                compiler: C [Usize, Pointer, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("host.product.form_provider"), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            ProductProviderControl {
                "Control one epoch-checked external provider activation.", "product_provider_control",
                native: (PRODUCT_PROVIDER_CONTROL_SYMBOL = "bray_runtime_product_provider_control", [Usize, U64, U32, Pointer] -> U32),
                service: Host,
                call_hook: (),
                compiler: C [Usize, U64, U32, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("host.product.control_provider"), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            ProductProviderRetirement {
                "Transfer a retired provider's final residency to its resident caller.", "product_provider_retirement",
                native: (PRODUCT_PROVIDER_RETIREMENT_SYMBOL = "bray_runtime_product_provider_retirement", [Usize, U64, Pointer] -> U32),
                service: Host,
                call_hook: (),
                compiler: C [Usize, U64, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("host.product.retire_provider"), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            ProductProviderNotification {
                "Resume admitted cleanup after native provider ownership changes.", "product_provider_notification",
                native: (PRODUCT_PROVIDER_NOTIFICATION_SYMBOL = "bray_runtime_product_provider_notification", [Usize, Usize, Pointer] -> U32),
                service: Host,
                call_hook: (),
                compiler: C [Usize, Usize, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("host.product.notify_provider"), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            ProductProviderRoot {
                "Acquire a root from an admitted image producer's activation slot.", "product_provider_root",
                native: (PRODUCT_PROVIDER_ROOT_SYMBOL = "bray_runtime_product_provider_root", [Usize, Pointer] -> U32),
                service: Host,
                call_hook: (),
                compiler: C [Usize, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("host.product.acquire_provider_root"), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            ProviderReferenceAcquire {
                "Transfer the final consumer's provider root into a native value.", "provider_reference_acquire",
                native: (PROVIDER_REFERENCE_ACQUIRE_SYMBOL = "bray_runtime_provider_reference_acquire", [Usize, Pointer] -> U32),
                service: Host,
                call_hook: (),
                compiler: C [Usize, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("provider.acquire"), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            ProviderReferenceRelease {
                "Consume an already admitted native provider reference.", "provider_reference_release",
                native: (PROVIDER_REFERENCE_RELEASE_SYMBOL = "bray_runtime_provider_reference_release", [Pointer] -> Void),
                service: Host,
                call_hook: (),
                compiler: C [Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("provider.release"), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            ProviderRootRetain {
                "Duplicate an existing whole-product root without fresh entry admission.", "provider_root_retain",
                native: (PROVIDER_ROOT_RETAIN_SYMBOL = "bray_runtime_provider_root_retain", [Usize] -> Void),
                service: Host,
                call_hook: (),
                compiler: C [Usize] -> Void,
                owner: Host, availability: All, bootstrap: ("host.product.root_retain"), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            ProviderRootRelease {
                "Release a whole-product root after protected callbacks return.", "provider_root_release",
                native: (PROVIDER_ROOT_RELEASE_SYMBOL = "bray_runtime_provider_root_release", [Usize] -> Void),
                service: Host,
                call_hook: (),
                compiler: C [Usize] -> Void,
                owner: Host, availability: All, bootstrap: ("host.product.root_release"), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            ProviderRootReferences {
                "Observe root references while their enclosing owner remains live.", "provider_root_references",
                native: (PROVIDER_ROOT_REFERENCES_SYMBOL = "bray_runtime_provider_root_references", [Usize] -> Usize),
                service: Host,
                call_hook: (),
                compiler: C [Usize] -> Usize,
                owner: Host, availability: All, bootstrap: ("host.product.root_references"), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            ProductCleanupContinuation {
                "Run Bray-owned terminal work borrowed by its synchronous execution context.", "product_cleanup_continuation",
                native: (PRODUCT_CLEANUP_CONTINUATION_SYMBOL = "bray_runtime_product_cleanup_continuation", [Usize] -> U32),
                service: Host,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: ("host.product.cleanup_continuation"), host_control: false,
                capabilities: [],
                effects: [ControlProductHost]
            }
            ProductExecutionProvider {
                "Acquire demanded execution resources for the Bray-owned product terminal state.", "product_execution_provider",
                native: (PRODUCT_EXECUTION_PROVIDER_SYMBOL = "bray_runtime_product_execution_provider", [U32, Pointer] -> U32),
                service: Host,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [AdmitCleanupStorage]
            }
            RootCancellationRequest {
                "Request cancellation of the root run from its host.", "root_cancellation_request",
                native: (ROOT_CANCELLATION_REQUEST_SYMBOL = "bray_runtime_root_cancellation_request", [U64] -> U32),
                service: Execution,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Cancellation, availability: All, bootstrap: (), host_control: true,
                capabilities: [],
                effects: [RequestCancellation]
            }
            TaskControlCreation {
                "Create the runtime identity for resident Bray task control.", "task_control_creation",
                native: (TASK_CONTROL_CREATION_SYMBOL = "bray_runtime_task_control_creation", [] -> U64),
                service: Execution,
                call_hook: (),
                compiler: C [] -> U64,
                owner: Host, availability: All, bootstrap: ("task.creation"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [AllocateTask]
            }
            TaskControlAdmission {
                "Admit a Bray control cell before transferring an inactive frame.", "task_control_admission",
                native: (TASK_CONTROL_ADMISSION_SYMBOL = "bray_runtime_task_control_admission", [U64] -> U64),
                service: Execution,
                call_hook: (),
                compiler: C [U64, Pointer] -> U64,
                owner: Host, availability: All, bootstrap: ("task.admission"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [AllocateTask, AdmitCleanupStorage]
            }
            TaskControlRetention {
                "Retain a resident cell independently of the source resolution owner.", "task_control_retention",
                native: (TASK_CONTROL_RETENTION_SYMBOL = "bray_runtime_task_control_retention", [U64, U64, Pointer] -> U32),
                service: Execution,
                call_hook: (),
                compiler: C [U64, U64, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("task.retention"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: []
            }
            TaskControlSnapshot {
                "Read scheduling metadata while retaining the actual Bray cell.", "task_control_snapshot",
                native: (TASK_CONTROL_SNAPSHOT_SYMBOL = "bray_runtime_task_control_snapshot", [Pointer, Pointer] -> U32),
                service: Execution,
                call_hook: (),
                compiler: C [Pointer, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("task.snapshot"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: []
            }
            TaskControlPublication {
                "Transfer an activated frame into its admitted Bray control record.", "task_control_publication",
                native: (TASK_CONTROL_PUBLICATION_SYMBOL = "bray_runtime_task_control_publication", [Pointer, Usize, Pointer] -> U32),
                service: Execution,
                call_hook: (),
                compiler: C [Pointer, Usize, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("task.publication"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [TransferFrame, PublishWork]
            }
            TaskControlDispatch {
                "Drive one exclusively claimed Bray task outside its cell lock.", "task_control_dispatch",
                native: (TASK_CONTROL_DISPATCH_SYMBOL = "bray_runtime_task_control_dispatch", [Pointer, Pointer, Pointer, Pointer] -> U32),
                service: Execution,
                call_hook: (),
                compiler: C [Pointer, Pointer, Pointer, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("task.dispatch"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [ExecuteCallbackRoot, PublishTerminalState]
            }
            TaskControlObservation {
                "Move one terminal outcome from the sole Bray source claim.", "task_control_observation",
                native: (TASK_CONTROL_OBSERVATION_SYMBOL = "bray_runtime_task_control_observation", [Pointer, Pointer] -> U32),
                service: Execution,
                call_hook: (),
                compiler: C [Pointer, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("task.observation"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [AcquireTerminalState, EstablishVisibility]
            }
            TaskControlDestruction {
                "Resolve the sole source claim after frame retirement.", "task_control_destruction",
                native: (TASK_CONTROL_DESTRUCTION_SYMBOL = "bray_runtime_task_control_destruction", [Pointer] -> U32),
                service: Execution,
                call_hook: (),
                compiler: C [Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("task.destruction"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [ReleaseCleanupStorage]
            }
            TaskControlReferenceRelease {
                "Release one storage reference and recycle the cell after its last user.", "task_control_reference_release",
                native: (TASK_CONTROL_REFERENCE_RELEASE_SYMBOL = "bray_runtime_task_control_reference_release", [Pointer] -> U32),
                service: Execution,
                call_hook: (),
                compiler: C [Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("task.reference_release"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [ReleaseCleanupStorage]
            }
            TaskControlCallbackEntry {
                "Enter the exact producer callback while retaining its dispatch context.", "task_control_callback_entry",
                native: (TASK_CONTROL_CALLBACK_ENTRY_SYMBOL = "bray_runtime_task_control_callback_entry", [Pointer, Pointer, U8, Pointer] -> Usize),
                service: Execution,
                call_hook: (),
                compiler: C [Pointer, Pointer, U8, Pointer] -> Usize,
                owner: Host, availability: All, bootstrap: ("task.callback_enter"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: []
            }
            TaskControlCallbackExit {
                "Restore the prior producer callback after native return.", "task_control_callback_exit",
                native: (TASK_CONTROL_CALLBACK_EXIT_SYMBOL = "bray_runtime_task_control_callback_exit", [Pointer, Usize, U8] -> Void),
                service: Execution,
                call_hook: (),
                compiler: C [Pointer, Usize, U8] -> Void,
                owner: Host, availability: All, bootstrap: ("task.callback_exit"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: []
            }
            TaskControlWait {
                "Register an admitted parent cell for terminal or retirement delivery.", "task_control_wait",
                native: (TASK_CONTROL_WAIT_SYMBOL = "bray_runtime_task_control_wait", [Pointer, Pointer, U8] -> U32),
                service: Execution,
                call_hook: (),
                compiler: C [Pointer, Pointer, U8] -> U32,
                owner: Host, availability: All, bootstrap: ("task.wait"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [RegisterContinuation]
            }
            TaskControlRetirement {
                "Request retirement after the sole outcome has moved.", "task_control_retirement",
                native: (TASK_CONTROL_RETIREMENT_SYMBOL = "bray_runtime_task_control_retirement", [Pointer] -> U32),
                service: Execution,
                call_hook: (),
                compiler: C [Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("task.request_retirement"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [ReleaseCleanupStorage]
            }
            TaskControlRuntimeRetirement {
                "Release unstarted admissions after the runtime closes.", "task_control_runtime_retirement",
                native: (TASK_CONTROL_RUNTIME_RETIREMENT_SYMBOL = "bray_runtime_task_control_runtime_retirement", [Pointer] -> U32),
                service: Execution,
                call_hook: (),
                compiler: C [Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("task.runtime_retirement"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [ReleaseCleanupStorage]
            }
            TaskControlComposition {
                "Compose beneath the current authentic producer callback.", "task_control_composition",
                native: (TASK_CONTROL_COMPOSITION_SYMBOL = "bray_runtime_task_control_composition", [Pointer, Pointer, U32, U64, Usize, U8, Pointer] -> U32),
                service: Execution,
                call_hook: (),
                compiler: C [Pointer, Pointer, U32, U64, Usize, U8, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("task.composition"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [TransferFrame]
            }
            TaskAllocation {
                "Allocate stable task-owned storage.", "task_allocation",
                native: (TASK_ALLOCATION_SYMBOL = "bray_runtime_task_allocation", [] -> TaskAllocation),
                service: Execution,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [AllocateTask]
            }
            TaskAdmissionFailure {
                "Publish the checked failure of task admission.", "task_admission_failure",
                native: (TASK_ADMISSION_FAILURE_SYMBOL = "bray_runtime_task_admission_failure", [Pointer] -> Void),
                call_hook: (),
                compiler: C [Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("task.admission_failure"), host_control: false,
                capabilities: [],
                effects: [ConstructPanicReport]
            }
            TaskStart {
                "Publish a newly initialized task for execution.", "task_start",
                native: (TASK_START_SYMBOL = "bray_runtime_task_start", [U64, InactiveFrame] -> U32),
                service: Execution,
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
                service: Execution,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [RegisterContinuation]
            }
            Wake {
                "Request another dispatch of a run.", "wake",
                native: (WAKE_SYMBOL = "bray_runtime_wake", [U64] -> U32),
                service: Execution,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [PublishWork, EstablishVisibility]
            }
            TaskCancellationRequest {
                "Request cancellation of a child task.", "task_cancellation_request",
                native: (TASK_CANCELLATION_REQUEST_SYMBOL = "bray_runtime_task_cancellation_request", [U64] -> U32),
                service: Execution,
                call_hook: (),
                compiler: Bray [U64] -> U32,
                owner: Cancellation, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [RequestCancellation]
            }
            CurrentRunCancellationObservation {
                "Observe cancellation requested for the current run.", "current_run_cancellation_observation",
                native: (CURRENT_RUN_CANCELLATION_OBSERVATION_SYMBOL = "bray_runtime_current_run_cancellation_observation", [] -> U8),
                service: Host,
                call_hook: (),
                compiler: Bray [] -> Bool,
                owner: Bootstrap, availability: All, bootstrap: ("entry.control.observe"), host_control: false,
                capabilities: [],
                effects: [ObserveCancellation]
            }
            CleanupShieldEnter {
                "Defer cancellation delivery during current-run cleanup.", "cleanup_shield_enter",
                native: (CLEANUP_SHIELD_ENTER_SYMBOL = "bray_runtime_cleanup_shield_enter", [] -> Void),
                service: Host,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Bootstrap, availability: All, bootstrap: ("entry.control.enter_cleanup"), host_control: false,
                capabilities: [],
                effects: []
            }
            CleanupShieldLeave {
                "Restore current-run cancellation delivery after cleanup.", "cleanup_shield_leave",
                native: (CLEANUP_SHIELD_LEAVE_SYMBOL = "bray_runtime_cleanup_shield_leave", [] -> Void),
                service: Host,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Bootstrap, availability: All, bootstrap: ("entry.control.leave_cleanup"), host_control: false,
                capabilities: [],
                effects: []
            }
            TaskObservationCreation {
                "Create one lazy task-observation frame.", "task_observation_creation",
                native: (TASK_OBSERVATION_CREATION_SYMBOL = "bray_runtime_task_observation_creation", [U64, U8, Pointer, Pointer, Pointer] -> InactiveFrame),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: ("task.observer.create"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [CreateFrame]
            }
            TaskObservationResume {
                "Resume one task observation within its active protected execution.", "task_observation_resume",
                native: (TASK_OBSERVATION_RESUME_SYMBOL = "bray_runtime_task_observation_resume", [Pointer, Usize] -> Void),
                call_hook: (),
                compiler: C [] -> Void,
                owner: Scheduler, availability: All, bootstrap: ("task.observer.resume"), host_control: false,
                capabilities: [],
                effects: []
            }
            TaskObservationCancel {
                "Cancel one task observation within its active protected execution.", "task_observation_cancel",
                native: (TASK_OBSERVATION_CANCEL_SYMBOL = "bray_runtime_task_observation_cancel", [Pointer, Usize] -> Void),
                call_hook: (),
                compiler: C [] -> Void,
                owner: Scheduler, availability: All, bootstrap: ("task.observer.cancel"), host_control: false,
                capabilities: [],
                effects: []
            }
            TaskObservationBroadcast {
                "Broadcast cancellation to one retained task observation.", "task_observation_broadcast",
                native: (TASK_OBSERVATION_BROADCAST_SYMBOL = "bray_runtime_task_observation_broadcast", [Pointer, Usize] -> Void),
                call_hook: (),
                compiler: C [] -> Void,
                owner: Scheduler, availability: All, bootstrap: ("task.observer.broadcast"), host_control: false,
                capabilities: [],
                effects: []
            }
            TaskObservationResolve {
                "Resolve one retained task observation.", "task_observation_resolve",
                native: (TASK_OBSERVATION_RESOLVE_SYMBOL = "bray_runtime_task_observation_resolve", [Pointer, Usize, U32] -> Void),
                call_hook: (),
                compiler: C [] -> Void,
                owner: Scheduler, availability: All, bootstrap: ("task.observer.resolve"), host_control: false,
                capabilities: [],
                effects: []
            }
            TaskObservationMove {
                "Move one resolved task observation completion.", "task_observation_move",
                native: (TASK_OBSERVATION_MOVE_SYMBOL = "bray_runtime_task_observation_move", [Usize, Usize] -> Void),
                call_hook: (),
                compiler: C [] -> Void,
                owner: Scheduler, availability: All, bootstrap: ("task.observer.move_completion"), host_control: false,
                capabilities: [],
                effects: []
            }
            TaskObservationDestroy {
                "Destroy one resolved task observation.", "task_observation_destroy",
                native: (TASK_OBSERVATION_DESTROY_SYMBOL = "bray_runtime_task_observation_destroy", [Usize] -> Void),
                call_hook: (),
                compiler: C [] -> Void,
                owner: Scheduler, availability: All, bootstrap: ("task.observer.destroy"), host_control: false,
                capabilities: [],
                effects: []
            }
            TaskObservationQuery {
                "Query one task observation execution requirement.", "task_observation_query",
                native: (TASK_OBSERVATION_QUERY_SYMBOL = "bray_runtime_task_observation_query", [Usize, U32, U32, Usize] -> FrameState),
                call_hook: (),
                compiler: C [] -> Void,
                owner: Scheduler, availability: All, bootstrap: ("task.observer.query_requirements"), host_control: false,
                capabilities: [],
                effects: []
            }
            TaskObservationService {
                "Borrow the current native execution context for a Bray observation.", "task_observation_service",
                native: (TASK_OBSERVATION_SERVICE_SYMBOL = "bray_runtime_task_observation_service", [U64, U32, Pointer] -> U32),
                service: Execution,
                call_hook: (),
                compiler: C [U64, U32, Pointer] -> U32,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: []
            }
            TaskResolution {
                "Resolve one task to its terminal result.", "task_resolution",
                native: (TASK_RESOLUTION_SYMBOL = "bray_runtime_task_resolution", [U64, Pointer, Pointer] -> U32),
                service: Execution,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [AcquireTerminalState, EstablishVisibility]
            }
            TaskResultTransfer {
                "Move one owned terminal outcome into its compiler-described result.", "task_result_transfer",
                native: (TASK_RESULT_TRANSFER_SYMBOL = "bray_runtime_task_result_transfer", [Pointer, Pointer, Pointer] -> U32),
                call_hook: (),
                compiler: C [Pointer, Pointer, Pointer] -> U32,
                owner: Scheduler, availability: All, bootstrap: ("task.result.transfer"), host_control: false,
                capabilities: [],
                effects: [MoveCompletion]
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
                service: Execution,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Event, availability: All, bootstrap: (), host_control: false,
                capabilities: [Reactor],
                effects: [ExecuteCallbackRoot]
            }
            CompatibleLaneSelection {
                "Select a lane compatible with checked execution requirements.", "compatible_lane_selection",
                native: (COMPATIBLE_LANE_SELECTION_SYMBOL = "bray_runtime_compatible_lane_selection", [U64, U32] -> LaneResult),
                service: Execution,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: []
            }
            CleanupIncidentTransfer {
                "Transfer ownership of one cleanup incident.", "cleanup_incident_transfer",
                native: (CLEANUP_INCIDENT_TRANSFER_SYMBOL = "bray_runtime_cleanup_incident_transfer", [Pointer, PanicReport] -> PanicReport),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: ("report.incident.retain"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            CleanupIncidentReporting {
                "Report and destroy product-host cleanup incidents.", "cleanup_incident_reporting",
                native: (CLEANUP_INCIDENT_REPORTING_SYMBOL = "bray_runtime_cleanup_incident_reporting", [] -> U32),
                service: Host,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: true,
                capabilities: [],
                effects: [ReportCleanupIncidents]
            }
            MainThreadLaneStartup {
                "Initialize the distinguished main-thread execution lane.", "main_thread_lane_startup",
                native: (MAIN_THREAD_LANE_STARTUP_SYMBOL = "bray_runtime_main_thread_lane_startup", [Configuration] -> U32),
                service: Execution,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution, MainThreadLane],
                effects: []
            }
            MainThreadLaneDrive {
                "Drive work assigned to the distinguished main-thread lane.", "main_thread_lane_drive",
                native: (MAIN_THREAD_LANE_DRIVE_SYMBOL = "bray_runtime_main_thread_lane_drive", [] -> U32),
                service: Execution,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution, MainThreadLane],
                effects: []
            }
            RootTerminalObservation {
                "Observe the root terminal record without creating a source task.", "root_terminal_observation",
                native: (ROOT_TERMINAL_OBSERVATION_SYMBOL = "bray_runtime_root_terminal_observation", [U64] -> RunOutcome),
                service: Execution,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: true,
                capabilities: [],
                effects: [AcquireTerminalState, EstablishVisibility]
            }
            RootCompletionResolution {
                "Release runtime-owned root completion storage after host resolution.", "root_completion_resolution",
                native: (ROOT_COMPLETION_RESOLUTION_SYMBOL = "bray_runtime_root_completion_resolution", [U64] -> U32),
                service: Execution,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: (), host_control: true,
                capabilities: [],
                effects: [ReleaseRootCompletion]
            }
            PanicReporting {
                "Report and resolve one root panic payload.", "panic_reporting",
                native: (PANIC_REPORTING_SYMBOL = "bray_runtime_panic_reporting", [PanicReportPointer] -> U32),
                service: Host,
                call_hook: (NativeThreadPanicReporting),
                compiler: Bray [PanicReport] -> U32,
                owner: Bootstrap, availability: All, bootstrap: ("panic_reporting"), host_control: true,
                capabilities: [],
                effects: [ReportPanic]
            }
            PanicReportDestruction {
                "Destroy one handled panic report without reporting it.", "panic_report_destruction",
                native: (PANIC_REPORT_DESTRUCTION_SYMBOL = "bray_runtime_panic_report_destruction", [PanicReportPointer] -> U32),
                service: Host,
                call_hook: (),
                compiler: Bray [PanicReport] -> U32,
                owner: Bootstrap, availability: All, bootstrap: ("panic_report_destruction"), host_control: false,
                capabilities: [],
                effects: [DestroyPanicReport]
            }
            CleanupStorageAdmission {
                "Acquire concrete backing before accepting a cleanup obligation.", "cleanup_storage_admission",
                native: (CLEANUP_STORAGE_ADMISSION_SYMBOL = "bray_runtime_cleanup_storage_admission", [Pointer, Pointer] -> U32),
                service: Host,
                call_hook: (),
                compiler: C [Pointer, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("storage.admit"), host_control: false,
                capabilities: [],
                effects: [AdmitCleanupStorage]
            }
            CleanupStorageActivation {
                "Activate a matching reserved cleanup region once.", "cleanup_storage_activation",
                native: (CLEANUP_STORAGE_ACTIVATION_SYMBOL = "bray_runtime_cleanup_storage_activation", [Pointer, Pointer] -> Pointer),
                service: Host,
                call_hook: (),
                compiler: C [Pointer, Pointer] -> Pointer,
                owner: Host, availability: All, bootstrap: ("storage.activate"), host_control: false,
                capabilities: [],
                effects: [ActivateCleanupStorage]
            }
            CleanupStorageClaim {
                "Transfer admitted owner backing to a concrete cleanup activation.", "cleanup_storage_claim",
                native: (CLEANUP_STORAGE_CLAIM_SYMBOL = "bray_runtime_cleanup_storage_claim", [Pointer, Pointer] -> Void),
                service: Host,
                call_hook: (),
                compiler: C [Pointer, Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("storage.claim"), host_control: false,
                capabilities: [],
                effects: [ActivateCleanupStorage]
            }
            ActivationHeaderLayout {
                "Describe the provider header's target layout.", "activation_header_layout",
                native: (ACTIVATION_HEADER_LAYOUT_SYMBOL = "bray_runtime_activation_header_layout", [Pointer, Pointer] -> Void),
                call_hook: (),
                compiler: C [Pointer, Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("activation.provider.layout"), host_control: false,
                capabilities: [], effects: []
            }
            ActivationInitialization {
                "Initialize an exclusively owned native provider header.", "activation_initialization",
                native: (ACTIVATION_INITIALIZATION_SYMBOL = "bray_runtime_activation_initialization", [Pointer, Usize, Pointer] -> Void),
                call_hook: (),
                compiler: C [Pointer, Usize, Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("activation.provider.initialize"), host_control: false,
                capabilities: [], effects: [CreateFrame]
            }
            ActivationPublication {
                "Publish initialized activation backing before transferring its token.", "activation_publication",
                native: (ACTIVATION_PUBLICATION_SYMBOL = "bray_runtime_activation_publication", [Pointer] -> U32),
                service: Host,
                call_hook: (),
                compiler: C [Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("activation.publication"), host_control: false,
                capabilities: [],
                effects: [ActivateCleanupStorage]
            }
            CleanupStorageRelease {
                "Release concrete backing after its final initialized user.", "cleanup_storage_release",
                native: (CLEANUP_STORAGE_RELEASE_SYMBOL = "bray_runtime_cleanup_storage_release", [Pointer] -> U32),
                service: Host,
                call_hook: (),
                compiler: C [Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("storage.retire"), host_control: false,
                capabilities: [],
                effects: [ReleaseCleanupStorage]
            }
            OutgoingAdmission {
                "Reserve local outgoing records before accepting an owner.", "outgoing_admission",
                native: (OUTGOING_ADMISSION_SYMBOL = "bray_runtime_outgoing_admission", [Pointer, Pointer] -> Void),
                service: Host,
                call_hook: (),
                compiler: C [Pointer, Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("outgoing.admission"), host_control: false,
                capabilities: [],
                effects: [ConstructPanicReport]
            }
            OutgoingDischarge {
                "Discharge a consumed owner without reclaiming transferred records.", "outgoing_discharge",
                native: (OUTGOING_DISCHARGE_SYMBOL = "bray_runtime_outgoing_discharge", [Pointer] -> Void),
                service: Host,
                call_hook: (),
                compiler: C [Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("outgoing.discharge"), host_control: false,
                capabilities: [],
                effects: [DestroyPanicReport]
            }
            OutgoingActivation {
                "Activate an admitted outgoing record before entering a local cleanup action.", "outgoing_activation",
                native: (OUTGOING_ACTIVATION_SYMBOL = "bray_runtime_outgoing_activation", [Pointer, Pointer] -> Usize),
                service: Host,
                call_hook: (),
                compiler: C [Pointer, Pointer] -> Usize,
                owner: Host, availability: All, bootstrap: ("outgoing.activation"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            OutgoingRetirement {
                "Retire an action record or transfer it with its outgoing report.", "outgoing_retirement",
                native: (OUTGOING_RETIREMENT_SYMBOL = "bray_runtime_outgoing_retirement", [Usize, Pointer, Pointer] -> Void),
                service: Host,
                call_hook: (),
                compiler: C [Usize, Pointer, Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("outgoing.retirement"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            PanicReportSuppression {
                "Attach an owned cleanup incident to the primary panic report.", "panic_report_suppression",
                native: (PANIC_REPORT_SUPPRESSION_SYMBOL = "bray_runtime_panic_report_suppression", [PanicReportPointer, PanicReportPointer] -> PanicReport),
                service: Host,
                call_hook: (),
                compiler: Bray [PanicReport, PanicReport] -> PanicReport,
                owner: Host, availability: All, bootstrap: ("panic_report_suppression"), host_control: false,
                capabilities: [],
                effects: [TransferCleanupIncident]
            }
            EntryFailureReporting {
                "Report one recoverable entrypoint failure value before host resolution.", "entry_failure_reporting",
                native: (ENTRY_FAILURE_REPORTING_SYMBOL = "bray_runtime_entry_failure_reporting", [Usize, Usize] -> U32),
                service: Host,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Bootstrap, availability: All, bootstrap: ("report.output.entry_failure"), host_control: true,
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
                native: (FRAME_COMPLETION_MOVE_SYMBOL = "bray_runtime_frame_completion_move", [Pointer, Pointer] -> Void),
                call_hook: (),
                compiler: C [Pointer, Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("activation.completion.take"), host_control: false,
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
            PanicReportConstruction {
                "Construct one owned panic report.", "panic_report_construction",
                native: (PANIC_REPORT_CONSTRUCTION_SYMBOL = "bray_runtime_panic_report_construction", [U32, U32, U32, U32, U32, U64, U64, U64, U64, U64, Pointer, Usize, Usize] -> PanicReport),
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Host, availability: All, bootstrap: ("panic_report_construction"), host_control: false,
                capabilities: [],
                effects: [ConstructPanicReport]
            }
            PanicPropagation {
                "Report and terminate an uncaught native-boundary failure.", "panic_propagation",
                native: (PANIC_PROPAGATION_SYMBOL = "bray_runtime_panic_propagation", [PanicReportPointer] -> Never),
                service: Host,
                call_hook: (),
                compiler: Bray [PanicReport] -> Void,
                owner: Bootstrap, availability: All, bootstrap: ("panic_propagation"), host_control: false,
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
            ActivationInspection {
                "Inspect an inactive frame's requirements without consuming its registered lease.", "activation_inspection",
                native: (ACTIVATION_INSPECTION_SYMBOL = "bray_runtime_activation_inspection", [Pointer, U32, U32, Usize, Pointer] -> U32),
                call_hook: (),
                compiler: C [Pointer, U32, U32, Usize, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("activation.requirements.inspect"), host_control: false,
                capabilities: [],
                effects: []
            }
            ActivationSnapshot {
                "Read an active frame's combined execution requirements without transferring ownership.", "activation_snapshot",
                native: (ACTIVATION_SNAPSHOT_SYMBOL = "bray_runtime_activation_snapshot", [Pointer, U32, U32, Usize, Pointer] -> U32),
                call_hook: (),
                compiler: C [Pointer, U32, U32, Usize, Pointer] -> U32,
                owner: Bootstrap, availability: All, bootstrap: ("activation.requirements.snapshot"), host_control: false,
                capabilities: [],
                effects: []
            }
            ActivationEntry {
                "Activate authorized resident storage, leaving token consumption to the successful caller.", "activation_entry",
                native: (ACTIVATION_ENTRY_SYMBOL = "bray_runtime_activation_entry", [Pointer, U64] -> Usize),
                call_hook: (),
                compiler: C [Pointer, U64] -> Usize,
                owner: Host, availability: All, bootstrap: ("activation.activate"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [ActivateCleanupStorage]
            }
            ActivationComposition {
                "Attach an admitted child using its parent's checked continuation requirements.", "activation_composition",
                native: (ACTIVATION_COMPOSITION_SYMBOL = "bray_runtime_activation_composition", [Pointer, Pointer, U32, U64, Usize, U8, Pointer] -> Usize),
                call_hook: (),
                compiler: C [Pointer, Pointer, U32, U64, Usize, U8, Pointer] -> Usize,
                owner: Host, availability: All, bootstrap: ("activation.composition.compose"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [ComposeAwaitedFrame]
            }
            ActivationRunSnapshot {
                "Read the current run continuation for scheduler publication.", "activation_run_snapshot",
                native: (ACTIVATION_RUN_SNAPSHOT_SYMBOL = "bray_runtime_activation_run_snapshot", [Pointer, U8, Pointer] -> Void),
                call_hook: (),
                compiler: C [Pointer, U8, Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("activation.run.snapshot"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: []
            }
            ActivationStep {
                "Advance the exclusively owned activation chain by one producer transition.", "activation_step",
                native: (ACTIVATION_STEP_SYMBOL = "bray_runtime_activation_step", [Pointer, Pointer, Pointer, U8, U8, Pointer, Pointer, Pointer] -> U32),
                call_hook: (), compiler: C [Pointer, Pointer, Pointer, U8, U8, Pointer, Pointer, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("activation.run.dispatch"), host_control: false,
                capabilities: [CooperativeExecution], effects: []
            }
            ActivationDispatch {
                "Dispatch a retaining callback or retire a committed activation without accessing released storage.", "activation_dispatch",
                native: (ACTIVATION_DISPATCH_SYMBOL = "bray_runtime_activation_dispatch", [Pointer, Pointer, Pointer, U32, Usize, Pointer, Pointer, Pointer] -> U32),
                call_hook: (),
                compiler: C [Pointer, Pointer, Pointer, U32, Usize, Pointer, Pointer, Pointer] -> U32,
                owner: Host, availability: All, bootstrap: ("activation.execution.execute"), host_control: false,
                capabilities: [CooperativeExecution],
                effects: []
            }
            ActivationCallbackAdmission {
                "Reserve native unwind payload backing before accepting a callback owner.", "activation_callback_admission",
                native: (ACTIVATION_CALLBACK_ADMISSION_SYMBOL = "bray_runtime_activation_callback_admission", [Usize, Pointer] -> U32),
                call_hook: (),
                compiler: C [Usize, Pointer] -> U32,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [TransferCleanupIncident]
            }
            ActivationCallback {
                "Contain one native producer callback within its Bray activation.", "activation_callback",
                native: (ACTIVATION_CALLBACK_SYMBOL = "bray_runtime_activation_callback", [Pointer, Pointer, Usize, U32, Usize, Pointer, Pointer] -> Void),
                call_hook: (),
                compiler: C [Pointer, Pointer, Usize, U32, Usize, Pointer, Pointer] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: []
            }
            ActivationActionReadiness {
                "Check a synchronous described action against its retained caller execution context.", "activation_action_readiness",
                native: (ACTIVATION_ACTION_READINESS_SYMBOL = "bray_runtime_activation_action_readiness", [Pointer] -> U32),
                call_hook: (), compiler: C [Pointer] -> U32,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution], effects: []
            }
            InactiveFrameBroadcast {
                "Broadcast cancellation to an inactive computation's captured owners.", "inactive_frame_broadcast",
                native: (INACTIVE_FRAME_BROADCAST_SYMBOL = "bray_runtime_inactive_frame_broadcast", [Pointer, Pointer, Pointer, Pointer] -> Void),
                call_hook: (), compiler: C [Pointer, Pointer, Pointer, Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("activation.inactive.broadcast"), host_control: false,
                capabilities: [CooperativeExecution], effects: []
            }
            InactiveFrameResolution {
                "Resolve synchronously admitted inactive captures and consume their computation's backing.", "inactive_frame_resolution",
                native: (INACTIVE_FRAME_RESOLUTION_SYMBOL = "bray_runtime_inactive_frame_resolution", [Pointer, Pointer, Pointer, Pointer] -> Void),
                call_hook: (), compiler: C [Pointer, Pointer, Pointer, Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("activation.inactive.dispose"), host_control: false,
                capabilities: [CooperativeExecution], effects: []
            }
            InactiveFrameDestruction {
                "Destroy quiescent captures and consume their inactive computation's backing.", "inactive_frame_destruction",
                native: (INACTIVE_FRAME_DESTRUCTION_SYMBOL = "bray_runtime_inactive_frame_destruction", [Pointer, Pointer, Pointer, Pointer] -> Void),
                call_hook: (), compiler: C [Pointer, Pointer, Pointer, Pointer] -> Void,
                owner: Host, availability: All, bootstrap: ("activation.inactive.destroy"), host_control: false,
                capabilities: [CooperativeExecution], effects: []
            }
            AwaitedFrameComposition {
                "Compose one erased directly awaited frame into its parent.", "awaited_frame_composition",
                native: (AWAITED_FRAME_COMPOSITION_SYMBOL = "bray_runtime_awaited_frame_composition", [InactiveFrame, U32, Usize, U8] -> Void),
                service: Execution,
                call_hook: (),
                compiler: Bray [] -> Void,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [ComposeAwaitedFrame]
            }
            TaskDestruction {
                "Infallibly destroy one terminal task control record.", "task_destruction",
                native: (TASK_DESTRUCTION_SYMBOL = "bray_runtime_task_destruction", [U64] -> U32),
                service: Execution,
                call_hook: (),
                compiler: Bray [U64] -> U32,
                owner: Scheduler, availability: All, bootstrap: (), host_control: false,
                capabilities: [CooperativeExecution],
                effects: [DestroyTask]
            }
            ExecutionShutdown {
                "Retire selected execution and test-host services before common host release.", "execution_shutdown",
                native: (EXECUTION_SHUTDOWN_SYMBOL = "bray_runtime_execution_shutdown", [] -> U32),
                call_hook: (),
                compiler: C [] -> U32,
                owner: Host, availability: All, bootstrap: (), host_control: false,
                capabilities: [],
                effects: [StructuredShutdown]
            }
            ControlStorageExit {
                "Retire the native execution-control projection without admitting entry.", "control_storage_exit",
                native: (CONTROL_STORAGE_EXIT_SYMBOL = "bray_runtime_control_storage_exit", [Pointer] -> Void),
                call_hook: (),
                compiler: C [Pointer] -> Void,
                owner: Bootstrap, availability: All, bootstrap: ("thread.destruction.control_storage_exit"), host_control: false,
                capabilities: [],
                effects: []
            }
        }
    };
}
