use std::collections::BTreeSet;
use std::path::Path;
use std::time::Instant;

use serde::Serialize;

use super::inventory::SelectedTest;

#[derive(Serialize)]
pub(super) struct TestTiming {
    binary: String,
    name: String,
    seconds: f64,
    success: bool,
}

#[derive(Serialize)]
pub(super) struct Report {
    pub(super) schema: u32,
    pub(super) tier: String,
    pub(super) shard: usize,
    pub(super) shards: usize,
    pub(super) preparation_seconds: f64,
    pub(super) execution_seconds: f64,
    pub(super) budget_seconds: f64,
    pub(super) selected: usize,
    pub(super) success: bool,
    pub(super) tests: Vec<TestTiming>,
}

pub(super) struct Recorder {
    pending: Vec<u8>,
    last: Instant,
    binary: String,
    expected: BTreeSet<String>,
    pub(super) tests: Vec<TestTiming>,
}

impl Recorder {
    pub(super) fn new(tests: &[SelectedTest]) -> Self {
        Self {
            pending: Vec::new(),
            last: Instant::now(),
            binary: tests[0].binary.clone(),
            expected: tests.iter().map(|test| test.name.clone()).collect(),
            tests: Vec::new(),
        }
    }

    pub(super) fn observe(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);

        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<_> = self.pending.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);

            self.line(line.trim_end(), Instant::now());
        }
    }

    fn line(&mut self, line: &str, now: Instant) {
        let Some((name, status)) = line
            .strip_prefix("test ")
            .and_then(|line| line.rsplit_once(" ... "))
        else {
            return;
        };

        let name = name.strip_suffix(" - should panic").unwrap_or(name);

        if status != "ok" && status != "FAILED" {
            return;
        }

        self.tests.push(TestTiming {
            binary: self.binary.clone(),
            name: name.to_owned(),
            seconds: now.duration_since(self.last).as_secs_f64(),
            success: status == "ok",
        });

        self.last = now;
    }

    pub(super) fn complete(&self) -> bool {
        self.tests.len() == self.expected.len()
            && self
                .tests
                .iter()
                .map(|test| test.name.clone())
                .collect::<BTreeSet<_>>()
                == self.expected
    }
}

pub(super) fn write(directory: &Path, mut report: Report) -> Result<(), String> {
    report.tests.sort_by(|left, right| {
        right
            .seconds
            .total_cmp(&left.seconds)
            .then_with(|| left.name.cmp(&right.name))
    });

    for test in report.tests.iter().take(20) {
        eprintln!("{:8.3}s {}::{}", test.seconds, test.binary, test.name);
    }

    let mut junit = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><testsuites><testsuite name=\"compiler-{}\" tests=\"{}\" failures=\"{}\" time=\"{}\">",
        report.tier,
        report.tests.len() + usize::from(!report.success),
        report.tests.iter().filter(|test| !test.success).count() + usize::from(!report.success),
        report.execution_seconds
    );

    for test in &report.tests {
        junit.push_str(&format!(
            "<testcase classname=\"{}\" name=\"{}\" time=\"{}\">{} </testcase>",
            xml(&test.binary),
            xml(&test.name),
            test.seconds,
            if test.success {
                ""
            } else {
                "<failure message=\"test failed, see run.log\"/>"
            }
        ));
    }

    if !report.success {
        junit.push_str("<testcase name=\"tier-completion\"><failure message=\"tier failed, exceeded its budget, or did not execute its complete selection, see run.log\"/></testcase>");
    }

    junit.push_str("</testsuite></testsuites>\n");

    std::fs::write(directory.join("junit.xml"), junit)
        .map_err(|error| format!("could not write JUnit report: {error}"))?;

    crate::json::write_pretty(&directory.join("timings.json"), &report)
}

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{Recorder, SelectedTest};

    #[test]
    fn records_split_output_failures_and_should_panic_tests() {
        let tests = ["normal", "panics"].map(|name| SelectedTest {
            binary: "compiler".to_owned(),
            executable: "compiler".into(),
            name: name.to_owned(),
            tier: "fast".to_owned(),
        });

        let mut recorder = Recorder::new(&tests);
        let start = Instant::now();

        recorder.last = start;
        recorder.line("test normal ... FAILED", start + Duration::from_secs(2));
        recorder.observe(b"test panics - should panic ... o");

        assert!(!recorder.complete());

        recorder.last = Instant::now();
        recorder.observe(b"k\n");

        assert!(recorder.complete());
        assert!(!recorder.tests[0].success);
        assert_eq!(recorder.tests[0].seconds, 2.0);
        assert!(recorder.tests[1].success);
    }
}
