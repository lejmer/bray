// rust-style: allow(module-too-large, reason = "the native standard-library audit keeps one end-to-end build and execution contract")

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use bray_base::is_lowercase_hex;
use bray_symbols::{PackageIdentity, ProductIdentity, TestExecutionConstraint};
use bray_target::NativeTarget;
use bray_test_protocol::decode_test_catalog;
use serde::Deserialize;

use super::command::BuildError;

const PACKAGE_IDENTITY: &str = "std";
const API_PRODUCT: &str = "api";
const OUTCOME_PRODUCT: &str = "outcomes";
const CHILD_EXECUTABLE_ENVIRONMENT_VARIABLE: &str = "BRAY_STANDARD_LIBRARY_TEST_EXECUTABLE";
const API_TEST_COUNT: usize = 65;
const API_FILTERED_TEST_COUNT: usize = 3;

pub(super) fn test() -> Result<(), BuildError> {
    let root = crate::workspace::root().map_err(BuildError::Workspace)?;

    let target = NativeTarget::current()
        .ok_or_else(|| BuildError::conformance("native", "the compiler host is not supported"))?;

    crate::native_toolchain::build_compiler(&root)
        .map_err(|error| BuildError::conformance("native", error))?;

    let temporary_directory = tempfile::Builder::new()
        .prefix("bray-standard-library-native-")
        .tempdir()
        .map_err(BuildError::TemporaryDirectory)?;

    let directory = temporary_directory.path();

    let runtime = directory.join("runtime");

    fs::create_dir(&runtime).map_err(|error| BuildError::write(&runtime, error))?;

    let runtime = crate::runtime_artifact::build_for_readiness(target, &runtime)
        .map_err(|error| BuildError::conformance("native runtime", error))?;

    let toolchain = directory.join("toolchain");

    crate::native_toolchain::assemble(&root, target, &runtime, &toolchain)
        .map_err(|error| BuildError::conformance("native toolchain", error))?;

    super::provider_retention::audit(&root, directory, &toolchain, target)?;
    super::interoperability::audit(&root, directory, &toolchain, &runtime, target)?;

    let workspace = directory.join("workspace");

    copy_standard_library_workspace(&root, &workspace)?;
    audit_api(&root, &workspace, &toolchain, target)?;

    audit_outcomes(&root, &workspace, &toolchain, target)
}

fn audit_api(
    root: &Path,
    workspace: &Path,
    toolchain: &Path,
    target: NativeTarget,
) -> Result<(), BuildError> {
    let sequential = run_tests(
        root,
        workspace,
        toolchain,
        target,
        API_PRODUCT,
        &["--sequential", "--timeout-ms", "1000"],
    )?;

    require_success("sequential execution", &sequential)?;

    let sequential_report = parse_report("sequential execution", &sequential)?;

    validate_api_report(&sequential_report)?;

    let executable = product_artifact(workspace, target, API_PRODUCT, Artifact::Executable)?;
    let catalog = product_artifact(workspace, target, API_PRODUCT, Artifact::Catalog)?;
    let sequential_executable = read_artifact(&executable)?;
    let sequential_catalog = read_artifact(&catalog)?;

    require_serial_metadata(&sequential_catalog)?;

    let parallel = run_tests(
        root,
        workspace,
        toolchain,
        target,
        API_PRODUCT,
        &["--jobs", "2", "--timeout-ms", "1000"],
    )?;

    require_success("parallel execution", &parallel)?;

    let parallel_report = parse_report("parallel execution", &parallel)?;

    validate_api_report(&parallel_report)?;
    require_stable_order(&sequential_report, &parallel_report)?;

    require_equal_artifact(
        "native test executable",
        &sequential_executable,
        &read_artifact(&executable)?,
    )?;

    require_equal_artifact(
        "native test catalog",
        &sequential_catalog,
        &read_artifact(&catalog)?,
    )?;

    let filtered = run_tests(
        root,
        workspace,
        toolchain,
        target,
        API_PRODUCT,
        &["--timeout-ms", "1000", "standard_output"],
    )?;

    require_success("filtered execution", &filtered)?;

    let filtered_report = parse_report("filtered execution", &filtered)?;

    require_selection(
        &filtered_report,
        API_TEST_COUNT,
        API_FILTERED_TEST_COUNT,
        API_TEST_COUNT - API_FILTERED_TEST_COUNT,
    )?;

    let tests = tests(&filtered_report);

    if tests.len() != 3
        || tests
            .iter()
            .any(|test| !test.identity.contains("standard_output"))
    {
        return Err(BuildError::conformance(
            "native filtering",
            "the standard_output filter did not select exactly the three I/O fixtures",
        ));
    }

    Ok(())
}

