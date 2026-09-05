// rust-style: allow(module-too-large, reason = "the native standard-library audit keeps one end-to-end build and execution contract")

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use bray_base::is_lowercase_hex;
use bray_symbols::TestExecutionConstraint;
use bray_target::{NativeTarget, TargetOutputKind, TargetOutputName};
use bray_test_protocol::{TestBatchPlan, TestBatchRequest, decode_test_catalog};
use serde::Deserialize;

use super::command::BuildError;

const PACKAGE_IDENTITY: &str = "std";
const API_PRODUCT: &str = "api";
const OUTCOME_PRODUCT: &str = "outcomes";
const CHILD_EXECUTABLE_ENVIRONMENT_VARIABLE: &str = "BRAY_STANDARD_LIBRARY_TEST_EXECUTABLE";
const API_TEST_COUNT: usize = 146;
const API_FILTERED_TEST_COUNT: usize = 3;
const CONCURRENCY_MODEL_TEST_COUNT: usize = 7;
const CONCURRENCY_STRESS_TEST_COUNT: usize = 5;
const OUTCOME_CASES: [OutcomeCase; 17] = [
    OutcomeCase::new(
        "assertion-failure",
        "assertion_failure",
        OutcomeExpectation::Assertion,
    ),
    OutcomeCase::new(
        "explicit-failure",
        "explicit_failure",
        OutcomeExpectation::Explicit,
    ),
    OutcomeCase::new(
        "assert-ok-rejects-error",
        "assert_ok_rejects_an_error",
        OutcomeExpectation::Panic {
            cause: "explicit_failure",
            message: "expected Result.Ok",
            source_available: false,
        },
    ),
    OutcomeCase::new(
        "assert-error-rejects-success",
        "assert_error_rejects_success",
        OutcomeExpectation::Panic {
            cause: "explicit_failure",
            message: "expected Result.Error",
            source_available: false,
        },
    ),
    OutcomeCase::new(
        "assert-present-rejects-absence",
        "assert_present_rejects_absence",
        OutcomeExpectation::Panic {
            cause: "explicit_failure",
            message: "expected a present nullable value",
            source_available: false,
        },
    ),
    OutcomeCase::new(
        "assert-absent-rejects-presence",
        "assert_absent_rejects_presence",
        OutcomeExpectation::Panic {
            cause: "explicit_failure",
            message: "expected an absent nullable value",
            source_available: false,
        },
    ),
    OutcomeCase::new(
        "assert-completed-rejects-cancellation",
        "assert_completed_rejects_cancellation",
        OutcomeExpectation::Panic {
            cause: "explicit_failure",
            message: "expected RunResult.Completed",
            source_available: false,
        },
    ),
    OutcomeCase::new(
        "assert-panicked-rejects-completion",
        "assert_panicked_rejects_completion",
        OutcomeExpectation::Panic {
            cause: "explicit_failure",
            message: "expected RunResult.Panicked",
            source_available: false,
        },
    ),
    OutcomeCase::new(
        "assert-cancelled-rejects-completion",
        "assert_cancelled_rejects_completion",
        OutcomeExpectation::Panic {
            cause: "explicit_failure",
            message: "expected RunResult.Cancelled",
            source_available: false,
        },
    ),
    OutcomeCase::new(
        "assert-not-completed-rejects-completion",
        "assert_not_completed_rejects_completion",
        OutcomeExpectation::Panic {
            cause: "explicit_failure",
            message: "expected RunResult.Panicked or RunResult.Cancelled",
            source_available: false,
        },
    ),
    OutcomeCase::new(
        "invalid-allocation-alignment",
        "invalid_allocation_alignment",
        OutcomeExpectation::Panic {
            cause: "assertion",
            message: "memory allocation alignment must be a nonzero power of two",
            source_available: false,
        },
    ),
    OutcomeCase::new(
        "allocation-size-overflow",
        "allocation_size_overflow",
        OutcomeExpectation::Panic {
            cause: "assertion",
            message: "memory allocation size exceeds the target address range",
            source_available: false,
        },
    ),
    OutcomeCase::new(
        "panic-failure",
        "panic_failure",
        OutcomeExpectation::Panic {
            cause: "message",
            message: "expected panic",
            source_available: true,
        },
    ),
    OutcomeCase::new(
        "once-indirect-reentry",
        "once_indirect_reentry_panics",
        OutcomeExpectation::Panic {
            cause: "message",
            message: "Once initialization reentered",
            source_available: false,
        },
    ),
    OutcomeCase::new(
        "recoverable-error",
        "recoverable_error",
        OutcomeExpectation::ReturnedError,
    ),
    OutcomeCase::timed(
        "timeout-observes-cancellation",
        "timeout_observes_cancellation",
        OutcomeExpectation::TimedOut,
    ),
    OutcomeCase::timed(
        "timeout-requires-forced-termination",
        "timeout_requires_forced_termination",
        OutcomeExpectation::ForcedTermination,
    ),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TestPart {
    ProviderRetention,
    Interoperability,
    Api,
    Outcomes,
}

impl TestPart {
    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "provider-retention" => Some(Self::ProviderRetention),
            "interoperability" => Some(Self::Interoperability),
            "api" => Some(Self::Api),
            "outcomes" => Some(Self::Outcomes),
            _ => None,
        }
    }
}

