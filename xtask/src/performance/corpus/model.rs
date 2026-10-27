use crate::performance::model::WorkloadCategory;

pub(in crate::performance) const CORPUS_REVISION: u32 = 1;
pub(in crate::performance) const CALIBRATION_SEED_INNER_ITERATIONS: u64 = 1_000_000;
pub(in crate::performance) const CALIBRATION_SAMPLE_COUNT: u32 = 3;
pub(in crate::performance) const CALIBRATION_TARGET_NANOSECONDS: u64 = 100_000_000;

pub(in crate::performance) struct Workload {
    pub id: &'static str,
    pub category: WorkloadCategory,
    pub scale: u64,
    pub units: &'static str,
    pub batching: BatchingPolicy,
    pub source: &'static str,
    pub expected_output: ExpectedOutput,
    pub expected_side_effects: ExpectedSideEffects,
    pub platform_operations: &'static [&'static str],
    pub retention: RetentionContract,
    pub storage: Option<StorageExpectation>,
}

impl Workload {
    pub(in crate::performance) fn storage_expectation(
        &self,
        target: bray_target::NativeTarget,
    ) -> Option<StorageExpectation> {
        if self.id != "file_output" {
            return self.storage;
        }

        let ExpectedSideEffects::AbsentPath(path) = self.expected_side_effects else {
            panic!("file output must identify its frozen path");
        };

        let unit_bytes = if target.object_format() == bray_target::ObjectFormat::Coff {
            2
        } else {
            1
        };

        let path_bytes = path.len() as u64 * unit_bytes;

        // One native path, three terminated paths (remove/open/remove), and the 256-byte writer.
        // UTF-8 path construction copies bytes. UTF-16 construction encodes scalars directly.
        Some(StorageExpectation {
            allocation_count: 5,
            allocated_bytes: 256 + path_bytes + 3 * (path_bytes + unit_bytes),
            copied_bytes: 3 * path_bytes + if unit_bytes == 1 { path_bytes } else { 0 },
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::performance) enum BatchingPolicy {
    SingleExecution,
    Calibrated,
}

pub(in crate::performance) struct RetentionContract {
    pub required_symbols: &'static [&'static str],
    pub forbidden_symbols: &'static [&'static str],
    pub required_provenance: &'static [&'static str],
    pub forbidden_provenance: &'static [&'static str],
}

pub(in crate::performance) const NO_RETENTION_CONTRACT: RetentionContract = RetentionContract {
    required_symbols: &[],
    forbidden_symbols: &[],
    required_provenance: &[],
    forbidden_provenance: &[],
};

pub(super) const NO_CALLBACK_RETENTION_CONTRACT: RetentionContract = RetentionContract {
    required_symbols: &[],
    forbidden_symbols: &["bray_runtime_foreign_callback_execution"],
    required_provenance: &[],
    forbidden_provenance: &[],
};

#[derive(Clone, Copy)]
pub(in crate::performance) enum ExpectedOutput {
    Empty,
    Repeated { byte: u8, count: u64 },
}

#[derive(Clone, Copy)]
pub(in crate::performance) enum ExpectedSideEffects {
    None,
    AbsentPath(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::performance) struct StorageExpectation {
    pub allocation_count: u64,
    pub allocated_bytes: u64,
    pub copied_bytes: u64,
}