fn audit_outcomes(
    root: &Path,
    workspace: &Path,
    toolchain: &Path,
    target: NativeTarget,
) -> Result<(), BuildError> {
    let cases = [
        OutcomeCase::new("assertion_failure", OutcomeExpectation::Assertion),
        OutcomeCase::new("explicit_failure", OutcomeExpectation::Explicit),
        OutcomeCase::new("panic_failure", OutcomeExpectation::Panic),
        OutcomeCase::new("recoverable_error", OutcomeExpectation::ReturnedError),
        OutcomeCase::timed(
            "timeout_observes_cancellation",
            OutcomeExpectation::TimedOut,
        ),
        OutcomeCase::timed(
            "timeout_requires_forced_termination",
            OutcomeExpectation::ForcedTermination,
        ),
    ];

    for case in cases {
        audit_outcome(root, workspace, toolchain, target, case)?;
    }

    let catalog = product_artifact(workspace, target, OUTCOME_PRODUCT, Artifact::Catalog)?;

    require_catalog_separation(&read_artifact(&catalog)?)
}

fn audit_outcome(
    root: &Path,
    workspace: &Path,
    toolchain: &Path,
    target: NativeTarget,
    case: OutcomeCase,
) -> Result<(), BuildError> {
    let mut arguments = vec!["--sequential"];

    if case.timed {
        arguments.extend(["--timeout-ms", "100"]);
    }

    arguments.push(case.identity);

    let output = run_tests(
        root,
        workspace,
        toolchain,
        target,
        OUTCOME_PRODUCT,
        &arguments,
    )?;

    if output.status.success() {
        return Err(BuildError::conformance(
            "native outcomes",
            format!("{} unexpectedly succeeded", case.identity),
        ));
    }

    let report = parse_report("native outcomes", &output)?;

    require_product(&report, OUTCOME_PRODUCT)?;
    require_selection(&report, 6, 1, 5)?;

    if report.summary.passed != 0 || report.summary.failed != 1 {
        return Err(BuildError::conformance(
            "native outcomes",
            format!("{} did not report one failed test", case.identity),
        ));
    }

    let results = tests(&report);

    let [test] = results.as_slice() else {
        return Err(BuildError::conformance(
            "native outcomes",
            format!("{} did not produce one result", case.identity),
        ));
    };

    if !test.identity.ends_with(case.identity) {
        return Err(BuildError::conformance(
            "native outcomes",
            format!("{} selected an unrelated result", case.identity),
        ));
    }

    case.expectation.validate(case.identity, &test.outcome)?;
    require_stream(&test.identity, "stdout", &test.stdout, &[])?;

    require_stream(&test.identity, "stderr", &test.stderr, &[])
}

fn validate_api_report(report: &NativeTestReport) -> Result<(), BuildError> {
    require_product(report, API_PRODUCT)?;
    require_selection(report, API_TEST_COUNT, API_TEST_COUNT, 0)?;

    if report.summary.passed != API_TEST_COUNT || report.summary.failed != 0 {
        return Err(BuildError::conformance(
            "native execution",
            format!("the API product did not report {API_TEST_COUNT} passing tests"),
        ));
    }

    let tests = tests(report);
    let identities: Vec<_> = tests.iter().map(|test| test.identity.as_str()).collect();
    let mut sorted = identities.clone();

    sorted.sort_unstable();

    if identities != sorted {
        return Err(BuildError::conformance(
            "native ordering",
            "test identities are not in canonical order",
        ));
    }

    for test in tests {
        if !matches!(test.outcome, NativeOutcome::Passed) {
            return Err(BuildError::conformance(
                "native execution",
                format!("{} did not pass", test.identity),
            ));
        }

        let (expected_output, expected_error) = if test
            .identity
            .ends_with("asynchronous_standard_output_is_captured")
        {
            (b"captured-async-output".as_slice(), &[][..])
        } else if test
            .identity
            .ends_with("repeated_standard_output_locks_are_released")
        {
            (b"first-lock|second-lock".as_slice(), &[][..])
        } else if test.identity.ends_with("standard_output_is_captured") {
            (b"captured-standard-output".as_slice(), &[][..])
        } else if test.identity.ends_with("standard_error_is_captured") {
            (&[][..], b"captured-standard-error".as_slice())
        } else {
            (&[][..], &[][..])
        };

        require_stream(&test.identity, "stdout", &test.stdout, expected_output)?;
        require_stream(&test.identity, "stderr", &test.stderr, expected_error)?;
    }

    Ok(())
}

