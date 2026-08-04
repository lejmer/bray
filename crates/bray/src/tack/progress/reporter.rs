use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Instant;

use bray_messages::{BuildProgressLineKind, BuildProgressMessageRenderer, BuildProgressOperation};
use bray_tooling::{OutputFormat, write_diagnostic_groups};

use super::model::{
    BuildProgressPackage, BuildProgressPlan, BuildProgressReport, BuildProgressStatus,
    PackageProgressReport,
};
use super::presentation::{duration_milliseconds, max_column_width};
use super::terminal::TerminalBuildProgress;
use super::text::{message_action, message_configuration, render_line};
use crate::tack::result::TackRunResult;

pub(crate) struct WorkflowProgress {
    interactive: bool,
    verbose: bool,
    reports: Mutex<Vec<BuildProgressReport>>,
}

impl WorkflowProgress {
    pub(crate) const fn new(interactive: bool, verbose: bool) -> Self {
        Self {
            interactive,
            verbose,
            reports: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn begin(&self, plan: BuildProgressPlan) -> BuildProgressSession<'_> {
        let terminal = self
            .interactive
            .then(|| TerminalBuildProgress::new(&plan, self.verbose));

        BuildProgressSession {
            workflow: self,
            plan,
            terminal,
            started_at: Instant::now(),
            package_states: Mutex::new(BTreeMap::new()),
            package_reports: Mutex::new(Vec::new()),
            completed: false,
        }
    }

    pub(crate) const fn interactive(&self) -> bool {
        self.interactive
    }

    pub(crate) fn write_to_result(&self, result: &mut TackRunResult) -> Result<(), ()> {
        let reports = self
            .reports
            .lock()
            .unwrap_or_else(|error| error.into_inner());

        if reports.is_empty() {
            return Ok(());
        }

        match result.output_format() {
            OutputFormat::Text if self.interactive => Ok(()),
            OutputFormat::Text => {
                result.prepend_stderr(&render_plain(&reports, self.verbose));

                Ok(())
            }
            OutputFormat::Json => write_json_report(result, &reports),
        }
    }

    fn push(&self, report: BuildProgressReport) {
        self.reports
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(report);
    }
}

pub(crate) struct BuildProgressSession<'workflow> {
    workflow: &'workflow WorkflowProgress,
    plan: BuildProgressPlan,
    terminal: Option<TerminalBuildProgress>,
    started_at: Instant,
    package_states: Mutex<BTreeMap<String, PackageProgressState>>,
    package_reports: Mutex<Vec<PackageProgressReport>>,
    completed: bool,
}

impl BuildProgressSession<'_> {
    pub(crate) fn start_package(&self, identity: &str) {
        let Some(package) = self.package(identity) else {
            return;
        };

        self.package_states
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .entry(package.identity().to_owned())
            .or_insert_with(PackageProgressState::started);

        if let Some(terminal) = &self.terminal {
            terminal.start_package(identity);
        }
    }

    pub(crate) fn finish_package_work(&self, identity: &str, units: u64, success: bool) {
        let Some(package) = self.package(identity) else {
            return;
        };

        let (completed_units, total_completed_units, duration, finalized) = {
            let mut states = self
                .package_states
                .lock()
                .unwrap_or_else(|error| error.into_inner());

            let state = states
                .entry(identity.to_owned())
                .or_insert_with(PackageProgressState::started);

            if state.finished {
                return;
            }

            if success {
                state.completed_units = state
                    .completed_units
                    .saturating_add(units)
                    .min(package.units());
            }

            let finalized = !success || state.completed_units == package.units();

            if finalized {
                state.finished = true;
            }

            let completed_units = state.completed_units;
            let duration = state.started_at.elapsed();

            let total_completed_units = states.values().map(|state| state.completed_units).sum();

            (completed_units, total_completed_units, duration, finalized)
        };

        if let Some(terminal) = &self.terminal {
            terminal.set_package_units(identity, completed_units);
            terminal.set_completed_units(total_completed_units);
        }

        if !finalized {
            return;
        }

        let status = progress_status(success);

        let report = PackageProgressReport::new(
            package,
            status,
            completed_units,
            duration_milliseconds(duration),
        );

        self.package_reports
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(report);

        if let Some(terminal) = &self.terminal {
            terminal.finish_package(identity, status);
        }
    }

    pub(crate) fn finish(mut self, success: bool) {
        self.complete(success);
    }

    fn complete(&mut self, success: bool) {
        if self.completed {
            return;
        }

        let status = progress_status(success);

        let mut package_reports = std::mem::take(
            &mut *self
                .package_reports
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        );

        package_reports.sort_by_key(|report| {
            self.plan
                .packages()
                .iter()
                .position(|package| package.identity() == report.package())
                .unwrap_or(usize::MAX)
        });

        let report = BuildProgressReport::new(
            &self.plan,
            status,
            duration_milliseconds(self.started_at.elapsed()),
            package_reports,
        );

        if let Some(terminal) = &self.terminal {
            terminal.finish(&self.plan, status);
        }

        self.workflow.push(report);
        self.completed = true;
    }

    fn package(&self, identity: &str) -> Option<&BuildProgressPackage> {
        self.plan
            .packages()
            .iter()
            .find(|package| package.identity() == identity)
    }
}

