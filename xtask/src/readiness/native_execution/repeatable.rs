use super::built_fixture::BuiltFixture;
use std::path::Path;

use bray_target::NativeTarget;

use super::core::{
    build_fixtures, execute_product, inspect_objects, product_output, require_equal_artifacts,
    require_equal_files, require_evidence,
};

#[expect(
    clippy::too_many_arguments,
    reason = "repeatability checks retain fixture and toolchain paths, expected behavior, output identity, and object evidence"
)]
pub(super) fn audit_repeatable_fixture(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
    prefix: &str,
    fixture: &str,
    expected: i32,
    name: &str,
    required_object_evidence: &[&str],
) -> Result<(), String> {
    audit_repeatable_fixtures(
        root,
        target,
        runtime,
        prefix,
        RepeatableFixtureAudit::new(&[fixture], expected, name, required_object_evidence),
    )
}

/// Inputs that distinguish one repeatable native fixture audit.
pub(super) struct RepeatableFixtureAudit<'fixture> {
    fixtures: &'fixture [&'fixture str],
    expected: i32,
    name: &'fixture str,
    required_object_evidence: &'fixture [&'fixture str],
    stdout: Option<&'fixture str>,
}

impl<'fixture> RepeatableFixtureAudit<'fixture> {
    /// Creates one native fixture audit from its sources and expected evidence.
    pub(super) const fn new(
        fixtures: &'fixture [&'fixture str],
        expected: i32,
        name: &'fixture str,
        required_object_evidence: &'fixture [&'fixture str],
    ) -> Self {
        Self {
            fixtures,
            expected,
            name,
            required_object_evidence,
            stdout: None,
        }
    }

    /// Requires this exact output from the executable, including cleanup.
    pub(super) const fn with_stdout(mut self, stdout: &'fixture str) -> Self {
        self.stdout = Some(stdout);

        self
    }
}

pub(super) fn audit_repeatable_fixtures(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
    prefix: &str,
    audit: RepeatableFixtureAudit<'_>,
) -> Result<(), String> {
    let first = BuiltFixture::build_command_line(&format!("{prefix}first-"), target, |output| {
        build_fixtures(root, target, runtime, output, None, audit.fixtures)
    })?;

    let second = BuiltFixture::build_command_line(&format!("{prefix}second-"), target, |output| {
        build_fixtures(root, target, runtime, output, None, audit.fixtures)
    })?;

    require_equal_files(
        first.executable(),
        second.executable(),
        &format!("{} executable", audit.name),
    )?;

    require_equal_artifacts(first.objects(), second.objects())?;

    if !audit.required_object_evidence.is_empty() {
        let report = inspect_objects(root, first.objects())?;

        require_evidence(&report, audit.required_object_evidence)?;
    }

    let operation = format!("executing {}", audit.name);
    let executable = first.executable();

    if let Some(expected) = audit.stdout {
        let output = product_output(executable, &operation)?;
        let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");

        if output.status.code() == Some(audit.expected) && stdout == expected && output.stderr.is_empty() {
            return Ok(());
        }

        return Err(crate::command::failure(&operation, &output));
    }

    execute_product(executable, audit.expected, &operation)
}
