use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;
use std::path::Path;
use std::time::Instant;

use super::super::corpus::{
    CALIBRATION_SAMPLE_COUNT, CALIBRATION_TARGET_NANOSECONDS, ExpectedSideEffects, Workload,
};
use super::super::model::{
    BRAY_EXECUTION_SCOPE, ExecutionStatistics, PROCESS_EXECUTION_SCOPE, PeerLanguage,
    WorkloadBatching,
};
use super::super::statistics;

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum ImplementationKey {
    Bray,
    Peer(PeerLanguage),
}

pub(super) struct ImplementationTarget<'a> {
    pub key: ImplementationKey,
    pub executable: &'a Path,
    pub timed_executable: &'a Path,
    pub timing_map: Option<&'a Path>,
}

struct ImplementationSamples {
    process: Vec<u64>,
    controlled: Vec<u64>,
}

pub(super) struct ImplementationExecution {
    pub process: ExecutionStatistics,
    pub controlled: ExecutionStatistics,
}

pub(super) fn calibrate_inner_iterations(
    implementations: &[ImplementationTarget<'_>],
    working_directory: &Path,
    workload: &Workload,
    expected_output_sha256: &str,
    seed_inner_iterations: NonZeroU64,
) -> Result<(NonZeroU64, WorkloadBatching), String> {
    require_calibration_implementations(implementations)?;

    let sample_capacity = usize::try_from(CALIBRATION_SAMPLE_COUNT)
        .map_err(|_| "calibration sample count cannot be represented by this host".to_owned())?;

    let mut samples = implementations
        .iter()
        .map(|implementation| (implementation.key, Vec::with_capacity(sample_capacity)))
        .collect::<BTreeMap<_, _>>();

    validate_timing_artifacts(implementations)?;

    for iteration in 0..CALIBRATION_SAMPLE_COUNT {
        let start = rotation_start(iteration, implementations.len(), 0);

        for offset in 0..implementations.len() {
            let implementation = &implementations[(start + offset) % implementations.len()];

            let elapsed = super::super::observation::execute_timing_sample(
                implementation.timed_executable,
                working_directory,
                working_directory,
                iteration,
                expected_output_sha256,
                workload.expected_side_effects,
            )?;

            samples
                .get_mut(&implementation.key)
                .ok_or_else(|| "calibration lost its implementation".to_owned())?
                .push(elapsed);
        }
    }

    calibrated_batch(samples, seed_inner_iterations)
}

fn require_calibration_implementations(
    implementations: &[ImplementationTarget<'_>],
) -> Result<(), String> {
    let expected = [
        ImplementationKey::Bray,
        ImplementationKey::Peer(PeerLanguage::Rust),
        ImplementationKey::Peer(PeerLanguage::Cpp),
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();

    let actual = implementations
        .iter()
        .map(|implementation| implementation.key)
        .collect::<BTreeSet<_>>();

    if actual != expected || actual.len() != implementations.len() {
        return Err("batch calibration requires Bray, Rust, and C++ exactly once".to_owned());
    }

    Ok(())
}

fn calibrated_batch(
    mut samples: BTreeMap<ImplementationKey, Vec<u64>>,
    seed_inner_iterations: NonZeroU64,
) -> Result<(NonZeroU64, WorkloadBatching), String> {
    let bray = samples
        .remove(&ImplementationKey::Bray)
        .ok_or_else(|| "calibration omitted Bray".to_owned())?;

    let rust = samples
        .remove(&ImplementationKey::Peer(PeerLanguage::Rust))
        .ok_or_else(|| "calibration omitted Rust".to_owned())?;

    let cpp = samples
        .remove(&ImplementationKey::Peer(PeerLanguage::Cpp))
        .ok_or_else(|| "calibration omitted C++".to_owned())?;

    let inner_iterations = statistics::calibrated_inner_iterations(
        seed_inner_iterations,
        CALIBRATION_TARGET_NANOSECONDS,
        [&bray[..], &rust[..], &cpp[..]],
    )?;

    let report = WorkloadBatching::Calibrated {
        seed_inner_iterations: seed_inner_iterations.get(),
        target_interval_nanoseconds: CALIBRATION_TARGET_NANOSECONDS,
        bray_samples_nanoseconds: bray,
        rust_samples_nanoseconds: rust,
        cpp_samples_nanoseconds: cpp,
        selected_inner_iterations: inner_iterations.get(),
    };

    Ok((inner_iterations, report))
}

#[expect(
    clippy::too_many_arguments,
    reason = "the interleaved run keeps its sample policy and validation contract explicit"
)]
pub(super) fn execute_interleaved(
    implementations: &[ImplementationTarget<'_>],
    working_directory: &Path,
    warmup: u32,
    samples: u32,
    workload: &Workload,
    expected_output_sha256: &str,
    timer_resolution_nanoseconds: u64,
    inner_iterations: NonZeroU64,
) -> Result<BTreeMap<ImplementationKey, ImplementationExecution>, String> {
    let sample_capacity = usize::try_from(samples)
        .map_err(|_| "sample count cannot be represented by this host".to_owned())?;

    if implementations.is_empty() {
        return Err("performance execution requires at least one implementation".to_owned());
    }

    validate_timing_artifacts(implementations)?;

    let mut measured = implementations
        .iter()
        .map(|implementation| {
            (
                implementation.key,
                ImplementationSamples {
                    process: Vec::with_capacity(sample_capacity),
                    controlled: Vec::with_capacity(sample_capacity),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    for iteration in 0..warmup.saturating_add(samples) {
        execute_process_round(
            implementations,
            working_directory,
            workload,
            expected_output_sha256,
            iteration,
            warmup,
            &mut measured,
        )?;

        execute_controlled_round(
            implementations,
            working_directory,
            workload,
            expected_output_sha256,
            iteration,
            warmup,
            &mut measured,
        )?;
    }

    summarize_implementations(
        measured,
        workload,
        inner_iterations,
        timer_resolution_nanoseconds,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "one process round requires the shared execution and sampling contract"
)]
fn execute_process_round(
    implementations: &[ImplementationTarget<'_>],
    working_directory: &Path,
    workload: &Workload,
    expected_output_sha256: &str,
    iteration: u32,
    warmup: u32,
    measured: &mut BTreeMap<ImplementationKey, ImplementationSamples>,
) -> Result<(), String> {
    let start = rotation_start(iteration, implementations.len(), 0);

    for offset in 0..implementations.len() {
        let implementation = &implementations[(start + offset) % implementations.len()];

        let elapsed = execute_process_sample(
            implementation.executable,
            working_directory,
            expected_output_sha256,
            workload.expected_side_effects,
        )?;

        if iteration >= warmup {
            measured
                .get_mut(&implementation.key)
                .ok_or_else(|| "interleaved process sample lost its implementation".to_owned())?
                .process
                .push(elapsed);
        }
    }

    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "one controlled round requires the shared execution and sampling contract"
)]
fn execute_controlled_round(
    implementations: &[ImplementationTarget<'_>],
    working_directory: &Path,
    workload: &Workload,
    expected_output_sha256: &str,
    iteration: u32,
    warmup: u32,
    measured: &mut BTreeMap<ImplementationKey, ImplementationSamples>,
) -> Result<(), String> {
    let start = rotation_start(iteration, implementations.len(), 1);

    for offset in 0..implementations.len() {
        let implementation = &implementations[(start + offset) % implementations.len()];

        let elapsed = super::super::observation::execute_timing_sample(
            implementation.timed_executable,
            working_directory,
            working_directory,
            iteration,
            expected_output_sha256,
            workload.expected_side_effects,
        )?;

        if iteration >= warmup {
            measured
                .get_mut(&implementation.key)
                .ok_or_else(|| "interleaved timing sample lost its implementation".to_owned())?
                .controlled
                .push(elapsed);
        }
    }

    Ok(())
}

fn summarize_implementations(
    measured: BTreeMap<ImplementationKey, ImplementationSamples>,
    workload: &Workload,
    inner_iterations: NonZeroU64,
    timer_resolution_nanoseconds: u64,
) -> Result<BTreeMap<ImplementationKey, ImplementationExecution>, String> {
    measured
        .into_iter()
        .map(|(key, samples)| {
            let process = statistics::summarize(
                samples.process,
                workload.scale,
                PROCESS_EXECUTION_SCOPE,
                1,
                timer_resolution_nanoseconds,
            )
            .ok_or_else(|| "at least one process execution sample is required".to_owned())?;

            let controlled = statistics::summarize(
                samples.controlled,
                workload.scale,
                BRAY_EXECUTION_SCOPE,
                inner_iterations.get(),
                timer_resolution_nanoseconds,
            )
            .ok_or_else(|| "at least one controlled execution sample is required".to_owned())?;

            Ok((
                key,
                ImplementationExecution {
                    process,
                    controlled,
                },
            ))
        })
        .collect()
}

fn validate_timing_artifacts(implementations: &[ImplementationTarget<'_>]) -> Result<(), String> {
    for implementation in implementations {
        if let Some(map) = implementation.timing_map {
            super::super::observation::validate_timing_artifact(map)?;
        }
    }

    Ok(())
}

fn rotation_start(iteration: u32, implementations: usize, phase_offset: usize) -> usize {
    usize::try_from(iteration)
        .unwrap_or(usize::MAX)
        .wrapping_add(phase_offset)
        .wrapping_rem(implementations)
}

fn execute_process_sample(
    executable: &Path,
    working_directory: &Path,
    expected_output_sha256: &str,
    expected_side_effects: ExpectedSideEffects,
) -> Result<u64, String> {
    let started = Instant::now();

    let output = super::super::execution::workload_command(executable, working_directory)
        .output()
        .map_err(|error| format!("could not execute {}: {error}", executable.display()))?;

    let elapsed = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);

    if !output.status.success() {
        return Err(format!(
            "{} exited unsuccessfully: {}",
            executable.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    super::validate_output(
        &output,
        expected_output_sha256,
        expected_side_effects,
        working_directory,
    )?;

    Ok(elapsed)
}

#[cfg(test)]
mod tests {
    use super::rotation_start;

    #[test]
    fn process_and_controlled_rounds_rotate_language_priority() {
        assert_eq!(
            (0..6)
                .map(|iteration| rotation_start(iteration, 3, 0))
                .collect::<Vec<_>>(),
            [0, 1, 2, 0, 1, 2]
        );

        assert_eq!(
            (0..6)
                .map(|iteration| rotation_start(iteration, 3, 1))
                .collect::<Vec<_>>(),
            [1, 2, 0, 1, 2, 0]
        );
    }
}
