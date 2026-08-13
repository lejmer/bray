use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use bray_base::lowercase_hex;
use sha2::{Digest as _, Sha256};

use super::corpus::StorageExpectation;
use super::model::{
    BRAY_EXECUTION_SCOPE, ExecutionStatistics, Observation, STORAGE_OBSERVATION_SCOPE,
    WorkloadObservations,
};

const RECORD_BYTES: usize = 9;
const ALLOCATION_RECORD: u8 = 1;
const COPY_RECORD: u8 = 2;
const CONTROLLED_DURATION_RECORD: u8 = 3;

pub(super) struct ObservedExecution {
    pub execution: ExecutionStatistics,
    pub observations: WorkloadObservations,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RecordedExecution {
    duration_nanoseconds: u64,
    storage: StorageExpectation,
}

pub(super) fn require_production_symbols_absent(linker_map: &Path) -> Result<(), String> {
    let symbols = linked_symbols(linker_map)?;

    for symbol in observation_symbols() {
        if symbols.contains(symbol) {
            return Err(format!(
                "production performance artifact unexpectedly retains {symbol}"
            ));
        }
    }

    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "the observed run requires the same explicit execution contract as the production samples"
)]
pub(super) fn measure_samples(
    executable: &Path,
    linker_map: &Path,
    working_directory: &Path,
    output: &Path,
    warmup: u32,
    samples: u32,
    scale: u64,
    expected_output_sha256: &str,
    expected_storage: Option<StorageExpectation>,
) -> Result<ObservedExecution, String> {
    require_observation_symbols(linker_map, expected_storage.is_some())?;

    let capacity = usize::try_from(samples)
        .map_err(|_| "sample count cannot be represented by this host".to_owned())?;

    let mut durations = Vec::with_capacity(capacity);
    let mut sampled_storage = None;

    for iteration in 0..warmup.saturating_add(samples) {
        let observation_path = output.join(format!("performance-observations-{iteration}.bin"));

        let execution = Command::new(executable)
            .current_dir(working_directory)
            .env(
                bray_runtime_abi::PERFORMANCE_OBSERVATION_PATH_ENVIRONMENT,
                &observation_path,
            )
            .output()
            .map_err(|error| format!("could not execute observed artifact: {error}"))?;

        if !execution.status.success() {
            return Err(format!(
                "observed artifact exited unsuccessfully: {}",
                String::from_utf8_lossy(&execution.stderr).trim()
            ));
        }

        if lowercase_hex(&Sha256::digest(&execution.stdout)) != expected_output_sha256 {
            return Err("observed artifact did not produce the corpus-defined output".to_owned());
        }

        let recorded = read(&observation_path)?;

        fs::remove_file(&observation_path).map_err(|error| {
            format!(
                "could not remove performance observation {}: {error}",
                observation_path.display()
            )
        })?;

        if let Some(expected) = expected_storage
            && recorded.storage != expected
        {
            return Err(format!(
                "observed storage work differs from the corpus contract: expected {expected:?}, measured {:?}",
                recorded.storage
            ));
        }

        if iteration >= warmup {
            durations.push(recorded.duration_nanoseconds);
            sampled_storage.get_or_insert(recorded.storage);
        }
    }

    let storage = sampled_storage
        .ok_or_else(|| "at least one observed execution sample is required".to_owned())?;

    let execution = super::statistics::summarize(durations, scale, BRAY_EXECUTION_SCOPE)
        .ok_or_else(|| "at least one observed execution sample is required".to_owned())?;

    let measured = |value| Observation::Measured {
        value,
        scope: STORAGE_OBSERVATION_SCOPE.to_owned(),
    };

    Ok(ObservedExecution {
        execution,
        observations: WorkloadObservations {
            allocation_count: measured(storage.allocation_count),
            allocated_bytes: measured(storage.allocated_bytes),
            copied_bytes: measured(storage.copied_bytes),
            platform_operations: BTreeMap::new(),
        },
    })
}