fn require_product(report: &NativeTestReport, product: &str) -> Result<(), BuildError> {
    let [actual] = report.products.as_slice() else {
        return Err(BuildError::conformance(
            "native report",
            "the report did not contain exactly one selected product",
        ));
    };

    if report.format != 1
        || actual.package != PACKAGE_IDENTITY
        || actual.product != product
        || !is_lowercase_sha256(&actual.catalog_digest)
    {
        return Err(BuildError::conformance(
            "native report",
            format!("the {product} report identity or catalog digest is invalid"),
        ));
    }

    Ok(())
}

fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64 && is_lowercase_hex(value)
}

fn require_stream(
    identity: &str,
    name: &str,
    stream: &NativeStream,
    expected: &[u8],
) -> Result<(), BuildError> {
    if stream.policy != "captured"
        || stream.bytes != expected
        || stream.truncated
        || stream.discarded_byte_count != 0
        || stream.failure.is_some()
    {
        return Err(BuildError::conformance(
            "native capture",
            format!("{identity} produced an invalid {name} stream"),
        ));
    }

    Ok(())
}

fn require_selection(
    report: &NativeTestReport,
    discovered: usize,
    selected: usize,
    filtered_out: usize,
) -> Result<(), BuildError> {
    if report.selection.discovered != discovered
        || report.selection.selected != selected
        || report.selection.filtered_out != filtered_out
    {
        return Err(BuildError::conformance(
            "native selection",
            format!(
                "expected {discovered}/{selected}/{filtered_out} discovered/selected/filtered but received {}/{}/{}",
                report.selection.discovered,
                report.selection.selected,
                report.selection.filtered_out
            ),
        ));
    }

    Ok(())
}

fn require_stable_order(
    first: &NativeTestReport,
    second: &NativeTestReport,
) -> Result<(), BuildError> {
    let first: Vec<_> = tests(first).iter().map(|test| &test.identity).collect();
    let second: Vec<_> = tests(second).iter().map(|test| &test.identity).collect();

    if first != second {
        return Err(BuildError::conformance(
            "native ordering",
            "sequential and parallel execution reported different test order",
        ));
    }

    Ok(())
}

fn require_catalog_separation(bytes: &[u8]) -> Result<(), BuildError> {
    for source_message in [
        "expected assertion failure",
        "expected explicit failure",
        "expected panic",
    ] {
        if bytes
            .windows(source_message.len())
            .any(|window| window == source_message.as_bytes())
        {
            return Err(BuildError::conformance(
                "catalog metadata",
                format!("catalog contains source body text {source_message:?}"),
            ));
        }
    }

    Ok(())
}

fn require_serial_metadata(bytes: &[u8]) -> Result<(), BuildError> {
    let (catalog, _) = decode_test_catalog(bytes).map_err(|error| {
        BuildError::conformance(
            "catalog metadata",
            format!("could not decode native test catalog: {error:?}"),
        )
    })?;

    let serial: Vec<_> = catalog
        .entries()
        .iter()
        .filter(|entry| entry.constraint() == TestExecutionConstraint::Serial)
        .collect();

    let names = serial
        .iter()
        .map(|entry| entry.identity().declaration().name().as_str())
        .collect::<Vec<_>>();

    if names
        != [
            "asynchronous_file_operations_preserve_data_and_metadata",
            "buffered_file_io_preserves_order_and_flushes",
            "files_and_directories_follow_the_portable_contract",
            "missing_files_report_the_portable_error_kind",
            "child_processes_capture_output_and_reap_cleanly",
            "string_operations",
        ]
    {
        return Err(BuildError::conformance(
            "catalog metadata",
            format!("the native catalog retained unexpected serial test constraints: {names:?}"),
        ));
    }

    Ok(())
}

fn run_tests(
    root: &Path,
    workspace: &Path,
    toolchain: &Path,
    target: NativeTarget,
    product: &str,
    test_arguments: &[&str],
) -> Result<Output, BuildError> {
    let executable = crate::workspace::cargo_target(root)
        .join("debug")
        .join(crate::native_toolchain::executable_name("bray"));

    let mut command = Command::new(&executable);

    command
        .current_dir(root)
        .env(CHILD_EXECUTABLE_ENVIRONMENT_VARIABLE, executable)
        .arg("--workspace")
        .arg(workspace)
        .arg("--toolchain-root")
        .arg(toolchain)
        .arg("--standard-library-source")
        .args([
            "--format",
            "json",
            "test",
            "--release",
            "--product",
            product,
            "--target",
        ])
        .arg(target_name(target))
        .args(test_arguments);

    command
        .output()
        .map_err(|error| BuildError::conformance("native command", error.to_string()))
}

