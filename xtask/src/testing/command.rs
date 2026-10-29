use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use super::inventory::{inventory, select};
use super::options::Options;
use super::report::{Report, write};
use crate::{json, workspace};

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
    let reference_target = options.tier.reference_target();

    let mut report = Report {
        schema: 1,
        tier: options.tier.name().to_owned(),
        shard: options.shard,
        shards: options.shards,
        preparation_seconds,
        execution_seconds: 0.0,
        reference_target_seconds: reference_target.as_secs_f64(),
        reference_target_exceeded: false,
        timeout_seconds: options.timeout.as_secs_f64(),
        selected: tests.len(),
        success: true,
        tests: Vec::new(),
    };

    eprintln!(
        "Running {} compiler tests, tier {}, shard {}/{}, reference target {}s, timeout {}s",
        tests.len(),
        options.tier.name(),
        options.shard,
        options.shards,
        reference_target.as_secs(),
        options.timeout.as_secs()
    );

    let result =
        super::execution::run(root, &directory, &tests, execution_started, options.timeout);

    report.success = result.success;
    report.tests = result.tests;

    report.finish(execution_started.elapsed());

    if report.reference_target_exceeded {
        eprintln!(
            "Reference runtime target exceeded: {:.3}s, target {}s. This timing observation does not fail the suite.",
            report.execution_seconds,
            reference_target.as_secs()
        );
    }

    std::fs::write(directory.join("run.log"), &result.log)
        .map_err(|error| format!("could not write compiler test log: {error}"))?;

    eprint!("{}", String::from_utf8_lossy(&result.log));

    let success = report.success;

    write(&directory, report)?;
    eprintln!("Compiler test report: {}", directory.display());

    if !success {
        return Err("compiler test tier failed or exceeded its timeout, see run.log".to_owned());
    }

    Ok(())
}
