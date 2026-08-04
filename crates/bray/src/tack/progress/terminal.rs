use std::collections::BTreeMap;
use std::time::Duration;

use bray_messages::{BuildProgressMessage, BuildProgressMessageRenderer};
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};

use super::model::{
    BuildProgressAction, BuildProgressPackage, BuildProgressPlan, BuildProgressStatus,
};
use super::text::{configuration_text, max_column_width, status_marker};

pub(super) struct TerminalBuildProgress {
    progress: MultiProgress,
    heading: ProgressBar,
    aggregate: ProgressBar,
    packages: BTreeMap<String, PackageTerminalProgress>,
    artifact_column_width: usize,
    path_column_width: usize,
    messages: BuildProgressMessageRenderer,
    verbose: bool,
}

impl TerminalBuildProgress {
    pub(super) fn new(plan: &BuildProgressPlan, verbose: bool) -> Self {
        let messages = BuildProgressMessageRenderer::english();
        let progress = MultiProgress::with_draw_target(ProgressDrawTarget::stderr_with_hz(20));

        let artifact_column_width = max_column_width(
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

        let heading = progress.add(ProgressBar::new(0));

        heading.set_style(progress_style("{prefix:.cyan.bold} {msg:.bold}"));
        heading.set_prefix(messages.render(BuildProgressMessage::Building));

        heading.set_message(format!(
            "{} [{}]",
            plan.product(),
            configuration_text(messages, plan.configuration())
        ));

        let aggregate = progress.add(ProgressBar::new(plan.total_units()));

        aggregate.set_style(active_product_style(messages));

        aggregate.set_prefix(padded_subject(
            messages.render(BuildProgressMessage::Building),
            plan.artifact(),
            artifact_column_width,
        ));

        aggregate.set_message(String::new());
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
            artifact_column_width,
            path_column_width,
            messages,
            verbose,
        }
    }

    pub(super) fn start_package(&self, identity: &str) {
        let Some(package) = self.packages.get(identity) else {
            return;
        };

        if package.started() {
            return;
        }

        let bar = self
            .progress
            .insert_before(&self.aggregate, ProgressBar::new(package.plan.units()));

        bar.set_style(active_package_style(self.messages));

        bar.set_prefix(padded_subject(
            self.messages.render(BuildProgressMessage::Compiling),
            package.plan.identity(),
            self.artifact_column_width,
        ));

        bar.set_message(padded_path(package.plan.path(), self.path_column_width));
        bar.enable_steady_tick(Duration::from_millis(80));

        package.start(bar);

        if self.verbose {
            let detail = match package.plan.action() {
                BuildProgressAction::CheckInterface => self
                    .messages
                    .render(BuildProgressMessage::CheckingInterface),
                BuildProgressAction::ProduceArtifacts => self
                    .messages
                    .render(BuildProgressMessage::ProducingArtifacts),
            };

            let _ = self.progress.println(format!("      {detail}"));
        }
    }

