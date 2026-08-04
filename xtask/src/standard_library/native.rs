use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use bray_target::{NativeTarget, TargetOutputKind, TargetOutputName};
use serde::Deserialize;

use super::command::BuildError;

const PACKAGE_IDENTITY: &str = "bray.standard_library_conformance";
const STANDARD_PRODUCT: &str = "standard";
const OUTCOME_PRODUCT: &str = "outcomes";

pub(super) fn test() -> Result<(), BuildError> {
    let root = crate::workspace::root().map_err(BuildError::Workspace)?;

    let target = NativeTarget::current()
        .ok_or_else(|| BuildError::conformance("native", "the compiler host is not supported"))?;

    crate::native_toolchain::build_compiler(&root)
        .map_err(|error| BuildError::conformance("native", error))?;

    let directory = tempfile::Builder::new()
        .prefix("bray-standard-library-native-")
        .tempdir()
        .map_err(BuildError::TemporaryDirectory)?;

    let runtime = directory.path().join("runtime");

    fs::create_dir(&runtime).map_err(|error| BuildError::write(&runtime, error))?;

    let runtime = crate::runtime_artifact::build_for_readiness(target, &runtime)
        .map_err(|error| BuildError::conformance("native runtime", error))?;

    let toolchain = directory.path().join("toolchain");

    crate::native_toolchain::assemble(&root, target, &runtime, &toolchain)
        .map_err(|error| BuildError::conformance("native toolchain", error))?;

    let workspace = directory.path().join("workspace");

    copy_fixture(&root.join("standard-library/conformance"), &workspace)?;
    audit_standard_product(&root, &workspace, &toolchain, target)?;

    audit_outcomes(&root, &workspace, &toolchain, target)
}

