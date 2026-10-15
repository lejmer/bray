use bray_runtime_abi::{
    NativePanicPrimary, NativePanicReport, NativeReportRecords, NativeReportSegment,
    NativeRunOutcome,
};

// These declarations bind the compiler-owned trusted Bray provider. The private callers
// transfer only live, exclusively owned handles and keep the linked image resident.
#[expect(
    unsafe_code,
    reason = "the linked Bray provider supplies the record ownership ABI"
)]
unsafe extern "C" {
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

    pub(crate) safe fn bray_runtime_report_records_pop(
        chain: &mut NativeReportRecords,
        primary: &mut NativePanicPrimary,
    );

    pub(crate) safe fn bray_runtime_report_segment_mark(
        chain: &mut NativeReportRecords,
        metadata: &NativeReportSegment,
    );

    pub(crate) safe fn bray_runtime_report_segment_take(
        chain: &mut NativeReportRecords,
        metadata: &mut NativeReportSegment,
        destination: &mut NativeReportRecords,
    );

    pub(crate) safe fn bray_runtime_report_consumer(
        report: &mut NativePanicReport,
        reporting: u32,
    ) -> bray_runtime_abi::NativeRuntimeStatus;

    pub(crate) safe fn bray_runtime_outgoing_admission(
        bundle: &bray_runtime_abi::NativeCleanupStorageBundle<'_>,
        outcome: &mut NativeRunOutcome,
    );

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
