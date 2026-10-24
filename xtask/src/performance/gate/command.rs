use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use serde::de::DeserializeOwned;

use super::super::command::read_report;
use super::super::model::PerformanceReport;
use super::model::{Checkpoint, Policy};

const USAGE: &str = "usage: cargo xtask performance gate --base-ref <ref> --baseline <report.json> --candidate <report.json> --output <directory> [--confirmation-baseline <report.json> --confirmation-candidate <report.json>] [--checkpoint <checkpoint.json>]";

pub(in crate::performance) fn run(arguments: impl Iterator<Item = String>) -> ExitCode {
    let result = crate::workspace::root().and_then(|root| execute(&root, arguments));

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");

            ExitCode::FAILURE
        }
    }
}

pub(super) fn execute(root: &Path, arguments: impl Iterator<Item = String>) -> Result<(), String> {
    let mut options = parse(arguments)?;
    let base_ref = required(&mut options, "--base-ref")?;
    let candidate_revision = git(root, &["rev-parse", "HEAD"])?;

    let base_revision = git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{base_ref}^{{commit}}"),
        ],
    )?;

    let merge_base = git(
        root,
        &["merge-base", "--all", &base_revision, &candidate_revision],
    )?;

    if !git(
        root,
        &["status", "--porcelain=v1", "--untracked-files=normal"],
    )?
    .is_empty()
    {
        return Err(
            "performance gating requires a clean candidate checkout and committed policy"
                .to_owned(),
        );
    }

    if merge_base.lines().count() != 1 || merge_base.len() != 40 {
        return Err("performance gating requires one unambiguous merge-base commit".to_owned());
    }

    let baseline: PerformanceReport = read(&required(&mut options, "--baseline")?)?;
    let candidate: PerformanceReport = read(&required(&mut options, "--candidate")?)?;
    let output = PathBuf::from(required(&mut options, "--output")?);

    if output.exists() {
        return Err(format!("gate output already exists: {}", output.display()));
    }

    let confirmation = match (
        options.remove("--confirmation-baseline"),
        options.remove("--confirmation-candidate"),
    ) {
        (None, None) => None,
        (Some(baseline), Some(candidate)) => Some((
            read::<PerformanceReport>(&baseline)?,
            read::<PerformanceReport>(&candidate)?,
        )),
        _ => return Err("confirmation requires both baseline and candidate reports".to_owned()),
    };

    let checkpoint = options
        .remove("--checkpoint")
        .map(|path| read::<Checkpoint>(&path))
        .transpose()?;

    let policy: Policy = read(root.join("xtask/performance-policy.json"))?;

    let mut report = super::evaluation::evaluate(
        &baseline,
        &candidate,
        confirmation
            .as_ref()
            .map(|(baseline, candidate)| (baseline, candidate)),
        policy,
        &merge_base,
        &candidate_revision,
        checkpoint,
    )?;

    super::waivers::apply(
        &mut report,
        super::waivers::today()?,
        super::waivers::live_issue_is_open,
    )?;

    std::fs::create_dir_all(&output)
        .map_err(|error| format!("could not create gate output {}: {error}", output.display()))?;

    crate::json::write_pretty(&output.join("gate.json"), &report)?;
    crate::json::write_pretty(&output.join("baseline.json"), &baseline)?;
    crate::json::write_pretty(&output.join("candidate.json"), &candidate)?;
    super::super::report::write_candidate(&output.join("baseline.html"), &baseline)?;
    super::super::report::write_candidate(&output.join("candidate.html"), &candidate)?;

    if let Some((baseline, candidate)) = &confirmation {
        crate::json::write_pretty(&output.join("confirmation-baseline.json"), baseline)?;
        crate::json::write_pretty(&output.join("confirmation-candidate.json"), candidate)?;
    }

    crate::json::write_pretty(&output.join("comparison.json"), &report.comparison)?;
    super::super::report::write_comparison(&output.join("comparison.html"), &report.comparison)?;
    super::presentation::write(&output.join("gate.html"), &report)?;

    println!("{}", output.join("gate.json").display());

    if report.blocking {
        return Err("performance regression gate failed, see gate.html and gate.json".to_owned());
    }

    eprintln!(
        "Performance regression state: {:?}. Review-required evidence needs human assessment. Absolute competitive targets remain separate.",
        report.state
    );

    Ok(())
}

pub(super) fn parse(
    arguments: impl Iterator<Item = String>,
) -> Result<BTreeMap<String, String>, String> {
    let mut arguments = arguments;
    let mut options = BTreeMap::new();

    while let Some(option) = arguments.next() {
        if !matches!(
            option.as_str(),
            "--base-ref"
                | "--baseline"
                | "--candidate"
                | "--output"
                | "--confirmation-baseline"
                | "--confirmation-candidate"
                | "--checkpoint"
        ) {
            return Err(format!("unexpected gate option {option}. {USAGE}"));
        }

        let value = arguments
            .next()
            .filter(|value| !value.starts_with("--") && !value.trim().is_empty())
            .ok_or_else(|| format!("missing value for {option}. {USAGE}"))?;

        if options.insert(option.clone(), value).is_some() {
            return Err(format!("duplicate gate option {option}"));
        }
    }

    Ok(options)
}

fn required(options: &mut BTreeMap<String, String>, name: &str) -> Result<String, String> {
    options
        .remove(name)
        .ok_or_else(|| format!("missing {name}. {USAGE}"))
}

fn read<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T, String> {
    let path = path.as_ref();
    let bytes = read_report(path)?;

    serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid performance input {}: {error}", path.display()))
}

pub(super) fn git(root: &Path, arguments: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(arguments)
        .output()
        .map_err(|error| format!("could not resolve performance Git identity: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "could not resolve performance Git identity: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