fn audit_standard_product(
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
        STANDARD_PRODUCT,
        &["--sequential"],
    )?;

    require_success("sequential execution", &sequential)?;

    let sequential_report = parse_report("sequential execution", &sequential)?;

    validate_standard_report(&sequential_report)?;

    let executable = product_artifact(workspace, target, STANDARD_PRODUCT, Artifact::Executable)?;
    let catalog = product_artifact(workspace, target, STANDARD_PRODUCT, Artifact::Catalog)?;
    let sequential_executable = read_artifact(&executable)?;
    let sequential_catalog = read_artifact(&catalog)?;

    let parallel = run_tests(
        root,
        workspace,
        toolchain,
        target,
        STANDARD_PRODUCT,
        &["--jobs", "2"],
    )?;

    require_success("parallel execution", &parallel)?;

    let parallel_report = parse_report("parallel execution", &parallel)?;

    validate_standard_report(&parallel_report)?;
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
        STANDARD_PRODUCT,
        &["standard_output"],
    )?;

    require_success("filtered execution", &filtered)?;

    let filtered_report = parse_report("filtered execution", &filtered)?;

    require_selection(&filtered_report, 6, 2, 4)?;

    let tests = tests(&filtered_report);

    if tests.len() != 2
        || tests
            .iter()
            .any(|test| !test.identity.contains("standard_output"))
    {
        return Err(BuildError::conformance(
            "native filtering",
            "the standard_output filter did not select exactly the two I/O fixtures",
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
    let output = run_tests(
        root,
        workspace,
        toolchain,
        target,
        OUTCOME_PRODUCT,
        &["--sequential", "--timeout-ms", "100"],
    )?;

    if output.status.success() {
        return Err(BuildError::conformance(
            "native outcomes",
            "failure fixtures unexpectedly succeeded",
        ));
    }

    let report = parse_report("native outcomes", &output)?;

    require_selection(&report, 5, 5, 0)?;

    let catalog = product_artifact(workspace, target, OUTCOME_PRODUCT, Artifact::Catalog)?;

    require_catalog_separation(&read_artifact(&catalog)?)?;

    let expected = [
        ("assertion_failure", "assertion_failure"),
        ("explicit_failure", "explicit_failure"),
        ("panic_failure", "panicked"),
        ("recoverable_error", "returned_error"),
        ("timeout_requires_forced_termination", "forced_termination"),
    ];

    for (identity, outcome) in expected {
        let Some(test) = tests(&report)
            .into_iter()
            .find(|test| test.identity.ends_with(identity))
        else {
            return Err(BuildError::conformance(
                "native outcomes",
                format!("missing fixture {identity}"),
            ));
        };

        let actual = match &test.outcome {
            NativeOutcome::InfrastructureFailed { failure } => failure.as_str(),
            outcome => outcome.kind(),
        };

        if actual != outcome {
            return Err(BuildError::conformance(
                "native outcomes",
                format!("fixture {identity} reported {actual} instead of {outcome}"),
            ));
        }
    }

    Ok(())
}

fn validate_standard_report(report: &NativeTestReport) -> Result<(), BuildError> {
    require_selection(report, 6, 6, 0)?;

    if report.summary.passed != 6 || report.summary.failed != 0 {
        return Err(BuildError::conformance(
            "native execution",
            "the standard product did not report six passing tests",
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

        let expected_output = if test
            .identity
            .ends_with("asynchronous_standard_output_is_captured")
        {
            b"captured-async-output".as_slice()
        } else if test.identity.ends_with("standard_output_is_captured") {
            b"captured-standard-output".as_slice()
        } else {
            &[]
        };

        require_stream(&test.identity, "stdout", &test.stdout, expected_output)?;
        require_stream(&test.identity, "stderr", &test.stderr, &[])?;
    }

    Ok(())
}

fn require_stream(
    identity: &str,
    name: &str,
    stream: &NativeStream,
    expected: &[u8],
) -> Result<(), BuildError> {
    if stream.bytes != expected
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

fn run_tests(
    root: &Path,
    workspace: &Path,
    toolchain: &Path,
    target: NativeTarget,
    product: &str,
    test_arguments: &[&str],
) -> Result<Output, BuildError> {
    let executable = root
        .join("target")
        .join("debug")
        .join(crate::native_toolchain::executable_name("bray"));

    let mut command = Command::new(executable);

    command
        .current_dir(root)
        .arg("--workspace")
        .arg(workspace)
        .arg("--toolchain-root")
        .arg(toolchain)
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

    let name = match artifact {
        Artifact::Catalog => format!("{product}.braytests"),
        Artifact::Executable => {
            TargetOutputName::for_native(target.object_format(), TargetOutputKind::Executable)
                .file_name(product)
                .ok_or_else(|| {
                    BuildError::conformance("native artifacts", "invalid executable output name")
                })?
        }
    };

    Ok(directory.join(name))
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

    entries.sort_by_key(std::fs::DirEntry::file_name);

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

#[derive(Deserialize)]
struct NativeTestReport {
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
    tests: Vec<NativeTestResult>,
}

#[derive(Deserialize)]
struct NativeTestResult {
    identity: String,
    outcome: NativeOutcome,
    stdout: NativeStream,
    stderr: NativeStream,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum NativeOutcome {
    Passed,
    ReturnedError,
    ExplicitFailure,
    AssertionFailure,
    Panicked,
    TimedOut,
    Cancelled,
    InfrastructureFailed { failure: String },
}

impl NativeOutcome {
    const fn kind(&self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::ReturnedError => "returned_error",
            Self::ExplicitFailure => "explicit_failure",
            Self::AssertionFailure => "assertion_failure",
            Self::Panicked => "panicked",
            Self::TimedOut => "timed_out",
            Self::Cancelled => "cancelled",
            Self::InfrastructureFailed { .. } => "infrastructure_failed",
        }
    }
}

#[derive(Deserialize)]
struct NativeStream {
    bytes: Vec<u8>,
    truncated: bool,
    discarded_byte_count: u64,
    failure: Option<serde_json::Value>,
}
