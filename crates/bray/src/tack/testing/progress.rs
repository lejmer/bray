use std::collections::BTreeMap;
use std::time::Duration;

use bray_messages::{
    TestReportActivity, TestReportLineKind, TestReportMessageRenderer, TestReportOutcome,
};
use bray_test_protocol::{
    TestCommandReport, TestExecutionPlan, TestIdentity, TestInvocationResult,
};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use super::identity::identity_text;
use super::report::{
    operation_width, outcome_state, render_details, report_outcome, summary_state,
    summary_status,
};
use crate::tack::progress::{
    ProgressVisualState, empty_count, empty_percentage, max_column_width, padded,
    terminal_heading, terminal_line_style, terminal_progress,
};

pub(super) struct TestProgress {
    terminal: Option<TerminalTestProgress>,
}

impl TestProgress {
    pub(super) fn new(plan: &TestExecutionPlan, interactive: bool) -> Self {
        Self {
            terminal: interactive.then(|| TerminalTestProgress::new(plan)),
        }
    }

    pub(super) fn start(&mut self, identity: &TestIdentity) {
        if let Some(terminal) = &mut self.terminal {
            terminal.start(identity);
        }
    }

    pub(super) fn finish_result(&mut self, result: &TestInvocationResult) {
        if let Some(terminal) = &mut self.terminal {
            terminal.finish_result(result);
        }
    }

    pub(super) fn finish(&mut self, report: &TestCommandReport, show_output: bool) {
        if let Some(terminal) = &mut self.terminal {
            terminal.finish(report, show_output);
        }
    }

    pub(super) fn fail(&mut self) {
        if let Some(terminal) = &mut self.terminal {
            terminal.fail();
        }
    }
}

struct TerminalTestProgress {
    progress: MultiProgress,
    heading: ProgressBar,
    tests: BTreeMap<TestIdentity, TestTerminalLine>,
    messages: TestReportMessageRenderer,
    operation_width: usize,
    completed: bool,
}

impl TerminalTestProgress {
    fn new(plan: &TestExecutionPlan) -> Self {
        let messages = TestReportMessageRenderer::english();
        let progress = terminal_progress();
        let _ = progress.println(String::new());

        let heading = terminal_heading(&progress, messages.heading(plan.invocations().len()));
        let operation_width = operation_width(messages);

        let identities = plan
            .invocations()
            .iter()
            .map(|invocation| identity_text(invocation.identity()))
            .collect::<Vec<_>>();

        let subject_width = max_column_width(identities.iter().map(String::as_str));

        let tests = plan
            .invocations()
            .iter()
            .zip(identities)
            .map(|(invocation, identity)| {
                let bar = progress.add(ProgressBar::new(1));

                bar.set_style(line_style(
                    messages,
                    TestReportLineKind::WaitingResult,
                    ProgressVisualState::Waiting,
                    String::new(),
                ));

                bar.set_prefix(padded(
                    messages.activity_operation(TestReportActivity::Waiting),
                    operation_width,
                ));

                bar.set_message(padded(&identity, subject_width));

                (
                    invocation.identity().clone(),
                    TestTerminalLine {
                        bar,
                        finished: false,
                    },
                )
            })
            .collect();

        Self {
            progress,
            heading,
            tests,
            messages,
            operation_width,
            completed: false,
        }
    }

    fn start(&mut self, identity: &TestIdentity) {
        let Some(test) = self.tests.get_mut(identity) else {
            return;
        };

        if test.finished {
            return;
        }

        test.bar.reset_elapsed();

        test.bar.set_style(line_style(
            self.messages,
            TestReportLineKind::Result,
            ProgressVisualState::Active,
            String::new(),
        ));

        test.bar.set_prefix(padded(
            self.messages
                .activity_operation(TestReportActivity::Running),
            self.operation_width,
        ));

        test.bar.enable_steady_tick(Duration::from_millis(80));
    }

    fn finish_result(&mut self, result: &TestInvocationResult) {
        let Some(test) = self.tests.get_mut(result.identity()) else {
            return;
        };

        if test.finished {
            return;
        }

        let outcome = report_outcome(result.outcome());
        let state = outcome_state(outcome);

        test.bar.disable_steady_tick();

        if let Some(duration) = result.duration() {
            test.bar.set_elapsed(duration.duration());
        }

        test.bar.set_position(1);

        test.bar.set_style(line_style(
            self.messages,
            TestReportLineKind::Result,
            state,
            String::new(),
        ));

        test.bar.set_prefix(padded(
            self.messages.result_operation(outcome),
            self.operation_width,
        ));

        test.bar.finish();
        test.finished = true;
    }

    fn finish(&mut self, report: &TestCommandReport, show_output: bool) {
        if self.completed {
            return;
        }

        let status = summary_status(report.succeeded());
        let state = summary_state(status);
        let counts = report.counts();

        let detail = self.messages.summary_counts(
            counts.passed(),
            counts.failed(),
            report.selection().filtered_out(),
        );

        let details = render_details(report, show_output);

        if !details.is_empty() {
            let _ = self.progress.println(String::new());

            for line in details.lines() {
                let _ = self.progress.println(line);
            }
        }

        let summary = self.progress.add(ProgressBar::new(0));

        summary.set_style(line_style(
            self.messages,
            TestReportLineKind::Summary,
            state,
            detail,
        ));

        summary.set_prefix(padded(
            self.messages.summary_operation(status),
            self.operation_width,
        ));

        if let Some(duration) = report.duration() {
            summary.set_elapsed(duration.duration());
        }

        summary.finish();
        self.heading.finish();
        self.completed = true;
    }

    fn fail(&mut self) {
        if self.completed {
            return;
        }

        let operation = self
            .messages
            .result_operation(TestReportOutcome::InfrastructureFailed);

        for test in self.tests.values_mut().filter(|test| !test.finished) {
            test.bar.disable_steady_tick();

            test.bar.set_style(line_style(
                self.messages,
                TestReportLineKind::Result,
                ProgressVisualState::Failed,
                String::new(),
            ));

            test.bar
                .set_prefix(padded(operation, self.operation_width));

            test.bar.finish();
            test.finished = true;
        }

        self.heading.finish();
        self.completed = true;
    }
}

struct TestTerminalLine {
    bar: ProgressBar,
    finished: bool,
}

fn line_style(
    messages: TestReportMessageRenderer,
    kind: TestReportLineKind,
    state: ProgressVisualState,
    detail: String,
) -> ProgressStyle {
    terminal_line_style(
        messages.fields(kind),
        state,
        String::new(),
        detail,
        empty_count,
        move |milliseconds| messages.duration(milliseconds),
        empty_percentage,
    )
}
