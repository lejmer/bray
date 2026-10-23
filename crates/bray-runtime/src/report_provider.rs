#[cfg(test)]
use bray_runtime_abi::NativeRunOutcome;

use bray_runtime_abi::{
    NativePanicPrimary, NativePanicReport, NativeReportRecords, NativeReportSegment,
};

// These declarations bind the compiler-owned trusted Bray provider. The private callers
// transfer only live, exclusively owned handles and keep the linked image resident.
#[expect(
    unsafe_code,
    reason = "the linked Bray provider supplies the record ownership ABI"
)]
unsafe extern "C" {
    #[cfg(test)]
    pub(crate) safe fn bray_runtime_frame_completion_move(
        parent: *mut u8,
        outcome: &mut NativeRunOutcome,
    );

    pub(crate) safe fn bray_runtime_activation_dispatch(
        root: *mut u8,
        callback: extern "C" fn(
            &bray_runtime_abi::NativeActivationExecution,
            &bray_runtime_abi::NativeActivationDescriptor,
            usize,
            u32,
            usize,
            &mut bray_runtime_abi::NativeFrameProgress,
            &mut NativeReportRecords,
        ),
        ready: extern "C" fn(&bray_runtime_abi::NativeActivationExecution) -> u32,
        operation: u32,
        argument: usize,
        progress: &mut bray_runtime_abi::NativeFrameProgress,
        records: &mut NativeReportRecords,
        incidents: &mut NativeReportRecords,
    ) -> u32;
    pub(crate) safe fn bray_runtime_activation_step(
        root: *mut u8,
        callback: extern "C" fn(
            &bray_runtime_abi::NativeActivationExecution,
            &bray_runtime_abi::NativeActivationDescriptor,
            usize,
            u32,
            usize,
            &mut bray_runtime_abi::NativeFrameProgress,
            &mut NativeReportRecords,
        ),
        ready: extern "C" fn(&bray_runtime_abi::NativeActivationExecution) -> u32,
        cancellation: u8,
        abandoning: u8,
        progress: &mut bray_runtime_abi::NativeFrameProgress,
        records: &mut NativeReportRecords,
        incidents: &mut NativeReportRecords,
    ) -> u32;
    pub(crate) safe fn bray_runtime_activation_composition(
        parent: *mut u8,
        token: &bray_runtime_abi::NativeInactiveFrame,
        state: u32,
        origin: u64,
        destination: usize,
        abandoning: u8,
        ready: extern "C" fn(&bray_runtime_abi::NativeActivationExecution) -> u32,
    ) -> usize;
    pub(crate) safe fn bray_runtime_activation_run_snapshot(
        root: *mut u8,
        cancellation: u8,
        output: &mut bray_runtime_abi::NativeActivationExecution,
    );

    pub(crate) safe fn bray_runtime_activation_inspection(
        token: &bray_runtime_abi::NativeInactiveFrame,
        state: u32,
        operation: u32,
        argument: usize,
        output: &mut bray_runtime_abi::NativeActivationExecution,
    ) -> u32;
    pub(crate) safe fn bray_runtime_activation_entry(
        token: &bray_runtime_abi::NativeInactiveFrame,
        origin: u64,
    ) -> usize;

    pub(crate) safe fn bray_runtime_activation_header_layout(
        bytes: &mut usize,
        alignment: &mut usize,
    );
    pub(crate) safe fn bray_runtime_activation_initialization(
        token: &bray_runtime_abi::NativeInactiveFrame,
        context: usize,
        outgoing: &mut NativeReportRecords,
    );
    pub(crate) safe fn bray_runtime_activation_publication(
        token: &bray_runtime_abi::NativeInactiveFrame,
    ) -> u32;

    pub(crate) safe fn bray_runtime_cleanup_storage_admission(
        descriptor: &bray_runtime_abi::NativeCleanupStorageShape,
        destination: &mut bray_runtime_abi::NativeCleanupStorageLease,
    ) -> u32;

    pub(crate) safe fn bray_runtime_cleanup_storage_activation(
        lease: &bray_runtime_abi::NativeCleanupStorageLease,
        descriptor: &bray_runtime_abi::NativeCleanupStorageShape,
    ) -> *mut u8;

    pub(crate) safe fn bray_runtime_cleanup_storage_release(
        lease: &mut bray_runtime_abi::NativeCleanupStorageLease,
    ) -> u32;

    pub(crate) safe fn bray_runtime_report_records_admit(
        count: usize,
        destination: &mut NativeReportRecords,
    ) -> u32;

    pub(crate) safe fn bray_runtime_report_records_take(
        source: &mut NativeReportRecords,
        count: usize,
        destination: &mut NativeReportRecords,
    );

    pub(crate) safe fn bray_runtime_report_record_exchange(
        handle: usize,
        primary: &mut NativePanicPrimary,
    );

    pub(crate) safe fn bray_runtime_report_records_append(
        destination: &mut NativeReportRecords,
        source: &mut NativeReportRecords,
    );

    pub(crate) safe fn bray_runtime_report_segment_take(
        chain: &mut NativeReportRecords,
        metadata: &mut NativeReportSegment,
        destination: &mut NativeReportRecords,
    );

    pub(crate) safe fn bray_runtime_report_sink_transfer(
        state: &mut crate::shutdown::CleanupReportState,
        records: &mut NativeReportRecords,
        metadata: &mut NativeReportSegment,
    );

    pub(crate) safe fn bray_runtime_report_sink_begin(
        state: &mut crate::shutdown::CleanupReportState,
    ) -> u32;
    pub(crate) safe fn bray_runtime_report_sink_take(
        state: &mut crate::shutdown::CleanupReportState,
        batch: &mut NativeReportRecords,
    ) -> u32;
    pub(crate) safe fn bray_runtime_report_sink_finish(
        state: &mut crate::shutdown::CleanupReportState,
    );

    pub(crate) safe fn bray_runtime_report_segment_consumer(
        metadata: &NativeReportSegment,
        report: &mut NativePanicReport,
    ) -> bray_runtime_abi::NativeRuntimeStatus;

    pub(crate) safe fn bray_runtime_cleanup_incident_consumer(
        incident: &mut bray_runtime_abi::NativeCleanupIncident,
        reporting: u32,
    ) -> bray_runtime_abi::NativeRuntimeStatus;

    pub(crate) safe fn bray_runtime_report_consumer(
        report: &mut NativePanicReport,
        reporting: u32,
    ) -> bray_runtime_abi::NativeRuntimeStatus;

    #[cfg(test)]
    pub(crate) safe fn bray_runtime_outgoing_admission(
        bundle: &bray_runtime_abi::NativeCleanupStorageBundle<'_>,
        outcome: &mut NativeRunOutcome,
    );

    #[cfg(test)]
    pub(crate) safe fn bray_runtime_outgoing_discharge(
        bundle: &bray_runtime_abi::NativeCleanupStorageBundle<'_>,
    );

    #[cfg(test)]
    pub(crate) safe fn bray_runtime_outgoing_activation(
        descriptor: *const bray_runtime_abi::NativeCleanupStorageShape,
        lease: *mut bray_runtime_abi::NativeCleanupStorageLease,
    ) -> usize;

    #[cfg(test)]
    pub(crate) safe fn bray_runtime_outgoing_retirement(
        record: usize,
        lease: *const bray_runtime_abi::NativeCleanupStorageLease,
        outcome: &mut NativeRunOutcome,
    );

    pub(crate) safe fn bray_runtime_panic_report_suppression(
        primary: &mut NativePanicReport,
        incident: &mut NativePanicReport,
    ) -> NativePanicReport;
}