pub(super) fn test(parts: &[TestPart], profile_output: Option<&Path>) -> Result<(), BuildError> {
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

    let runtime = crate::progress::run("Building the native runtime artifacts", || {
        crate::runtime_artifact::build_for_readiness(target, &runtime)
    })
    .map_err(|error| BuildError::conformance("native runtime", error))?;

    let toolchain = directory.join("toolchain");

    crate::progress::run("Assembling the native test toolchain", || {
        crate::native_toolchain::assemble(&root, target, &runtime, &toolchain)
    })
    .map_err(|error| BuildError::conformance("native toolchain", error))?;

    if selected(parts, TestPart::ProviderRetention) {
        crate::progress::run("Auditing native provider retention", || {
            super::provider_retention::audit(&root, directory, &toolchain, target)
        })?;
    }

    if selected(parts, TestPart::Interoperability) {
        crate::progress::run("Auditing native interoperability", || {
            super::interoperability::audit(&root, directory, &toolchain, &runtime, target)
        })?;
    }

    if selected(parts, TestPart::Api) || selected(parts, TestPart::Outcomes) {
        let workspace = directory.join("workspace");

        crate::progress::run("Preparing the standard library test workspace", || {
            copy_standard_library_workspace(&root, &workspace)
        })?;

        if selected(parts, TestPart::Api) {
            crate::progress::run(
                &format!(
                    "Running native standard library API tests ({API_TEST_COUNT} tests, 7 plans)"
                ),
                || audit_api(&root, &workspace, &toolchain, target, profile_output),
            )?;
        }

        if selected(parts, TestPart::Outcomes) {
            crate::progress::run(
                &format!(
                    "Checking native test outcomes ({} cases)",
                    OUTCOME_CASES.len()
                ),
                || audit_outcomes(&root, &workspace, &toolchain, target, profile_output),
            )?;
        }
    }

    Ok(())
}

fn selected(parts: &[TestPart], part: TestPart) -> bool {
    parts.is_empty() || parts.contains(&part)
}