struct PackageProgressState {
    started_at: Instant,
    completed_units: u64,
    finished: bool,
}

impl PackageProgressState {
    fn started() -> Self {
        Self {
            started_at: Instant::now(),
            completed_units: 0,
            finished: false,
        }
    }
}

impl Drop for BuildProgressSession<'_> {
    fn drop(&mut self) {
        self.complete(false);
    }
}

fn render_plain(reports: &[BuildProgressReport], verbose: bool) -> String {
    let messages = BuildProgressMessageRenderer::english();
    let mut output = String::new();

    for report in reports {
        output.push_str(&messages.heading(
            report.product(),
            message_configuration(report.configuration()),
        ));

        output.push('\n');

        let subject_width = max_column_width(
            report
                .packages()
                .iter()
                .map(PackageProgressReport::package)
                .chain(std::iter::once(report.artifact())),
        );

        let path_width = max_column_width(
            report
                .packages()
                .iter()
                .map(PackageProgressReport::path)
                .chain(std::iter::once(report.output_path())),
        );

        for package in report.packages() {
            let operation = match package.status() {
                BuildProgressStatus::Complete => BuildProgressOperation::Compiled,
                BuildProgressStatus::Failed => BuildProgressOperation::Failed,
            };

            let subject = format!("{:<subject_width$}", package.package());
            let path = format!("{:<path_width$}", package.path());

            let line = render_line(
                messages,
                BuildProgressLineKind::Package,
                package.status(),
                operation,
                &subject,
                &path,
                package.completed_units(),
                package.total_units(),
                u128::from(package.duration_milliseconds()),
            );

            output.push_str(&line);
            output.push('\n');

            if verbose {
                output.push_str(&format!(
                    "      {}\n",
                    messages.action(message_action(package.action()))
                ));
            }
        }

        let operation = match report.status() {
            BuildProgressStatus::Complete => BuildProgressOperation::Finished,
            BuildProgressStatus::Failed => BuildProgressOperation::Failed,
        };

        let subject = format!("{:<subject_width$}", report.artifact());
        let path = format!("{:<path_width$}", report.output_path());

        let line = render_line(
            messages,
            BuildProgressLineKind::FinishedProduct,
            report.status(),
            operation,
            &subject,
            &path,
            report.completed_units(),
            report.total_units(),
            u128::from(report.duration_milliseconds()),
        );

        output.push_str(&line);
        output.push('\n');
    }

    output
}

fn write_json_report(
    result: &mut TackRunResult,
    reports: &[BuildProgressReport],
) -> Result<(), ()> {
    let mut diagnostic_report = if result.diagnostics().is_empty() {
        None
    } else {
        let mut output = Vec::new();
        let mut error = Vec::new();

        write_diagnostic_groups(
            result.diagnostic_groups(),
            OutputFormat::Json,
            &mut output,
            &mut error,
        )
        .map_err(|_| ())?;

        Some(serde_json::from_slice::<serde_json::Value>(&output).map_err(|_| ())?)
    };

    let has_child_report = !result.stdout().is_empty();

    let mut report = if has_child_report {
        serde_json::from_str::<serde_json::Value>(result.stdout()).map_err(|_| ())?
    } else {
        diagnostic_report.take().unwrap_or_else(|| {
            serde_json::json!({
                "has_errors": false,
                "diagnostics": [],
            })
        })
    };

    let Some(report) = report.as_object_mut() else {
        return Err(());
    };

    if has_child_report {
        merge_diagnostic_report(report, diagnostic_report.as_ref())?;
    }

    let workflow = serde_json::to_value(reports).map_err(|_| ())?;

    report.insert(String::from("build_progress"), workflow);

    let output = serde_json::to_string_pretty(report).map_err(|_| ())?;

    result.replace_stdout(format!("{output}\n"));
    result.clear_diagnostics();

    Ok(())
}

