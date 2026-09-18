/// Exclusively owned record chain. Handles belong to the linked report provider.
#[repr(C)]
#[derive(Debug, Default)]
pub struct NativeReportRecords {
    /// First record, or zero for an empty chain.
    pub head: usize,
    /// Last record, or zero for an empty chain.
    pub tail: usize,
    /// Number of owned records.
    pub count: usize,
}

/// Source context of one ordered cleanup-report segment.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeReportSegment {
    /// Encounter order within the cleanup sink.
    pub ordinal: u64,
    /// Producing task identity. Zero denotes the synchronous root.
    pub task: u64,
    /// Stable protected-frame identity.
    pub frame: [u8; 32],
    /// Protected-frame state that produced the incident.
    pub state: u32,
}

#[cfg(test)]
mod tests {
    use super::{NativeReportRecords, NativeReportSegment};

    #[test]
    fn report_record_ownership_and_cleanup_context_match_the_provider_abi() {
        assert_abi_layout!(NativeReportRecords, size: 24, align: 8, fields: {
            head: 0, tail: 8, count: 16,
        });

        assert_abi_layout!(NativeReportSegment, size: 56, align: 8, fields: {
            ordinal: 0, task: 8, frame: 16, state: 48,
        });
    }
}