fn audit_api(
    root: &Path,
    workspace: &Path,
    toolchain: &Path,
    target: NativeTarget,
    profile_output: Option<&Path>,
) -> Result<(), BuildError> {
    let request = test_batch_request([
        test_batch_plan(
            "startup-byte-buffer",
            ["byte_buffer_mutation"],
            1,
            Some(1000),
        )?,
        test_batch_plan(
            "startup-output-lock",
            ["repeated_standard_output_locks_are_released"],
            1,
            Some(1000),
        )?,
        test_batch_plan("api-sequential", std::iter::empty::<&str>(), 1, Some(5000))?,
        test_batch_plan("api-parallel", std::iter::empty::<&str>(), 2, Some(5000))?,
        test_batch_plan("api-filtered", ["standard_output"], 2, Some(1000))?,
        test_batch_plan("concurrency-model", ["concurrency_model_"], 2, Some(5000))?,
        test_batch_plan("concurrency-stress", ["concurrency_stress_"], 2, Some(5000))?,
    ])?;

    let output = run_test_batch(
        root,
        workspace,
        toolchain,
        target,
        API_PRODUCT,
        &request,
        profile_output,
        "api",
        false,
    )?;

    require_success("native API batch", &output)?;

    let batch = parse_batch_report("native API batch", &output, &request)?;
    let byte_buffer = batch.report("startup-byte-buffer")?;
    let output_lock = batch.report("startup-output-lock")?;
    let sequential = batch.report("api-sequential")?;
    let parallel = batch.report("api-parallel")?;
    let filtered = batch.report("api-filtered")?;
    let concurrency_model = batch.report("concurrency-model")?;
    let concurrency_stress = batch.report("concurrency-stress")?;

    let catalog = product_catalog(workspace, target, API_PRODUCT)?;
    let catalog = read_artifact(&catalog)?;

    require_startup_report(
        "startup-byte-buffer",
        byte_buffer,
        "byte_buffer_mutation",
        &[],
    )?;

    require_startup_report(
        "startup-output-lock",
        output_lock,
        "repeated_standard_output_locks_are_released",
        b"first-lock|second-lock",
    )?;

    validate_api_report("api-sequential", sequential)?;
    validate_api_report("api-parallel", parallel)?;
    require_stable_order(sequential, parallel)?;
    require_serial_metadata(&catalog)?;

    require_selection(
        filtered,
        API_TEST_COUNT,
        API_FILTERED_TEST_COUNT,
        API_TEST_COUNT - API_FILTERED_TEST_COUNT,
    )?;

    let tests = tests(filtered);

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

    require_concurrency_selection(
        concurrency_model,
        "concurrency_model_",
        CONCURRENCY_MODEL_TEST_COUNT,
    )?;

    require_concurrency_selection(
        concurrency_stress,
        "concurrency_stress_",
        CONCURRENCY_STRESS_TEST_COUNT,
    )?;

    audit_no_build_rerun(root, workspace, toolchain, target)?;

    Ok(())
}

fn audit_no_build_rerun(
    root: &Path,
    workspace: &Path,
    toolchain: &Path,
    target: NativeTarget,
) -> Result<(), BuildError> {
    let request = test_batch_request([test_batch_plan(
        "no-build",
        ["byte_buffer_mutation"],
        1,
        Some(1000),
    )?])?;

    let output = run_test_batch(
        root,
        workspace,
        toolchain,
        target,
        API_PRODUCT,
        &request,
        None,
        "no-build",
        true,
    )?;

    require_success("native no-build rerun", &output)?;

    let batch = parse_batch_report("native no-build rerun", &output, &request)?;

    if !batch.build.reused
        || batch.build.compilation
        || batch.build.emission
        || batch.build.linking
        || batch.build.products.len() != 1
    {
        return Err(BuildError::conformance(
            "native no-build rerun",
            "the no-build report did not identify one fully reused retained generation",
        ));
    }

    require_startup_report(
        "no-build",
        batch.report("no-build")?,
        "byte_buffer_mutation",
        &[],
    )
}

fn require_concurrency_selection(
    report: &NativeTestReport,
    identity_fragment: &str,
    expected: usize,
) -> Result<(), BuildError> {
    require_product(report, API_PRODUCT)?;
    require_selection(report, API_TEST_COUNT, expected, API_TEST_COUNT - expected)?;

    let tests = tests(report);

    if tests.len() != expected
        || tests
            .iter()
            .any(|test| !test.identity.contains(identity_fragment))
    {
        return Err(BuildError::conformance(
            "native concurrency",
            format!("the {identity_fragment} filter did not select exactly {expected} fixtures"),
        ));
    }

    Ok(())
}

fn require_startup_report(
    plan: &str,
    report: &NativeTestReport,
    identity: &str,
    expected_output: &[u8],
) -> Result<(), BuildError> {
    require_product(report, API_PRODUCT)?;
    require_selection(report, API_TEST_COUNT, 1, API_TEST_COUNT - 1)?;

    let tests = tests(report);

    let [test] = tests.as_slice() else {
        return Err(BuildError::conformance(
            "focused test-host startup",
            format!("{identity} did not produce one result"),
        ));
    };

    require_stream(
        plan,
        &test.identity,
        "stdout",
        &test.stdout,
        expected_output,
    )?;

    require_stream(plan, &test.identity, "stderr", &test.stderr, &[])
}

