use std::collections::BTreeMap;
use std::time::Duration;

use bray_messages::{
    BuildProgressField, BuildProgressLineKind, BuildProgressMessageRenderer,
    BuildProgressOperation,
};
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};

use super::model::{
    BuildProgressPackage, BuildProgressPlan, BuildProgressStatus,
};
use super::text::{
    max_column_width, message_action, message_configuration, status_marker,
};

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
        let progress = MultiProgress::with_draw_target(ProgressDrawTarget::stderr_with_hz(20));

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

        let heading = progress.add(ProgressBar::new(0));

        heading.set_style(progress_style("{msg:.bold}"));

        heading.set_message(messages.heading(
            plan.product(),
            message_configuration(plan.configuration()),
        ));

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

            bar.set_style(active_progress_style(
                self.messages,
                BuildProgressLineKind::Package,
                padded(package.plan.path(), self.path_column_width),
                "{spinner:.cyan.bold}",
                "cyan",
            ));

            bar.set_prefix(
                self.messages
                    .operation(BuildProgressOperation::Compiling),
            );

            bar.set_message(padded(
                package.plan.identity(),
                self.subject_column_width,
            ));

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

        bar.set_prefix(self.messages.operation(status_operation(
            status,
            BuildProgressOperation::Compiled,
        )));

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

        self.aggregate.set_prefix(self.messages.operation(status_operation(
            status,
            BuildProgressOperation::Finished,
        )));

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
    active_progress_style(
        messages,
        BuildProgressLineKind::ActiveProduct,
        String::new(),
        "{spinner:.white}",
        "cyan",
    )
    .tick_strings(&["○", "○"])
}

fn active_progress_style(
    messages: BuildProgressMessageRenderer,
    kind: BuildProgressLineKind,
    path: String,
    marker: &str,
    color: &str,
) -> ProgressStyle {
    line_style(messages, kind, path, marker, color)
        .tick_strings(&["◐", "◓", "◑", "◒", "◐"])
        .progress_chars("━╸ ")
}

fn finished_progress_style(
    messages: BuildProgressMessageRenderer,
    kind: BuildProgressLineKind,
    path: String,
    status: BuildProgressStatus,
) -> ProgressStyle {
    line_style(
        messages,
        kind,
        path,
        status_marker(status),
        status_color(status),
    )
}

fn line_style(
    messages: BuildProgressMessageRenderer,
    kind: BuildProgressLineKind,
    path: String,
    marker: &str,
    color: &str,
) -> ProgressStyle {
    let fields = messages
        .fields(kind)
        .iter()
        .map(|field| field_template(*field, color))
        .collect::<Vec<_>>()
        .join(" ");

    let template = format!("   {marker} {fields}");

    progress_style(&template)
        .with_key("path", move |_: &indicatif::ProgressState, writer: &mut dyn std::fmt::Write| {
            let _ = writer.write_str(&path);
        })
        .with_key("unit_count", move |state: &indicatif::ProgressState, writer: &mut dyn std::fmt::Write| {
            let _ = writer.write_str(
                &messages.unit_count(state.pos(), state.len().unwrap_or_default()),
            );
        })
        .with_key("duration", move |state: &indicatif::ProgressState, writer: &mut dyn std::fmt::Write| {
            let _ = writer.write_str(&messages.duration(state.elapsed().as_millis()));
        })
        .with_key("percentage", move |state: &indicatif::ProgressState, writer: &mut dyn std::fmt::Write| {
            let percentage = match state.len().unwrap_or_default() {
                0 => 100,
                total => state.pos().saturating_mul(100) / total,
            };

            let _ = writer.write_str(&messages.percentage(percentage));
        })
}

fn field_template(field: BuildProgressField, color: &str) -> String {
    match field {
        BuildProgressField::Operation => format!("{{prefix:.{color}.bold}}"),
        BuildProgressField::Subject => String::from("{msg}"),
        BuildProgressField::Path => String::from("{path}"),
        BuildProgressField::Bar => String::from("{bar:20.cyan/dim}"),
        BuildProgressField::Percentage => String::from("{percentage}"),
        BuildProgressField::UnitCount => String::from("{unit_count}"),
        BuildProgressField::Duration => String::from("{duration:.dim}"),
    }
}

fn progress_style(template: &str) -> ProgressStyle {
    match ProgressStyle::with_template(template) {
        Ok(style) => style,
        Err(error) => panic!("validated progress template must compile: {error}"),
    }
}

fn padded(value: &str, width: usize) -> String {
    format!("{value:<width$}")
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

const fn status_color(status: BuildProgressStatus) -> &'static str {
    match status {
        BuildProgressStatus::Complete => "green",
        BuildProgressStatus::Failed => "red",
    }
}

#[cfg(test)]
mod tests {
    use bray_messages::{BuildProgressLineKind, BuildProgressMessageRenderer};

    use super::{active_product_progress_style, active_progress_style, finished_progress_style};
    use crate::tack::progress::model::BuildProgressStatus;

    #[test]
    fn terminal_progress_styles_have_valid_templates_and_ticks() {
        let messages = BuildProgressMessageRenderer::english();

        let active_product = active_product_progress_style(messages);

        assert_eq!(active_product.get_tick_str(0), "○");
        assert_eq!(active_product.get_final_tick_str(), "○");

        let package = active_progress_style(
            messages,
            BuildProgressLineKind::Package,
            String::from("package"),
            "{spinner}",
            "cyan",
        );

        assert_eq!(package.get_tick_str(0), "◐");
        assert_eq!(package.get_tick_str(1), "◓");
        assert_eq!(package.get_tick_str(2), "◑");
        assert_eq!(package.get_tick_str(3), "◒");

        for status in [BuildProgressStatus::Complete, BuildProgressStatus::Failed] {
            let _ = finished_progress_style(
                messages,
                BuildProgressLineKind::Package,
                String::from("package"),
                status,
            );

            let _ = finished_progress_style(
                messages,
                BuildProgressLineKind::FinishedProduct,
                String::from("product"),
                status,
            );
        }
    }
}
