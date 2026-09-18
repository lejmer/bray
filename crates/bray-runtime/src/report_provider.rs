use bray_runtime_abi::{
    NativePanicPrimary, NativePanicReport, NativePanicReportConsumer, NativeReportRecords,
    NativeReportSegment, NativeRunOutcome,
};

// These declarations bind the compiler-owned trusted Bray provider. The private callers
// transfer only live, exclusively owned handles and keep the linked image resident.
#[expect(
    unsafe_code,
    reason = "the linked Bray provider supplies the record ownership ABI"
)]
unsafe extern "C" {
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
    pub(crate) safe fn bray_runtime_report_consumer() -> NativePanicReportConsumer;
    pub(crate) safe fn bray_runtime_outgoing_admission(
        count: usize,
        outcome: &mut NativeRunOutcome,
    );
    pub(crate) safe fn bray_runtime_outgoing_discharge(count: usize);
    pub(crate) safe fn bray_runtime_outgoing_activation() -> usize;
    pub(crate) safe fn bray_runtime_outgoing_retirement(
        record: usize,
        outcome: &mut NativeRunOutcome,
    );
    pub(crate) safe fn bray_runtime_panic_report_suppression(
        primary: &mut NativePanicReport,
        incident: &mut NativePanicReport,
    ) -> NativePanicReport;
}
