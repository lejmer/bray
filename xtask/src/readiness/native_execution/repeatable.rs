use super::built_fixture::BuiltFixture;
use std::path::Path;

use bray_target::NativeTarget;

use super::artifact::{
    inspect_objects, require_equal_artifacts, require_equal_files, require_evidence,
};
use super::core::{build_fixtures, execute_product};

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
        RepeatableFixtureAudit {
            fixtures: &[fixture],
            expected,
            name,
            required_object_evidence,
        },
    )
}

/// Inputs that distinguish one repeatable native fixture audit.
pub(super) struct RepeatableFixtureAudit<'fixture> {
    fixtures: &'fixture [&'fixture str],
    expected: i32,
    name: &'fixture str,
    required_object_evidence: &'fixture [&'fixture str],
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
        }
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

    execute_product(
        first.executable(),
        audit.expected,
        &format!("executing {}", audit.name),
    )
}