fn audit_outcomes(
    root: &Path,
    workspace: &Path,
    toolchain: &Path,
    target: NativeTarget,
    profile_output: Option<&Path>,
) -> Result<(), BuildError> {
    let request = outcome_batch_request()?;

    let output = run_test_batch(
        root,
        workspace,
        toolchain,
        target,
        OUTCOME_PRODUCT,
        &request,
        profile_output,
        "outcomes",
        false,
    )?;

    if output.status.success() {
        return Err(BuildError::conformance(
            "native outcomes",
            "the failing outcome batch unexpectedly succeeded",
        ));
    }

    let batch = parse_batch_report("native outcomes", &output, &request)?;

    for case in OUTCOME_CASES {
        audit_outcome(batch.report(case.plan_identity)?, case)?;
    }

    let catalog = product_catalog(workspace, target, OUTCOME_PRODUCT)?;

    require_catalog_separation(&read_artifact(&catalog)?)
}

fn outcome_batch_request() -> Result<TestBatchRequest, BuildError> {
    let plans = OUTCOME_CASES
        .iter()
        .map(|case| {
            test_batch_plan(
                case.plan_identity,
                [case.test_identity],
                1,
                case.timed.then_some(100),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    test_batch_request(plans)
}

fn audit_outcome(report: &NativeTestReport, case: OutcomeCase) -> Result<(), BuildError> {
    require_product(report, OUTCOME_PRODUCT)?;
    require_selection(report, OUTCOME_CASES.len(), 1, OUTCOME_CASES.len() - 1)?;

    if report.summary.passed != 0 || report.summary.failed != 1 {
        return Err(BuildError::conformance(
            "native outcomes",
            format!("{} did not report one failed test", case.test_identity),
        ));
    }

    let results = tests(report);

    let [test] = results.as_slice() else {
        return Err(BuildError::conformance(
            "native outcomes",
            format!("{} did not produce one result", case.test_identity),
        ));
    };

    if !test.identity.ends_with(case.test_identity) {
        return Err(BuildError::conformance(
            "native outcomes",
            format!("{} selected an unrelated result", case.test_identity),
        ));
    }

    case.expectation
        .validate(case.test_identity, &test.outcome)?;

    require_stream(
        case.plan_identity,
        &test.identity,
        "stdout",
        &test.stdout,
        &[],
    )?;

    require_stream(
        case.plan_identity,
        &test.identity,
        "stderr",
        &test.stderr,
        &[],
    )
}

fn validate_api_report(plan: &str, report: &NativeTestReport) -> Result<(), BuildError> {
    require_product(report, API_PRODUCT)?;
    require_selection(report, API_TEST_COUNT, API_TEST_COUNT, 0)?;

    let tests = tests(report);

    if let Some(test) = tests
        .iter()
        .find(|test| !matches!(test.outcome, NativeOutcome::Passed))
    {
        return Err(BuildError::conformance(
            "native execution",
            format!("{} did not pass: {:?}", test.identity, test.outcome),
        ));
    }

    if report.summary.passed != API_TEST_COUNT || report.summary.failed != 0 {
        return Err(BuildError::conformance(
            "native execution",
            format!("the API product did not report {API_TEST_COUNT} passing tests"),
        ));
    }

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

        require_stream(
            plan,
            &test.identity,
            "stdout",
            &test.stdout,
            expected_output,
        )?;

        require_stream(plan, &test.identity, "stderr", &test.stderr, expected_error)?;
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
    plan: &str,
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
            format!(
                "the {plan} plan test {identity} produced an invalid {name} stream, expected bytes {}, actual policy {:?}, bytes {}, truncated {}, discarded byte count {}, failure {:?}",
                bytes_preview(expected),
                stream.policy,
                bytes_preview(&stream.bytes),
                stream.truncated,
                stream.discarded_byte_count,
                stream.failure,
            ),
        ));
    }

    Ok(())
}

