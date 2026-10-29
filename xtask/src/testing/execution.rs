use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use subprocess::Exec;

use super::inventory::{SelectedTest, batches};
use super::report::{Recorder, TestTiming};
use crate::validation_process;

pub(super) struct Execution {
    pub(super) log: Vec<u8>,
    pub(super) tests: Vec<TestTiming>,
    pub(super) success: bool,
}

pub(super) fn run(
    root: &Path,
    directory: &Path,
    tests: &[SelectedTest],
    started: Instant,
    timeout: Duration,
) -> Execution {
    let batches = batches(tests);

    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(4)
        .min(batches.len());

    let next = AtomicUsize::new(0);

    let results = std::thread::scope(|scope| {
        let mut handles = Vec::new();

        for _ in 0..workers {
            let batches = &batches;
            let next = &next;

            handles.push(scope.spawn(move || {
                let mut results = Vec::new();

                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);

                    let Some(batch) = batches.get(index) else {
                        break;
                    };

                    let result = execute_batch(
                        root,
                        directory,
                        batch,
                        timeout.saturating_sub(started.elapsed()),
                    );

                    let success = result.success;

                    results.push((index, result));

                    if !success {
                        break;
                    }
                }

                results
            }));
        }

        handles
            .into_iter()
            .flat_map(|handle| {
                handle
                    .join()
                    .expect("compiler test batch worker must complete")
            })
            .collect::<Vec<_>>()
    });

    merge(results)
}

fn execute_batch(
    root: &Path,
    directory: &Path,
    batch: &[SelectedTest],
    remaining: Duration,
) -> Execution {
    let command = Exec::cmd(&batch[0].executable)
        .args(&[
            "--include-ignored",
            "--exact",
            "--test-threads=1",
            "--color=never",
        ])
        .args(
            &batch
                .iter()
                .map(|test| test.name.as_str())
                .collect::<Vec<_>>(),
        )
        .cwd(root.join("crates/bray-compilation"))
        .env("BRAY_TEST_PROFILE_OUTPUT", directory.join("phases"));

    let mut recorder = Recorder::new(batch);

    let result = validation_process::output_observed(command, remaining, &mut |bytes| {
        recorder.observe(bytes)
    });

    match result {
        Ok(output) => {
            let mut log = output.stdout;

            log.extend_from_slice(&output.stderr);

            Execution {
                log,
                success: output.status.success() && recorder.complete(),
                tests: recorder.tests,
            }
        }
        Err(error) => Execution {
            log: error.to_string().into_bytes(),
            success: false,
            tests: recorder.tests,
        },
    }
}

fn merge(mut results: Vec<(usize, Execution)>) -> Execution {
    results.sort_by_key(|(index, _)| *index);

    let mut combined = Execution {
        log: Vec::new(),
        tests: Vec::new(),
        success: true,
    };

    for (_, result) in results {
        combined.log.extend(result.log);
        combined.tests.extend(result.tests);
        combined.success &= result.success;
    }

    combined
}

#[cfg(test)]
mod tests {
    use super::{Execution, merge};

    #[test]
    fn completed_lanes_keep_inventory_order_and_any_failure() {
        let result = merge(vec![
            (
                2,
                Execution {
                    log: b"third".to_vec(),
                    tests: Vec::new(),
                    success: true,
                },
            ),
            (
                0,
                Execution {
                    log: b"first".to_vec(),
                    tests: Vec::new(),
                    success: true,
                },
            ),
            (
                1,
                Execution {
                    log: b"second".to_vec(),
                    tests: Vec::new(),
                    success: false,
                },
            ),
        ]);

        assert_eq!(result.log, b"firstsecondthird");
        assert!(!result.success);
    }
}