fn merge_diagnostic_report(
    report: &mut serde_json::Map<String, serde_json::Value>,
    additional: Option<&serde_json::Value>,
) -> Result<(), ()> {
    let Some(additional) = additional else {
        return Ok(());
    };

    let additional_has_errors = additional
        .get("has_errors")
        .and_then(serde_json::Value::as_bool)
        .ok_or(())?;

    let additional_diagnostics = additional
        .get("diagnostics")
        .and_then(serde_json::Value::as_array)
        .ok_or(())?;

    let diagnostics = report
        .get_mut("diagnostics")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or(())?;

    diagnostics.extend(additional_diagnostics.iter().cloned());

    if additional_has_errors {
        report.insert(String::from("has_errors"), serde_json::Value::Bool(true));
    }

    Ok(())
}

const fn progress_status(success: bool) -> BuildProgressStatus {
    if success {
        BuildProgressStatus::Complete
    } else {
        BuildProgressStatus::Failed
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticBag;
    use bray_tooling::OutputFormat;

    use super::WorkflowProgress;
    use crate::tack::model::TackBuildConfiguration;
    use crate::tack::progress::{BuildProgressAction, BuildProgressPackage, BuildProgressPlan};
    use crate::tack::result::TackRunResult;

    #[test]
    fn noninteractive_progress_keeps_one_summary_line_per_package() {
        let progress = WorkflowProgress::new(false, false);
        let session = progress.begin(plan());

        session.start_package("std");
        session.finish_package_work("std", 5, true);
        session.finish_package_work("std", 7, true);
        session.start_package("hello_world");
        session.finish_package_work("hello_world", 1, true);
        session.finish_package_work("hello_world", 2, true);
        session.finish(true);

        let mut result = TackRunResult::new(
            std::process::ExitCode::SUCCESS,
            DiagnosticBag::new(),
            OutputFormat::Text,
        );

        assert_eq!(progress.write_to_result(&mut result), Ok(()));

        let lines = result.stderr().lines().collect::<Vec<_>>();

        assert_eq!(lines.len(), 4, "{:#?}", result.stderr());
        assert!(lines[0].starts_with("Building hello_world/application [debug]"));
        assert!(lines[1].starts_with("   ✓ Compiled std"));
        assert!(lines[2].starts_with("   ✓ Compiled hello_world"));
        assert!(lines[3].starts_with("   ✓ Finished application.exe"));
    }

    #[test]
    fn noninteractive_verbose_progress_retains_compiler_action_detail() {
        let progress = WorkflowProgress::new(false, true);
        let session = progress.begin(plan());

        session.start_package("std");
        session.finish_package_work("std", 12, true);
        session.finish(true);

        let mut result = TackRunResult::new(
            std::process::ExitCode::SUCCESS,
            DiagnosticBag::new(),
            OutputFormat::Text,
        );

        assert_eq!(progress.write_to_result(&mut result), Ok(()));
        assert!(result.stderr().contains("Checking dependency interface"));
    }

    #[test]
    fn json_progress_remains_structured_and_locale_neutral() {
        let progress = WorkflowProgress::new(false, false);
        let session = progress.begin(plan());

        session.start_package("std");
        session.finish_package_work("std", 12, true);
        session.finish(false);

        let mut result = TackRunResult::new(
            std::process::ExitCode::FAILURE,
            DiagnosticBag::new(),
            OutputFormat::Json,
        );

        assert_eq!(progress.write_to_result(&mut result), Ok(()));

        let report: serde_json::Value = serde_json::from_str(result.stdout())
            .unwrap_or_else(|error| panic!("progress JSON must parse: {error:?}"));

        assert_eq!(report["build_progress"][0]["status"], "failed");
        assert_eq!(report["build_progress"][0]["packages"][0]["package"], "std");
        assert!(result.stdout().find("Compiling").is_none());
    }

    fn plan() -> BuildProgressPlan {
        BuildProgressPlan::new(
            "hello_world/application",
            TackBuildConfiguration::Debug,
            "application.exe",
            "build/native/debug",
            vec![
                BuildProgressPackage::new(
                    "std",
                    "toolchain/standard-library",
                    12,
                    BuildProgressAction::CheckInterface,
                ),
                BuildProgressPackage::new(
                    "hello_world",
                    "examples/hello_world",
                    3,
                    BuildProgressAction::ProduceArtifacts,
                ),
            ],
        )
    }
}