    pub(super) fn finish_package(&self, identity: &str, status: BuildProgressStatus) {
        let Some(package) = self.packages.get(identity) else {
            return;
        };

        let Some(bar) = package.bar() else {
            return;
        };

        match status {
            BuildProgressStatus::Complete => {
                bar.set_position(package.plan.units());
                bar.set_style(finished_package_style(self.messages, true));

                bar.set_prefix(padded_subject(
                    self.messages.render(BuildProgressMessage::Compiled),
                    package.plan.identity(),
                    self.artifact_column_width,
                ));
            }
            BuildProgressStatus::Failed => {
                bar.set_style(finished_package_style(self.messages, false));

                bar.set_prefix(padded_subject(
                    self.messages.render(BuildProgressMessage::Failed),
                    package.plan.identity(),
                    self.artifact_column_width,
                ));
            }
        }

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

        self.aggregate
            .set_message(padded_path(plan.output_path(), self.path_column_width));

        match status {
            BuildProgressStatus::Complete => {
                self.aggregate.set_position(plan.total_units());

                self.aggregate
                    .set_style(finished_product_style(self.messages, true));

                self.aggregate.set_prefix(padded_subject(
                    self.messages.render(BuildProgressMessage::Finished),
                    plan.artifact(),
                    self.artifact_column_width,
                ));
            }
            BuildProgressStatus::Failed => {
                self.aggregate
                    .set_style(finished_product_style(self.messages, false));

                self.aggregate.set_prefix(padded_subject(
                    self.messages.render(BuildProgressMessage::Failed),
                    plan.artifact(),
                    self.artifact_column_width,
                ));
            }
        }

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

    fn started(&self) -> bool {
        self.bar
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_some()
    }

    fn start(&self, bar: ProgressBar) {
        let mut current = self.bar.lock().unwrap_or_else(|error| error.into_inner());

        *current = Some(bar);
    }

    fn bar(&self) -> Option<ProgressBar> {
        self.bar
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

fn active_package_style(messages: BuildProgressMessageRenderer) -> ProgressStyle {
    progress_style(&format!(
        "   {{spinner:.cyan.bold}} {{prefix:.cyan.bold}} {{msg}} {{pos:>4}}/{{len:<4}} {} {{elapsed_ms:.dim}}",
        messages.render(BuildProgressMessage::Units)
    ))
    .tick_strings(&["◐", "◓", "◑", "◒"])
    .with_key("elapsed_ms", elapsed_milliseconds)
}

fn finished_package_style(messages: BuildProgressMessageRenderer, success: bool) -> ProgressStyle {
    let status = if success {
        BuildProgressStatus::Complete
    } else {
        BuildProgressStatus::Failed
    };

    let marker = status_marker(status);
    let color = status_color(status);

    progress_style(&format!(
        "   {marker} {{prefix:.{color}.bold}} {{msg}} {{pos:>4}}/{{len:<4}} {} {{elapsed_ms:.dim}}",
        messages.render(BuildProgressMessage::Units),
        marker = marker,
        color = color
    ))
    .with_key("elapsed_ms", elapsed_milliseconds)
}

fn active_product_style(messages: BuildProgressMessageRenderer) -> ProgressStyle {
    progress_style(&format!(
        "   {{spinner:.white}} {{prefix:.cyan.bold}} {{bar:20.cyan/dim}} {{percent:>3}}% {{pos:>4}}/{{len:<4}} {} {{elapsed_ms:.dim}}",
        messages.render(BuildProgressMessage::Units)
    ))
    .tick_strings(&["○"])
    .progress_chars("━╸ ")
    .with_key("elapsed_ms", elapsed_milliseconds)
}

fn finished_product_style(messages: BuildProgressMessageRenderer, success: bool) -> ProgressStyle {
    let status = if success {
        BuildProgressStatus::Complete
    } else {
        BuildProgressStatus::Failed
    };

    let marker = status_marker(status);
    let color = status_color(status);

    progress_style(&format!(
        "   {marker} {{prefix:.{color}.bold}} {{msg}} {{pos:>4}}/{{len:<4}} {} {{elapsed_ms:.dim}}",
        messages.render(BuildProgressMessage::Units),
        marker = marker,
        color = color
    ))
    .with_key("elapsed_ms", elapsed_milliseconds)
}

const fn status_color(status: BuildProgressStatus) -> &'static str {
    match status {
        BuildProgressStatus::Complete => "green",
        BuildProgressStatus::Failed => "red",
    }
}

fn progress_style(template: &str) -> ProgressStyle {
    match ProgressStyle::with_template(template) {
        Ok(style) => style,
        Err(error) => panic!("validated progress template must compile: {error}"),
    }
}

fn elapsed_milliseconds(state: &indicatif::ProgressState, writer: &mut dyn std::fmt::Write) {
    let _ = write!(writer, "{} ms", state.elapsed().as_millis());
}

fn padded_subject(status: &str, subject: &str, subject_width: usize) -> String {
    format!("{status} {subject:<subject_width$}")
}

fn padded_path(path: &str, path_width: usize) -> String {
    format!("{path:<path_width$}")
}

#[cfg(test)]
mod tests {
    use bray_messages::BuildProgressMessageRenderer;

    use super::{
        active_package_style, active_product_style, finished_package_style, finished_product_style,
    };

    #[test]
    fn every_terminal_progress_template_is_valid() {
        let messages = BuildProgressMessageRenderer::english();

        let _ = active_package_style(messages);
        let _ = active_product_style(messages);
        let _ = finished_package_style(messages, true);
        let _ = finished_package_style(messages, false);
        let _ = finished_product_style(messages, true);
        let _ = finished_product_style(messages, false);
    }
}