fn read(path: &Path) -> Result<RecordedExecution, String> {
    let maximum = bray_runtime_abi::MAX_PERFORMANCE_OBSERVATION_RECORDS
        .checked_mul(RECORD_BYTES as u64)
        .and_then(|bytes| {
            bytes.checked_add(bray_runtime_abi::PERFORMANCE_OBSERVATION_HEADER.len() as u64)
        })
        .ok_or_else(|| "performance observation size bound overflowed".to_owned())?;

    let metadata = fs::metadata(path)
        .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;

    if metadata.len() > maximum {
        return Err("performance observation stream exceeds its record bound".to_owned());
    }

    let bytes = fs::read(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;

    let Some(records) = bytes.strip_prefix(&bray_runtime_abi::PERFORMANCE_OBSERVATION_HEADER) else {
        return Err("performance observation stream has an unsupported header".to_owned());
    };

    let mut chunks = records.chunks_exact(RECORD_BYTES);
    let mut duration_nanoseconds = None;

    let mut storage = StorageExpectation {
        allocation_count: 0,
        allocated_bytes: 0,
        copied_bytes: 0,
    };

    for record in &mut chunks {
        let value = u64::from_le_bytes(
            record[1..]
                .try_into()
                .map_err(|_| "performance observation record is truncated".to_owned())?,
        );

        match record[0] {
            ALLOCATION_RECORD => {
                storage.allocation_count = storage
                    .allocation_count
                    .checked_add(1)
                    .ok_or_else(|| "memory allocation observation count overflowed".to_owned())?;

                storage.allocated_bytes = storage
                    .allocated_bytes
                    .checked_add(value)
                    .ok_or_else(|| "memory allocation byte observation overflowed".to_owned())?;
            }
            COPY_RECORD => {
                storage.copied_bytes = storage
                    .copied_bytes
                    .checked_add(value)
                    .ok_or_else(|| "memory copy byte observation overflowed".to_owned())?;
            }
            CONTROLLED_DURATION_RECORD if duration_nanoseconds.replace(value).is_none() => {}
            CONTROLLED_DURATION_RECORD => {
                return Err("performance observation stream repeats its controlled interval".to_owned());
            }
            kind => return Err(format!("performance observation stream has unknown record {kind}")),
        }
    }

    if !chunks.remainder().is_empty() {
        return Err("performance observation stream ends with a partial record".to_owned());
    }

    let duration_nanoseconds = duration_nanoseconds
        .ok_or_else(|| "performance observation stream has no controlled interval".to_owned())?;

    Ok(RecordedExecution {
        duration_nanoseconds,
        storage,
    })
}

fn require_observation_symbols(linker_map: &Path, memory_required: bool) -> Result<(), String> {
    let symbols = linked_symbols(linker_map)?;

    let required = [
        bray_runtime_abi::PERFORMANCE_INTERVAL_BEGIN_SYMBOL,
        bray_runtime_abi::PERFORMANCE_INTERVAL_END_SYMBOL,
    ]
    .into_iter()
    .chain(memory_required.then_some(bray_runtime_abi::MEMORY_ALLOCATION_OBSERVATION_SYMBOL))
    .chain(memory_required.then_some(bray_runtime_abi::MEMORY_COPY_OBSERVATION_SYMBOL));

    for symbol in required {
        if !symbols.contains(symbol) {
            return Err(format!("observed performance artifact is missing {symbol}"));
        }
    }

    Ok(())
}

fn linked_symbols(linker_map: &Path) -> Result<String, String> {
    fs::read_to_string(linker_map)
        .map_err(|error| format!("could not read linker map {}: {error}", linker_map.display()))
}

const fn observation_symbols() -> [&'static str; 4] {
    [
        bray_runtime_abi::MEMORY_ALLOCATION_OBSERVATION_SYMBOL,
        bray_runtime_abi::MEMORY_COPY_OBSERVATION_SYMBOL,
        bray_runtime_abi::PERFORMANCE_INTERVAL_BEGIN_SYMBOL,
        bray_runtime_abi::PERFORMANCE_INTERVAL_END_SYMBOL,
    ]
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        ALLOCATION_RECORD, CONTROLLED_DURATION_RECORD, COPY_RECORD, RecordedExecution, read,
    };

    #[test]
    fn fixed_records_preserve_measured_execution_and_storage_work() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("observation directory must exist: {error}"));

        let path = directory.path().join("observations.bin");
        let mut bytes = bray_runtime_abi::PERFORMANCE_OBSERVATION_HEADER.to_vec();

        push(&mut bytes, ALLOCATION_RECORD, 1);
        push(&mut bytes, ALLOCATION_RECORD, 2);
        push(&mut bytes, COPY_RECORD, 1);
        push(&mut bytes, CONTROLLED_DURATION_RECORD, 50);

        fs::write(&path, bytes)
            .unwrap_or_else(|error| panic!("observation fixture must write: {error}"));

        assert_eq!(
            read(&path).unwrap_or_else(|error| panic!("observation must parse: {error}")),
            RecordedExecution {
                duration_nanoseconds: 50,
                storage: super::StorageExpectation {
                    allocation_count: 2,
                    allocated_bytes: 3,
                    copied_bytes: 1,
                },
            }
        );
    }

    #[test]
    fn fixed_records_reject_unknown_partial_missing_and_repeated_intervals() {
        let cases = [
            vec![7; 9],
            vec![ALLOCATION_RECORD; 1],
            record(ALLOCATION_RECORD, 1),
            [
                record(CONTROLLED_DURATION_RECORD, 1),
                record(CONTROLLED_DURATION_RECORD, 2),
            ]
            .concat(),
        ];

        for records in cases {
            let directory = tempfile::tempdir()
                .unwrap_or_else(|error| panic!("observation directory must exist: {error}"));

            let path = directory.path().join("observations.bin");
            let mut bytes = bray_runtime_abi::PERFORMANCE_OBSERVATION_HEADER.to_vec();

            bytes.extend(records);

            fs::write(&path, bytes)
                .unwrap_or_else(|error| panic!("observation fixture must write: {error}"));

            assert!(read(&path).is_err());
        }
    }

    fn record(kind: u8, value: u64) -> Vec<u8> {
        let mut bytes = Vec::new();

        push(&mut bytes, kind, value);

        bytes
    }

    fn push(bytes: &mut Vec<u8>, kind: u8, value: u64) {
        bytes.push(kind);
        bytes.extend(value.to_le_bytes());
    }
}