fn require_success(operation: &'static str, output: &Output) -> Result<(), BuildError> {
    if output.status.success() {
        return Ok(());
    }

    Err(BuildError::conformance(
        operation,
        crate::command::failure(operation, output),
    ))
}

fn parse_report(operation: &'static str, output: &Output) -> Result<NativeTestReport, BuildError> {
    serde_json::from_slice(&output.stdout).map_err(|error| {
        BuildError::conformance(
            operation,
            format!(
                "could not decode JSON report: {error}; {}",
                crate::command::failure(operation, output)
            ),
        )
    })
}

fn tests(report: &NativeTestReport) -> Vec<&NativeTestResult> {
    report
        .products
        .iter()
        .flat_map(|product| product.tests.iter())
        .collect()
}

fn product_artifact(
    workspace: &Path,
    target: NativeTarget,
    product: &str,
    artifact: Artifact,
) -> Result<PathBuf, BuildError> {
    let directory = workspace
        .join("build")
        .join(target_name(target))
        .join("release")
        .join(PACKAGE_IDENTITY)
        .join(product);

    match artifact {
        Artifact::Catalog => Ok(directory.join(format!("{product}.braytests"))),
        Artifact::Executable => {
            let package = PackageIdentity::try_new(PACKAGE_IDENTITY).ok_or_else(|| {
                BuildError::conformance("native artifacts", "invalid package identity")
            })?;

            let product = ProductIdentity::try_new(package, product).ok_or_else(|| {
                BuildError::conformance("native artifacts", "invalid product identity")
            })?;

            super::artifact::resolve_executable(&directory, &product, "native artifacts")
        }
    }
}

fn read_artifact(path: &Path) -> Result<Vec<u8>, BuildError> {
    fs::read(path).map_err(|error| BuildError::read(path, error))
}

fn require_equal_artifact(name: &str, first: &[u8], second: &[u8]) -> Result<(), BuildError> {
    if first != second {
        return Err(BuildError::conformance(
            "native determinism",
            format!("{name} differs between sequential and parallel builds"),
        ));
    }

    Ok(())
}

fn copy_fixture(source: &Path, destination: &Path) -> Result<(), BuildError> {
    fs::create_dir_all(destination).map_err(|error| BuildError::write(destination, error))?;

    let mut entries = fs::read_dir(source)
        .map_err(|error| BuildError::read(source, error))?
        .map(|entry| entry.map_err(|error| BuildError::read(source, error)))
        .collect::<Result<Vec<_>, _>>()?;

    entries.sort_by_key(fs::DirEntry::file_name);

    for entry in entries {
        if entry.file_name() == "build" {
            continue;
        }

        let source = entry.path();
        let destination = destination.join(entry.file_name());

        if entry
            .file_type()
            .map_err(|error| BuildError::read(&source, error))?
            .is_dir()
        {
            copy_fixture(&source, &destination)?;
        } else {
            fs::copy(&source, &destination)
                .map_err(|error| BuildError::write(&destination, error))?;
        }
    }

    Ok(())
}

fn copy_standard_library_workspace(root: &Path, destination: &Path) -> Result<(), BuildError> {
    fs::create_dir_all(destination).map_err(|error| BuildError::write(destination, error))?;

    let source = root.join("standard-library");
    let manifest = source.join("bray-workspace.json");
    let destination_manifest = destination.join("bray-workspace.json");

    fs::copy(&manifest, &destination_manifest)
        .map_err(|error| BuildError::write(&destination_manifest, error))?;

    copy_fixture(&source.join("std"), &destination.join("std"))
}

const fn target_name(target: NativeTarget) -> &'static str {
    match target {
        NativeTarget::X86_64LinuxGnu => "x86-64-linux",
        NativeTarget::Aarch64LinuxGnu => "aarch64-linux",
        NativeTarget::X86_64WindowsMsvc => "x86-64-windows",
        NativeTarget::Aarch64WindowsMsvc => "aarch64-windows",
        NativeTarget::X86_64MacOs => "x86-64-macos",
        NativeTarget::Aarch64MacOs => "aarch64-macos",
    }
}

enum Artifact {
    Catalog,
    Executable,
}

#[derive(Clone, Copy)]
struct OutcomeCase {
    identity: &'static str,
    expectation: OutcomeExpectation,
    timed: bool,
}

