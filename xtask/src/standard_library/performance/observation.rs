use std::fs;
use std::path::Path;
use std::process::Command;

use bray_base::lowercase_hex;
use sha2::{Digest as _, Sha256};

use super::corpus::StorageExpectation;
use super::model::{Observation, WorkloadObservations};

const RECORD_BYTES: usize = 9;
const ALLOCATION_RECORD: u8 = 1;
const COPY_RECORD: u8 = 2;
const SCOPE: &str = "generated memory operations in one dedicated observed execution";

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

pub(super) fn measure(
    executable: &Path,
    linker_map: &Path,
    working_directory: &Path,
    output: &Path,
    expected_output_sha256: &str,
    expected: StorageExpectation,
) -> Result<WorkloadObservations, String> {
    require_observation_symbols(linker_map)?;

    let output = output.join("memory-observations.bin");

    let execution = Command::new(executable)
        .current_dir(working_directory)
        .env(bray_runtime_abi::MEMORY_OBSERVATION_PATH_ENVIRONMENT, &output)
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

    let measured = read(&output)?;

    if measured != expected {
        return Err(format!(
            "observed storage work differs from the corpus contract: expected {expected:?}, measured {measured:?}"
        ));
    }

    let observation = |value| Observation::Measured {
        value,
        scope: SCOPE.to_owned(),
    };

    Ok(WorkloadObservations {
        allocation_count: observation(measured.allocation_count),
        allocated_bytes: observation(measured.allocated_bytes),
        copied_bytes: observation(measured.copied_bytes),
        platform_operations: Default::default(),
    })
}

fn read(path: &Path) -> Result<StorageExpectation, String> {
    let maximum = bray_runtime_abi::MAX_MEMORY_OBSERVATION_RECORDS
        .checked_mul(RECORD_BYTES as u64)
        .and_then(|bytes| bytes.checked_add(bray_runtime_abi::MEMORY_OBSERVATION_HEADER.len() as u64))
        .ok_or_else(|| "memory observation size bound overflowed".to_owned())?;

    let metadata = fs::metadata(path)
        .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;

    if metadata.len() > maximum {
        return Err("memory observation stream exceeds its record bound".to_owned());
    }

    let bytes = fs::read(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;

    let Some(records) = bytes.strip_prefix(&bray_runtime_abi::MEMORY_OBSERVATION_HEADER) else {
        return Err("memory observation stream has an unsupported header".to_owned());
    };

    let mut chunks = records.chunks_exact(RECORD_BYTES);

    let mut measured = StorageExpectation {
        allocation_count: 0,
        allocated_bytes: 0,
        copied_bytes: 0,
    };

    for record in &mut chunks {
        let value = u64::from_le_bytes(
            record[1..]
                .try_into()
                .map_err(|_| "memory observation record is truncated".to_owned())?,
        );

        match record[0] {
            ALLOCATION_RECORD => {
                measured.allocation_count = measured
                    .allocation_count
                    .checked_add(1)
                    .ok_or_else(|| "memory allocation observation count overflowed".to_owned())?;

                measured.allocated_bytes = measured
                    .allocated_bytes
                    .checked_add(value)
                    .ok_or_else(|| "memory allocation byte observation overflowed".to_owned())?;
            }
            COPY_RECORD => {
                measured.copied_bytes = measured
                    .copied_bytes
                    .checked_add(value)
                    .ok_or_else(|| "memory copy byte observation overflowed".to_owned())?;
            }
            kind => return Err(format!("memory observation stream has unknown record {kind}")),
        }
    }

    if !chunks.remainder().is_empty() {
        return Err("memory observation stream ends with a partial record".to_owned());
    }

    Ok(measured)
}

fn require_observation_symbols(linker_map: &Path) -> Result<(), String> {
    let symbols = linked_symbols(linker_map)?;

    for symbol in observation_symbols() {
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

const fn observation_symbols() -> [&'static str; 2] {
    [
        bray_runtime_abi::MEMORY_ALLOCATION_OBSERVATION_SYMBOL,
        bray_runtime_abi::MEMORY_COPY_OBSERVATION_SYMBOL,
    ]
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{ALLOCATION_RECORD, COPY_RECORD, read};

    #[test]
    fn fixed_records_preserve_measured_storage_work() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("observation directory must exist: {error}"));

        let path = directory.path().join("observations.bin");
        let mut bytes = bray_runtime_abi::MEMORY_OBSERVATION_HEADER.to_vec();

        push(&mut bytes, ALLOCATION_RECORD, 1);
        push(&mut bytes, ALLOCATION_RECORD, 2);
        push(&mut bytes, COPY_RECORD, 1);

        fs::write(&path, bytes)
            .unwrap_or_else(|error| panic!("observation fixture must write: {error}"));

        assert_eq!(
            read(&path).unwrap_or_else(|error| panic!("observation must parse: {error}")),
            super::StorageExpectation {
                allocation_count: 2,
                allocated_bytes: 3,
                copied_bytes: 1,
            }
        );
    }

    #[test]
    fn fixed_records_reject_unknown_and_partial_work() {
        for records in [vec![7; 9], vec![ALLOCATION_RECORD; 1]] {
            let directory = tempfile::tempdir()
                .unwrap_or_else(|error| panic!("observation directory must exist: {error}"));

            let path = directory.path().join("observations.bin");
            let mut bytes = bray_runtime_abi::MEMORY_OBSERVATION_HEADER.to_vec();

            bytes.extend(records);

            fs::write(&path, bytes)
                .unwrap_or_else(|error| panic!("observation fixture must write: {error}"));

            assert!(read(&path).is_err());
        }
    }

    fn push(bytes: &mut Vec<u8>, kind: u8, value: u64) {
        bytes.push(kind);
        bytes.extend(value.to_le_bytes());
    }
}
