use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::corpus::{ExpectedSideEffects, StorageExpectation};
use super::model::{Observation, STORAGE_OBSERVATION_SCOPE, WorkloadObservations};

const RECORD_BYTES: usize = 9;
const ALLOCATION_RECORD: u8 = 1;
const COPY_RECORD: u8 = 2;
const CONTROLLED_DURATION_RECORD: u8 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RecordedExecution {
    duration_nanoseconds: Option<u64>,
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

pub(super) fn validate_timing_artifact(linker_map: &Path) -> Result<(), String> {
    require_observation_symbols(linker_map, ObservationKind::Timing)
}

pub(super) fn execute_timing_sample(
    executable: &Path,
    working_directory: &Path,
    output: &Path,
    iteration: u32,
    expected_output_sha256: &str,
    expected_side_effects: ExpectedSideEffects,
) -> Result<u64, String> {
    let recorded = execute_observed(
        executable,
        working_directory,
        output,
        iteration,
        expected_output_sha256,
        expected_side_effects,
    )?;

    if recorded.storage != empty_storage() {
        return Err("timed performance execution unexpectedly recorded memory work".to_owned());
    }

    recorded.duration_nanoseconds.ok_or_else(|| {
        "timed performance execution did not record its controlled interval".to_owned()
    })
}

pub(super) fn measure_storage(
    executable: &Path,
    linker_map: &Path,
    working_directory: &Path,
    output: &Path,
    expected_output_sha256: &str,
    expected: StorageExpectation,
) -> Result<WorkloadObservations, String> {
    require_observation_symbols(linker_map, ObservationKind::Memory)?;

    let recorded = execute_observed(
        executable,
        working_directory,
        output,
        0,
        expected_output_sha256,
        ExpectedSideEffects::None,
    )?;

    if recorded.duration_nanoseconds.is_some() {
        return Err("memory observation execution unexpectedly recorded timing".to_owned());
    }

    if recorded.storage != expected {
        return Err(format!(
            "observed storage work differs from the corpus contract: expected {expected:?}, measured {:?}",
            recorded.storage
        ));
    }

    let measured = |value| Observation::Measured {
        value,
        scope: STORAGE_OBSERVATION_SCOPE.to_owned(),
    };

    Ok(WorkloadObservations {
        allocation_count: measured(recorded.storage.allocation_count),
        allocated_bytes: measured(recorded.storage.allocated_bytes),
        copied_bytes: measured(recorded.storage.copied_bytes),
        platform_operations: BTreeMap::new(),
    })
}

fn execute_observed(
    executable: &Path,
    working_directory: &Path,
    output: &Path,
    iteration: u32,
    expected_output_sha256: &str,
    expected_side_effects: ExpectedSideEffects,
) -> Result<RecordedExecution, String> {
    let observation_path = output.join(format!("performance-observations-{iteration}.bin"));

    let execution = super::execution::workload_command(executable, working_directory)
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

    super::validate_output(
        &execution,
        expected_output_sha256,
        expected_side_effects,
        working_directory,
    )?;

    let recorded = read(&observation_path)?;

    fs::remove_file(&observation_path).map_err(|error| {
        format!(
            "could not remove performance observation {}: {error}",
            observation_path.display()
        )
    })?;

    Ok(recorded)
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

    let bytes =
        fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))?;

    let Some(records) = bytes.strip_prefix(&bray_runtime_abi::PERFORMANCE_OBSERVATION_HEADER)
    else {
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
                return Err(
                    "performance observation stream repeats its controlled interval".to_owned(),
                );
            }
            kind => {
                return Err(format!(
                    "performance observation stream has unknown record {kind}"
                ));
            }
        }
    }

    if !chunks.remainder().is_empty() {
        return Err("performance observation stream ends with a partial record".to_owned());
    }

    Ok(RecordedExecution {
        duration_nanoseconds,
        storage,
    })
}

#[derive(Clone, Copy)]
enum ObservationKind {
    Timing,
    Memory,
}

