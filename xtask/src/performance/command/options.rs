use std::collections::BTreeSet;
use std::path::PathBuf;

use bray_target::NativeTarget;

use super::super::corpus::WORKLOADS;
use super::super::model::MAX_SAMPLE_COUNT;

pub(super) struct Options {
    pub output: PathBuf,
    pub baseline: Option<PathBuf>,
    pub target: NativeTarget,
    pub warmup: u32,
    pub samples: u32,
    pub workloads: BTreeSet<String>,
}

impl Options {
    pub fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut output = None;
        let mut baseline = None;
        let mut target = None;
        let mut warmup = 2;
        let mut samples = 7;
        let mut workloads = BTreeSet::new();

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--output" if output.is_none() => output = Some(path(&mut arguments, "--output")?),
                "--baseline" if baseline.is_none() => {
                    baseline = Some(path(&mut arguments, "--baseline")?);
                }
                "--target" if target.is_none() => {
                    let value = value(&mut arguments, "--target")?;

                    target = bray_target::TargetIdentity::try_new(value.as_str())
                        .and_then(|identity| NativeTarget::for_identity(&identity));

                    if target.is_none() {
                        return Err(format!("unsupported native target: {value}"));
                    }
                }
                "--warmup" => warmup = count(&mut arguments, "--warmup")?,
                "--samples" => samples = count(&mut arguments, "--samples")?,
                "--workload" => {
                    let workload = value(&mut arguments, "--workload")?;

                    if !WORKLOADS.iter().any(|candidate| candidate.id == workload) {
                        return Err(format!("unknown workload: {workload}"));
                    }

                    workloads.insert(workload);
                }
                _ => return Err(format!("unexpected or duplicate argument: {argument}")),
            }
        }

        let output = output.ok_or_else(|| "missing --output".to_owned())?;

        let host = NativeTarget::current()
            .ok_or_else(|| "the compiler host does not select a native target".to_owned())?;

        let target = target.unwrap_or(host);

        if target != host {
            return Err(format!(
                "performance workloads must target the current host ({})",
                host.as_str()
            ));
        }

        Ok(Self {
            output,
            baseline,
            target,
            warmup,
            samples,
            workloads,
        })
    }
}

fn path(
    arguments: &mut impl Iterator<Item = String>,
    option: &'static str,
) -> Result<PathBuf, String> {
    value(arguments, option).map(PathBuf::from)
}

fn value(
    arguments: &mut impl Iterator<Item = String>,
    option: &'static str,
) -> Result<String, String> {
    arguments
        .next()
        .ok_or_else(|| format!("missing value for {option}"))
}

fn count(
    arguments: &mut impl Iterator<Item = String>,
    option: &'static str,
) -> Result<u32, String> {
    let value = value(arguments, option)?;

    let count = value
        .parse()
        .map_err(|_| format!("invalid count for {option}: {value}"))?;

    if count == 0 {
        return Err(format!("{option} must be positive"));
    }

    if count > MAX_SAMPLE_COUNT {
        return Err(format!(
            "{option} exceeds the report bound of {MAX_SAMPLE_COUNT}"
        ));
    }

    Ok(count)
}

#[cfg(test)]
pub(in crate::performance) fn parse_for_test(
    arguments: &[&str],
) -> Result<(), String> {
    Options::parse(arguments.iter().map(|argument| (*argument).to_owned())).map(|_| ())
}
