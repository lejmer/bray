use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use subprocess::Exec;

use super::inventory::{batches, inventory, select};
use super::options::Options;
use super::report::{Recorder, Report, write};
use crate::{json, validation_process, workspace};

pub(crate) fn run(arguments: impl Iterator<Item = String>) -> ExitCode {
    let result = Options::parse(arguments).and_then(|options| {
        let root = workspace::root()?;

        execute(&root, &options)
    });

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");

            ExitCode::FAILURE
        }
    }
}

fn execute(root: &Path, options: &Options) -> Result<(), String> {
    let started = Instant::now();
    let tests = select(inventory(root)?, options);

    let directory = workspace::cargo_target(root)
        .join("compiler-tests")
        .join(options.tier.name())
        .join(format!("{}-of-{}", options.shard, options.shards));

    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("could not create {}: {error}", directory.display()))?;

    json::write_pretty(&directory.join("selection.json"), &tests)?;

    if tests.is_empty() {
        return Err("compiler test selection is empty, reduce the shard count".to_owned());
    }

    let preparation_seconds = started.elapsed().as_secs_f64();
    let execution_started = Instant::now();
    let budget = options.tier.budget();

    let mut report = Report {
        schema: 1,
        tier: options.tier.name().to_owned(),
        shard: options.shard,
        shards: options.shards,
        preparation_seconds,
        execution_seconds: 0.0,
        budget_seconds: budget.as_secs_f64(),
        selected: tests.len(),
        success: true,
        tests: Vec::new(),
    };

    let mut log = Vec::new();

    eprintln!(
        "Running {} compiler tests, tier {}, shard {}/{}, budget {}s",
        tests.len(),
        options.tier.name(),
        options.shard,
        options.shards,
        budget.as_secs()
    );

    for batch in batches(&tests) {
        let remaining = budget.saturating_sub(execution_started.elapsed());

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
                report.success &= output.status.success() && recorder.complete();
                log.extend_from_slice(&output.stdout);
                log.extend_from_slice(&output.stderr);
            }
            Err(error) => {
                report.success = false;
                log.extend_from_slice(error.to_string().as_bytes());
            }
        }

        report.tests.extend(recorder.tests);

        if !report.success {
            break;
        }
    }

    report.execution_seconds = execution_started.elapsed().as_secs_f64();

    report.success &=
        report.execution_seconds <= budget.as_secs_f64() && report.tests.len() == tests.len();

    std::fs::write(directory.join("run.log"), &log)
        .map_err(|error| format!("could not write compiler test log: {error}"))?;

    eprint!("{}", String::from_utf8_lossy(&log));

    let success = report.success;

    write(&directory, report)?;
    eprintln!("Compiler test report: {}", directory.display());

    if !success {
        return Err("compiler test tier failed or exceeded its budget, see run.log".to_owned());
    }

    Ok(())
}