fn require_observation_symbols(linker_map: &Path, kind: ObservationKind) -> Result<(), String> {
    let symbols = linked_symbols(linker_map)?;

    let required: &[&str] = match kind {
        ObservationKind::Timing => &[
            bray_runtime_abi::PERFORMANCE_INTERVAL_BEGIN_SYMBOL,
            bray_runtime_abi::PERFORMANCE_INTERVAL_END_SYMBOL,
        ],
        ObservationKind::Memory => &[bray_runtime_abi::MEMORY_OBSERVATION_BEGIN_SYMBOL],
    };

    for symbol in required {
        if !symbols.contains(symbol) {
            return Err(format!("observed performance artifact is missing {symbol}"));
        }
    }

    Ok(())
}

const fn empty_storage() -> StorageExpectation {
    StorageExpectation {
        allocation_count: 0,
        allocated_bytes: 0,
        copied_bytes: 0,
    }
}

fn linked_symbols(linker_map: &Path) -> Result<String, String> {
    fs::read_to_string(linker_map).map_err(|error| {
        format!(
            "could not read linker map {}: {error}",
            linker_map.display()
        )
    })
}

const fn observation_symbols() -> [&'static str; 5] {
    [
        bray_runtime_abi::MEMORY_OBSERVATION_BEGIN_SYMBOL,
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
        ALLOCATION_RECORD, CONTROLLED_DURATION_RECORD, COPY_RECORD, ObservationKind,
        RecordedExecution, read, require_observation_symbols,
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
                duration_nanoseconds: Some(50),
                storage: super::StorageExpectation {
                    allocation_count: 2,
                    allocated_bytes: 3,
                    copied_bytes: 1,
                },
            }
        );
    }

    #[test]
    fn fixed_records_reject_unknown_partial_and_repeated_intervals() {
        let cases = [
            vec![7; 9],
            vec![ALLOCATION_RECORD; 1],
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

    #[test]
    fn fixed_records_allow_timing_and_memory_to_be_observed_separately() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("observation directory must exist: {error}"));

        let path = directory.path().join("observations.bin");
        let mut bytes = bray_runtime_abi::PERFORMANCE_OBSERVATION_HEADER.to_vec();

        push(&mut bytes, ALLOCATION_RECORD, 8);

        fs::write(&path, bytes)
            .unwrap_or_else(|error| panic!("observation fixture must write: {error}"));

        assert_eq!(
            read(&path).unwrap_or_else(|error| panic!("observation must parse: {error}")),
            RecordedExecution {
                duration_nanoseconds: None,
                storage: super::StorageExpectation {
                    allocation_count: 1,
                    allocated_bytes: 8,
                    copied_bytes: 0,
                },
            }
        );
    }

    #[test]
    fn initialized_memory_session_can_record_zero_events() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("observation directory must exist: {error}"));

        let path = directory.path().join("observations.bin");

        fs::write(&path, bray_runtime_abi::PERFORMANCE_OBSERVATION_HEADER)
            .unwrap_or_else(|error| panic!("observation fixture must write: {error}"));

        assert_eq!(
            read(&path).unwrap_or_else(|error| panic!("observation must parse: {error}")),
            RecordedExecution {
                duration_nanoseconds: None,
                storage: super::empty_storage(),
            }
        );
    }

    #[test]
    fn memory_observation_requires_session_root_not_event_hooks() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("observation directory must exist: {error}"));

        let path = directory.path().join("application.map");

        fs::write(&path, bray_runtime_abi::MEMORY_OBSERVATION_BEGIN_SYMBOL)
            .unwrap_or_else(|error| panic!("linker map fixture must write: {error}"));

        require_observation_symbols(&path, ObservationKind::Memory)
            .unwrap_or_else(|error| panic!("memory observation root must validate: {error}"));

        fs::write(
            &path,
            bray_runtime_abi::MEMORY_ALLOCATION_OBSERVATION_SYMBOL,
        )
        .unwrap_or_else(|error| panic!("linker map fixture must write: {error}"));

        assert!(require_observation_symbols(&path, ObservationKind::Memory).is_err());
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