impl OutcomeCase {
    const fn new(identity: &'static str, expectation: OutcomeExpectation) -> Self {
        Self {
            identity,
            expectation,
            timed: false,
        }
    }

    const fn timed(identity: &'static str, expectation: OutcomeExpectation) -> Self {
        Self {
            identity,
            expectation,
            timed: true,
        }
    }
}

#[derive(Clone, Copy)]
enum OutcomeExpectation {
    Assertion,
    Explicit,
    Panic,
    ReturnedError,
    TimedOut,
    ForcedTermination,
}

impl OutcomeExpectation {
    fn validate(self, identity: &str, outcome: &NativeOutcome) -> Result<(), BuildError> {
        let valid = match (self, outcome) {
            (Self::Assertion, NativeOutcome::AssertionFailure { source, message }) => {
                source.is_valid() && message.as_deref() == Some("expected assertion failure")
            }
            (Self::Explicit, NativeOutcome::ExplicitFailure { source, message }) => {
                source.is_valid() && message == "expected explicit failure"
            }
            (
                Self::Panic,
                NativeOutcome::Panicked {
                    cause,
                    source,
                    message,
                },
            ) => {
                cause == "message"
                    && source.is_some_and(NativeSourceAnchor::is_valid)
                    && message == "expected panic"
            }
            (
                Self::ReturnedError,
                NativeOutcome::ReturnedError {
                    error_type,
                    formatted_value,
                },
            ) => is_lowercase_sha256(error_type) && formatted_value.is_none(),
            (Self::TimedOut, NativeOutcome::TimedOut { nanoseconds }) => {
                *nanoseconds == 100_000_000
            }
            (
                Self::ForcedTermination,
                NativeOutcome::InfrastructureFailed {
                    failure,
                    detail_code,
                },
            ) => failure == "forced_termination" && detail_code.is_none(),
            _ => false,
        };

        if !valid {
            return Err(BuildError::conformance(
                "native outcomes",
                format!(
                    "{identity} produced an invalid {} payload: {outcome:?}",
                    outcome.kind()
                ),
            ));
        }

        Ok(())
    }
}

#[derive(Deserialize)]
struct NativeTestReport {
    format: u32,
    selection: NativeSelection,
    products: Vec<NativeProductReport>,
    summary: NativeSummary,
}

#[derive(Deserialize)]
struct NativeSelection {
    discovered: usize,
    selected: usize,
    filtered_out: usize,
}

#[derive(Deserialize)]
struct NativeSummary {
    passed: usize,
    failed: usize,
}

#[derive(Deserialize)]
struct NativeProductReport {
    package: String,
    product: String,
    catalog_digest: String,
    tests: Vec<NativeTestResult>,
}

#[derive(Deserialize)]
struct NativeTestResult {
    identity: String,
    outcome: NativeOutcome,
    stdout: NativeStream,
    stderr: NativeStream,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum NativeOutcome {
    Passed,
    ReturnedError {
        error_type: String,
        formatted_value: Option<String>,
    },
    ExplicitFailure {
        source: NativeSourceAnchor,
        message: String,
    },
    AssertionFailure {
        source: NativeSourceAnchor,
        message: Option<String>,
    },
    Panicked {
        cause: String,
        source: Option<NativeSourceAnchor>,
        message: String,
    },
    TimedOut {
        nanoseconds: u64,
    },
    Cancelled {
        #[serde(rename = "source")]
        _source: String,
    },
    InfrastructureFailed {
        failure: String,
        detail_code: Option<u64>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct NativeSourceAnchor {
    source: u32,
    start: u32,
    end: u32,
    version: u64,
}

impl NativeSourceAnchor {
    const fn is_valid(self) -> bool {
        self.source == 0 && self.start < self.end && self.version == 0
    }
}

impl NativeOutcome {
    const fn kind(&self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::ReturnedError { .. } => "returned_error",
            Self::ExplicitFailure { .. } => "explicit_failure",
            Self::AssertionFailure { .. } => "assertion_failure",
            Self::Panicked { .. } => "panicked",
            Self::TimedOut { .. } => "timed_out",
            Self::Cancelled { .. } => "cancelled",
            Self::InfrastructureFailed { .. } => "infrastructure_failed",
        }
    }
}

#[derive(Deserialize)]
struct NativeStream {
    policy: String,
    bytes: Vec<u8>,
    truncated: bool,
    discarded_byte_count: u64,
    failure: Option<serde_json::Value>,
}