fn bytes_preview(bytes: &[u8]) -> String {
    const MAXIMUM_PREVIEW_LENGTH: usize = 64;

    if bytes.len() <= MAXIMUM_PREVIEW_LENGTH {
        return format!("{bytes:?}");
    }

    format!(
        "{:?} followed by {} more bytes",
        &bytes[..MAXIMUM_PREVIEW_LENGTH],
        bytes.len() - MAXIMUM_PREVIEW_LENGTH,
    )
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
            "child_processes_accept_an_empty_environment",
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

fn test_batch_plan(
    identity: &str,
    filters: impl IntoIterator<Item = impl AsRef<str>>,
    maximum_concurrency: usize,
    timeout_milliseconds: Option<u64>,
) -> Result<TestBatchPlan, BuildError> {
    TestBatchPlan::try_new(
        identity,
        filters.into_iter().map(|filter| filter.as_ref().to_owned()),
        maximum_concurrency,
        timeout_milliseconds,
    )
    .map_err(|error| BuildError::conformance("native test batch", format!("{error:?}")))
}

fn test_batch_request(
    plans: impl IntoIterator<Item = TestBatchPlan>,
) -> Result<TestBatchRequest, BuildError> {
    TestBatchRequest::try_new(plans)
        .map_err(|error| BuildError::conformance("native test batch", format!("{error:?}")))
}

fn run_test_batch(
    root: &Path,
    workspace: &Path,
    toolchain: &Path,
    target: NativeTarget,
    product: &str,
    request: &TestBatchRequest,
    profile_output: Option<&Path>,
    profile_identity: &str,
    no_build: bool,
) -> Result<Output, BuildError> {
    let request_path = workspace.join(format!(".{product}-test-batch.json"));

    let request_bytes = serde_json::to_vec(request).map_err(|error| {
        BuildError::conformance(
            "native test batch",
            format!("could not encode request: {error}"),
        )
    })?;

    fs::write(&request_path, request_bytes)
        .map_err(|error| BuildError::write(&request_path, error))?;

    let executable = crate::native_toolchain::compiler_executable(root, "bray");

    let mut command = Command::new(&executable);

    command
        .current_dir(root)
        .env(CHILD_EXECUTABLE_ENVIRONMENT_VARIABLE, executable)
        .arg("--workspace")
        .arg(workspace)
        .arg("--toolchain-root")
        .arg(toolchain)
        .arg("--standard-library-source");

    if let Some(profile_output) = profile_output {
        command
            .args(["--profile", "trace", "--profile-output"])
            .arg(profile_output.join(profile_identity));
    }

    command.args(["--format", "json", "test"]);

    if no_build {
        command.arg("--no-build");
    }

    let target_identity = target.identity();

    let native_links = super::os_bindings::native_links(&target_identity)
        .map_err(|error| BuildError::conformance("native link inputs", error))?;

    for input in native_links {
        command.arg("--native-link-input").arg(format!(
            "{}={}",
            input.name(),
            input.kind().as_str()
        ));
    }

    command
        .args(["--release", "--product", product, "--target"])
        .arg(target_name(target))
        .arg("--batch-request")
        .arg(&request_path);

    crate::command::output_with_streamed_stderr(&mut command)
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

fn parse_batch_report(
    operation: &'static str,
    output: &Output,
    request: &TestBatchRequest,
) -> Result<NativeTestBatchReport, BuildError> {
    let report =
        serde_json::from_slice::<NativeTestBatchReport>(&output.stdout).map_err(|error| {
            BuildError::conformance(
                operation,
                format!(
                    "could not decode JSON report: {error}. {}",
                    crate::command::failure(operation, output)
                ),
            )
        })?;

    report.validate(request)?;

    Ok(report)
}

fn tests(report: &NativeTestReport) -> Vec<&NativeTestResult> {
    report
        .products
        .iter()
        .flat_map(|product| product.tests.iter())
        .collect()
}

fn product_catalog(
    workspace: &Path,
    target: NativeTarget,
    product: &str,
) -> Result<PathBuf, BuildError> {
    let destination = native_product_destination(workspace, target)?;

    let name = TargetOutputName::for_native(target.object_format(), TargetOutputKind::TestCatalog)
        .file_name(product)
        .ok_or_else(|| BuildError::conformance("native test catalog", "invalid product name"))?;

    Ok(destination.directory().join(name))
}

fn native_product_destination(
    workspace: &Path,
    target: NativeTarget,
) -> Result<bray_emitter::ManagedFilesystemDestination, BuildError> {
    let directory = bray_emitter::ManagedOutputDirectory::try_new(format!(
        "{}/release/{PACKAGE_IDENTITY}",
        target_name(target)
    ))
    .ok_or_else(|| BuildError::conformance("native artifacts", "invalid output directory"))?;

    Ok(bray_emitter::ManagedFilesystemDestination::new(
        workspace.join("build"),
        directory,
    ))
}

fn read_artifact(path: &Path) -> Result<Vec<u8>, BuildError> {
    fs::read(path).map_err(|error| BuildError::read(path, error))
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

#[derive(Clone, Copy)]
struct OutcomeCase {
    plan_identity: &'static str,
    test_identity: &'static str,
    expectation: OutcomeExpectation,
    timed: bool,
}

impl OutcomeCase {
    const fn new(
        plan_identity: &'static str,
        test_identity: &'static str,
        expectation: OutcomeExpectation,
    ) -> Self {
        Self {
            plan_identity,
            test_identity,
            expectation,
            timed: false,
        }
    }

    const fn timed(
        plan_identity: &'static str,
        test_identity: &'static str,
        expectation: OutcomeExpectation,
    ) -> Self {
        Self {
            plan_identity,
            test_identity,
            expectation,
            timed: true,
        }
    }
}

#[derive(Clone, Copy)]
enum OutcomeExpectation {
    Assertion,
    Explicit,
    Panic {
        cause: &'static str,
        message: &'static str,
        source_available: bool,
    },
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
                Self::Panic {
                    cause: expected_cause,
                    message: expected_message,
                    source_available,
                },
                NativeOutcome::Panicked {
                    cause,
                    source,
                    message,
                },
            ) => {
                cause == expected_cause
                    && match (source_available, source) {
                        (true, Some(source)) => source.is_valid(),
                        (false, None) => true,
                        _ => false,
                    }
                    && message == expected_message
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
struct NativeTestBatchReport {
    format: u32,
    build: NativeTestBuildProvenance,
    plans: Vec<NativeTestBatchPlanReport>,
}

impl NativeTestBatchReport {
    fn validate(&self, request: &TestBatchRequest) -> Result<(), BuildError> {
        if self.format != 1 || self.plans.len() != request.plans().len() {
            return Err(BuildError::conformance(
                "native test batch",
                "the report header does not match the request",
            ));
        }

        for (actual, expected) in self.plans.iter().zip(request.plans()) {
            let report_succeeded = actual.report.summary.failed == 0;

            if actual.identity != expected.identity()
                || actual.report.format != 1
                || actual.succeeded != report_succeeded
            {
                return Err(BuildError::conformance(
                    "native test batch",
                    format!("the {} plan report is inconsistent", expected.identity()),
                ));
            }
        }

        Ok(())
    }

    fn report(&self, identity: &str) -> Result<&NativeTestReport, BuildError> {
        self.plans
            .iter()
            .find(|plan| plan.identity == identity)
            .map(|plan| &plan.report)
            .ok_or_else(|| {
                BuildError::conformance(
                    "native test batch",
                    format!("the {identity} plan report is absent"),
                )
            })
    }
}

#[derive(Deserialize)]
struct NativeTestBatchPlanReport {
    identity: String,
    succeeded: bool,
    report: NativeTestReport,
}

#[derive(Deserialize)]
struct NativeTestReport {
    format: u32,
    #[serde(rename = "build")]
    _build: NativeTestBuildProvenance,
    selection: NativeSelection,
    products: Vec<NativeProductReport>,
    summary: NativeSummary,
}

#[derive(Deserialize)]
struct NativeTestBuildProvenance {
    reused: bool,
    compilation: bool,
    emission: bool,
    linking: bool,
    products: Vec<NativeTestProductGeneration>,
}

#[derive(Deserialize)]
struct NativeTestProductGeneration {
    #[serde(rename = "product")]
    _product: String,
    #[serde(rename = "generation")]
    _generation: String,
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use bray_target::NativeTarget;

    #[test]
    fn native_product_destination_matches_project_build_output() {
        let destination = super::native_product_destination(
            Path::new("workspace"),
            NativeTarget::X86_64WindowsMsvc,
        )
        .unwrap_or_else(|error| panic!("native product destination must build: {error:?}"));

        assert_eq!(destination.root(), Path::new("workspace/build"));

        assert_eq!(
            destination
                .relative_directory()
                .unwrap_or_else(|| panic!("native product destination must be namespaced"))
                .as_str(),
            "x86-64-windows/release/std"
        );
    }

    #[test]
    fn native_batch_request_retains_ordered_one_build_execution_plans() {
        let request = super::test_batch_request([
            super::test_batch_plan("sequential", std::iter::empty::<&str>(), 1, Some(1000))
                .unwrap_or_else(|error| panic!("sequential plan must build: {error:?}")),
            super::test_batch_plan("filtered", ["output"], 2, None)
                .unwrap_or_else(|error| panic!("filtered plan must build: {error:?}")),
        ])
        .unwrap_or_else(|error| panic!("batch request must build: {error:?}"));

        let encoded = serde_json::to_vec(&request)
            .unwrap_or_else(|error| panic!("batch request must encode: {error:?}"));

        let decoded = serde_json::from_slice::<bray_test_protocol::TestBatchRequest>(&encoded)
            .unwrap_or_else(|error| panic!("batch request must decode: {error:?}"));

        let identities = decoded
            .plans()
            .iter()
            .map(bray_test_protocol::TestBatchPlan::identity)
            .collect::<Vec<_>>();

        assert_eq!(identities, ["sequential", "filtered"]);
        assert_eq!(decoded.plans()[0].maximum_concurrency(), 1);
        assert_eq!(decoded.plans()[1].filters(), ["output"]);
    }

    #[test]
    fn outcome_batch_uses_portable_plan_identities_and_exact_source_filters() {
        let request = super::outcome_batch_request()
            .unwrap_or_else(|error| panic!("outcome batch must build: {error:?}"));

        for (plan, case) in request.plans().iter().zip(super::OUTCOME_CASES) {
            assert_eq!(plan.identity(), case.plan_identity);
            assert_eq!(plan.filters(), [case.test_identity]);
        }
    }

    #[test]
    fn batch_report_rejects_a_child_report_with_an_unknown_format() {
        let request = super::test_batch_request([super::test_batch_plan(
            "plan",
            std::iter::empty::<&str>(),
            1,
            None,
        )
        .unwrap_or_else(|error| panic!("plan must build: {error:?}"))])
        .unwrap_or_else(|error| panic!("batch request must build: {error:?}"));

        let report = serde_json::from_value::<super::NativeTestBatchReport>(serde_json::json!({
            "format": 1,
            "build": {
                "reused": false,
                "compilation": true,
                "emission": true,
                "linking": true,
                "products": []
            },
            "plans": [{
                "identity": "plan",
                "succeeded": true,
                "report": {
                    "format": 3,
                    "build": {
                        "reused": false,
                        "compilation": true,
                        "emission": true,
                        "linking": true,
                        "products": []
                    },
                    "selection": { "discovered": 0, "selected": 0, "filtered_out": 0 },
                    "products": [],
                    "summary": { "passed": 0, "failed": 0 }
                }
            }]
        }))
        .unwrap_or_else(|error| panic!("test report must decode: {error:?}"));

        assert!(report.validate(&request).is_err());
    }

    #[test]
    fn focused_startup_report_requires_the_api_product_identity() {
        let report = super::NativeTestReport {
            format: 1,
            _build: super::NativeTestBuildProvenance {
                reused: false,
                compilation: true,
                emission: true,
                linking: true,
                products: Vec::new(),
            },
            selection: super::NativeSelection {
                discovered: super::API_TEST_COUNT,
                selected: 1,
                filtered_out: super::API_TEST_COUNT - 1,
            },
            products: vec![super::NativeProductReport {
                package: super::PACKAGE_IDENTITY.to_owned(),
                product: super::OUTCOME_PRODUCT.to_owned(),
                catalog_digest: "0".repeat(64),
                tests: Vec::new(),
            }],
            summary: super::NativeSummary {
                passed: 1,
                failed: 0,
            },
        };

        let error = super::require_startup_report("focused", &report, "fixture", &[])
            .expect_err("the outcome product must not satisfy an API startup report");

        assert!(error.to_string().contains("report identity"));
    }
}
