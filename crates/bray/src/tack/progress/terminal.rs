use std::collections::BTreeMap;
use std::time::Duration;

use bray_messages::{BuildProgressLineKind, BuildProgressMessageRenderer, BuildProgressOperation};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use super::model::{BuildProgressPackage, BuildProgressPlan, BuildProgressStatus};
use super::presentation::{
    ProgressVisualState, max_column_width, padded, terminal_heading, terminal_line_style,
    terminal_progress,
};
use super::text::{message_action, message_configuration};

pub(super) struct TerminalBuildProgress {
    progress: MultiProgress,
    heading: ProgressBar,
    aggregate: ProgressBar,
    packages: BTreeMap<String, PackageTerminalProgress>,
    subject_column_width: usize,
    path_column_width: usize,
    messages: BuildProgressMessageRenderer,
    verbose: bool,
}

impl TerminalBuildProgress {
    pub(super) fn new(plan: &BuildProgressPlan, verbose: bool) -> Self {
        let messages = BuildProgressMessageRenderer::english();
        let progress = terminal_progress();

        let subject_column_width = max_column_width(
            plan.packages()
                .iter()
                .map(BuildProgressPackage::identity)
                .chain(std::iter::once(plan.artifact())),
        );

        let path_column_width = max_column_width(
            plan.packages()
                .iter()
                .map(BuildProgressPackage::path)
                .chain(std::iter::once(plan.output_path())),
        );

        let heading = terminal_heading(
            &progress,
            messages.heading(plan.product(), message_configuration(plan.configuration())),
        );

        let aggregate = progress.add(ProgressBar::new(plan.total_units()));

        aggregate.set_style(active_product_progress_style(messages));

        aggregate.set_prefix(messages.operation(BuildProgressOperation::Building));
        aggregate.set_message(padded(plan.artifact(), subject_column_width));
        aggregate.enable_steady_tick(Duration::from_millis(80));

        let packages = plan
            .packages()
            .iter()
            .map(|package| {
                (
                    package.identity().to_owned(),
                    PackageTerminalProgress::new(package.clone()),
                )
            })
            .collect();

        Self {
            progress,
            heading,
            aggregate,
            packages,
            subject_column_width,
            path_column_width,
            messages,
            verbose,
        }
    }

    pub(super) fn start_package(&self, identity: &str) {
        let Some(package) = self.packages.get(identity) else {
            return;
        };

        let started = package.start(|| {
            let bar = self
                .progress
                .insert_before(&self.aggregate, ProgressBar::new(package.plan.units()));

            bar.set_style(line_style(
                self.messages,
                BuildProgressLineKind::Package,
                padded(package.plan.path(), self.path_column_width),
                ProgressVisualState::Active,
            ));

            bar.set_prefix(self.messages.operation(BuildProgressOperation::Compiling));

            bar.set_message(padded(package.plan.identity(), self.subject_column_width));

            bar.enable_steady_tick(Duration::from_millis(80));

            bar
        });

        if !started || !self.verbose {
            return;
        }

        let detail = self.messages.action(message_action(package.plan.action()));
        let _ = self.progress.println(format!("      {detail}"));
    }

    pub(super) fn finish_package(&self, identity: &str, status: BuildProgressStatus) {
        let Some(package) = self.packages.get(identity) else {
            return;
        };

        let Some(bar) = package.bar() else {
            return;
        };

        if status == BuildProgressStatus::Complete {
            bar.set_position(package.plan.units());
        }

        bar.set_style(finished_progress_style(
            self.messages,
            BuildProgressLineKind::Package,
            padded(package.plan.path(), self.path_column_width),
            status,
        ));

        bar.set_prefix(
            self.messages
                .operation(status_operation(status, BuildProgressOperation::Compiled)),
        );

        bar.finish();
    }

    pub(super) fn set_package_units(&self, identity: &str, completed: u64) {
        let Some(package) = self.packages.get(identity) else {
            return;
        };

        let Some(bar) = package.bar() else {
            return;
        };

        bar.set_position(completed);
    }

    pub(super) fn set_completed_units(&self, completed: u64) {
        self.aggregate.set_position(completed);
    }

    pub(super) fn finish(&self, plan: &BuildProgressPlan, status: BuildProgressStatus) {
        self.aggregate.disable_steady_tick();

        if status == BuildProgressStatus::Complete {
            self.aggregate.set_position(plan.total_units());
        }

        self.aggregate.set_style(finished_progress_style(
            self.messages,
            BuildProgressLineKind::FinishedProduct,
            padded(plan.output_path(), self.path_column_width),
            status,
        ));

        self.aggregate.set_prefix(
            self.messages
                .operation(status_operation(status, BuildProgressOperation::Finished)),
        );

        self.aggregate.finish();
        self.heading.finish();
    }
}

struct PackageTerminalProgress {
    plan: BuildProgressPackage,
    bar: std::sync::Mutex<Option<ProgressBar>>,
}

impl PackageTerminalProgress {
    fn new(plan: BuildProgressPackage) -> Self {
        Self {
            plan,
            bar: std::sync::Mutex::new(None),
        }
    }

    fn start(&self, create: impl FnOnce() -> ProgressBar) -> bool {
        let mut current = self.bar.lock().unwrap_or_else(|error| error.into_inner());

        if current.is_some() {
            return false;
        }

        *current = Some(create());

        true
    }

    fn bar(&self) -> Option<ProgressBar> {
        self.bar
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

fn active_product_progress_style(messages: BuildProgressMessageRenderer) -> ProgressStyle {
    line_style(
        messages,
        BuildProgressLineKind::ActiveProduct,
        String::new(),
        ProgressVisualState::Waiting,
    )
}

fn finished_progress_style(
    messages: BuildProgressMessageRenderer,
    kind: BuildProgressLineKind,
    path: String,
    status: BuildProgressStatus,
) -> ProgressStyle {
    line_style(messages, kind, path, visual_state(status))
}

fn line_style(
    messages: BuildProgressMessageRenderer,
    kind: BuildProgressLineKind,
    path: String,
    state: ProgressVisualState,
) -> ProgressStyle {
    terminal_line_style(
        messages.fields(kind),
        state,
        path,
        String::new(),
        move |completed, total| messages.unit_count(completed, total),
        move |milliseconds| messages.duration(milliseconds),
        move |percentage| messages.percentage(percentage),
    )
}

const fn status_operation(
    status: BuildProgressStatus,
    successful: BuildProgressOperation,
) -> BuildProgressOperation {
    match status {
        BuildProgressStatus::Complete => successful,
        BuildProgressStatus::Failed => BuildProgressOperation::Failed,
    }
}

const fn visual_state(status: BuildProgressStatus) -> ProgressVisualState {
    match status {
        BuildProgressStatus::Complete => ProgressVisualState::Complete,
        BuildProgressStatus::Failed => ProgressVisualState::Failed,
    }
}
