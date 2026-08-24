use serde::Serialize;

use super::command::DiagnosticProjectCommandFailureJson;
use crate::output::path_to_output_string;

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticProfileContextJson {
    package: String,
    product: String,
    target: String,
}

impl DiagnosticProfileContextJson {
    fn from_context(context: &bray_diagnostics::DiagnosticProfileContext) -> Self {
        Self {
            package: context.package.clone(),
            product: context.product.clone(),
            target: context.target.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticProfileComparisonProblemJson {
    Context {
        before: DiagnosticProfileContextJson,
        after: DiagnosticProfileContextJson,
    },
    Descriptor {
        kind: &'static str,
        id: u16,
    },
}

impl DiagnosticProfileComparisonProblemJson {
    pub(super) fn from_problem(
        problem: &bray_diagnostics::DiagnosticProfileComparisonProblem,
    ) -> Self {
        match problem {
            bray_diagnostics::DiagnosticProfileComparisonProblem::Context { before, after } => {
                Self::Context {
                    before: DiagnosticProfileContextJson::from_context(before),
                    after: DiagnosticProfileContextJson::from_context(after),
                }
            }
            bray_diagnostics::DiagnosticProfileComparisonProblem::Descriptor { kind, id } => {
                Self::Descriptor {
                    kind: profile_descriptor_kind_key(*kind),
                    id: *id,
                }
            }
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticProfileValidationProblemJson {
    SchemaRevision { expected: u32, actual: u32 },
    DuplicateDescriptor { kind: &'static str, id: u16 },
    DuplicateObservation { kind: &'static str, id: u16 },
    UnknownDescriptor { kind: &'static str, id: u16 },
    InvalidRuntimeArtifactIdentity { index: usize },
    NonCanonicalRuntimeArtifacts { first: String, second: String },
    InvalidSchedulerStatistics,
    InvalidQueryStatistics { id: u16 },
}

impl DiagnosticProfileValidationProblemJson {
    pub(super) fn from_problem(
        problem: &bray_diagnostics::DiagnosticProfileValidationProblem,
    ) -> Self {
        use bray_diagnostics::DiagnosticProfileValidationProblem as Problem;

        match problem {
            Problem::SchemaRevision { expected, actual } => Self::SchemaRevision {
                expected: *expected,
                actual: *actual,
            },
            Problem::DuplicateDescriptor { kind, id } => Self::DuplicateDescriptor {
                kind: profile_descriptor_kind_key(*kind),
                id: *id,
            },
            Problem::DuplicateObservation { kind, id } => Self::DuplicateObservation {
                kind: profile_descriptor_kind_key(*kind),
                id: *id,
            },
            Problem::UnknownDescriptor { kind, id } => Self::UnknownDescriptor {
                kind: profile_descriptor_kind_key(*kind),
                id: *id,
            },
            Problem::InvalidRuntimeArtifactIdentity { index } => {
                Self::InvalidRuntimeArtifactIdentity { index: *index }
            }
            Problem::NonCanonicalRuntimeArtifacts { first, second } => {
                Self::NonCanonicalRuntimeArtifacts {
                    first: first.clone(),
                    second: second.clone(),
                }
            }
            Problem::InvalidSchedulerStatistics => Self::InvalidSchedulerStatistics,
            Problem::InvalidQueryStatistics { id } => Self::InvalidQueryStatistics { id: *id },
        }
    }
}

const fn profile_descriptor_kind_key(
    kind: bray_diagnostics::DiagnosticProfileDescriptorKind,
) -> &'static str {
    match kind {
        bray_diagnostics::DiagnosticProfileDescriptorKind::Operation => "operation",
        bray_diagnostics::DiagnosticProfileDescriptorKind::Query => "query",
        bray_diagnostics::DiagnosticProfileDescriptorKind::Metric => "metric",
    }
}

#[derive(Serialize)]
#[serde(tag = "category", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticProjectProcessFailureJson {
    Io { error: &'static str },
    InvalidSize,
    ThreadIdentityExhausted,
    EventGenerationExhausted,
    InvalidEventIdentity,
    RuntimeThreadAlreadyInitialized,
    SynchronizationPoisoned,
    Unsupported,
}

impl DiagnosticProjectProcessFailureJson {
    pub(super) fn from_failure(failure: bray_diagnostics::DiagnosticProjectProcessFailure) -> Self {
        use bray_diagnostics::DiagnosticProjectProcessFailure as Failure;

        match failure {
            Failure::Io(error) => Self::Io {
                error: error.as_str(),
            },
            Failure::InvalidSize => Self::InvalidSize,
            Failure::ThreadIdentityExhausted => Self::ThreadIdentityExhausted,
            Failure::EventGenerationExhausted => Self::EventGenerationExhausted,
            Failure::InvalidEventIdentity => Self::InvalidEventIdentity,
            Failure::RuntimeThreadAlreadyInitialized => Self::RuntimeThreadAlreadyInitialized,
            Failure::SynchronizationPoisoned => Self::SynchronizationPoisoned,
            Failure::Unsupported => Self::Unsupported,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticTestExecutionPlanProblemJson {
    InvocationCountMismatch {
        entries: u32,
        invocations: u32,
    },
    InvocationIdentityMismatch {
        test: String,
    },
    DuplicateIdentity {
        test: String,
    },
    CaptureBudgetExceeded {
        test: String,
        required_bytes: u64,
        maximum_bytes: u64,
    },
}

pub(super) fn test_execution_plan_failure_json(
    problem: &bray_diagnostics::DiagnosticTestExecutionPlanProblem,
) -> DiagnosticProjectCommandFailureJson {
    use bray_diagnostics::DiagnosticTestExecutionPlanProblem as Problem;

    let problem = match problem {
        Problem::InvocationCountMismatch {
            entries,
            invocations,
        } => DiagnosticTestExecutionPlanProblemJson::InvocationCountMismatch {
            entries: *entries,
            invocations: *invocations,
        },
        Problem::InvocationIdentityMismatch(test) => {
            DiagnosticTestExecutionPlanProblemJson::InvocationIdentityMismatch {
                test: test.clone(),
            }
        }
        Problem::DuplicateIdentity(test) => {
            DiagnosticTestExecutionPlanProblemJson::DuplicateIdentity { test: test.clone() }
        }
        Problem::CaptureBudgetExceeded {
            test,
            required_bytes,
            maximum_bytes,
        } => DiagnosticTestExecutionPlanProblemJson::CaptureBudgetExceeded {
            test: test.clone(),
            required_bytes: *required_bytes,
            maximum_bytes: *maximum_bytes,
        },
    };

    DiagnosticProjectCommandFailureJson::TestExecutionPlan { problem }
}

#[derive(Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticProjectSelectionJson {
    WorkerCountZero,
    ProfileRequiresCompilation,
    InvalidStandardLibraryRoot {
        path: String,
    },
    InvalidPackageIdentity {
        value: String,
    },
    InvalidSourcePackageIdentity {
        value: String,
    },
    InvalidPackageVersion {
        value: String,
    },
    InvalidProductIdentity {
        value: String,
    },
    DependencyArgumentCountMismatch {
        products: u32,
        interfaces: u32,
        implementations: u32,
    },
    InvalidDependencyProduct {
        value: String,
    },
    InvalidPlatformServiceBinding {
        value: String,
    },
    NoMatchingProduct {
        selection: String,
    },
    SingleProductRequired {
        actual: u32,
    },
    InvalidPackageSelector {
        value: String,
    },
    UnknownPackage {
        value: String,
    },
    UnknownTarget {
        value: String,
    },
    ProductTargetUnavailable {
        product: String,
        target: String,
    },
    InvalidInstallName {
        value: String,
    },
    MissingExecutable,
    MissingExecutableOutput,
    MissingTestExecutableOutput,
    MissingTestCatalogOutput,
    MissingTestHost,
    UnsupportedInspectionProduct {
        value: String,
    },
    TargetRequired,
    InvalidTestFilter {
        value: String,
    },
}

impl DiagnosticProjectSelectionJson {
    pub(in crate::output::diagnostic::json) fn from_problem(
        problem: &bray_diagnostics::DiagnosticProjectSelectionProblem,
    ) -> Self {
        use bray_diagnostics::DiagnosticProjectSelectionProblem as Problem;

        match problem {
            Problem::WorkerCountZero => Self::WorkerCountZero,
            Problem::ProfileRequiresCompilation => Self::ProfileRequiresCompilation,
            Problem::InvalidStandardLibraryRoot { path } => Self::InvalidStandardLibraryRoot {
                path: path_to_output_string(path),
            },
            Problem::InvalidPackageIdentity(value) => Self::InvalidPackageIdentity {
                value: value.clone(),
            },
            Problem::InvalidSourcePackageIdentity(value) => Self::InvalidSourcePackageIdentity {
                value: value.clone(),
            },
            Problem::InvalidPackageVersion(value) => Self::InvalidPackageVersion {
                value: value.clone(),
            },
            Problem::InvalidProductIdentity(value) => Self::InvalidProductIdentity {
                value: value.clone(),
            },
            Problem::DependencyArgumentCountMismatch {
                products,
                interfaces,
                implementations,
            } => Self::DependencyArgumentCountMismatch {
                products: *products,
                interfaces: *interfaces,
                implementations: *implementations,
            },
            Problem::InvalidDependencyProduct(value) => Self::InvalidDependencyProduct {
                value: value.clone(),
            },
            Problem::InvalidPlatformServiceBinding(value) => Self::InvalidPlatformServiceBinding {
                value: value.clone(),
            },
            Problem::NoMatchingProduct(selection) => Self::NoMatchingProduct {
                selection: selection.clone(),
            },
            Problem::SingleProductRequired { actual } => {
                Self::SingleProductRequired { actual: *actual }
            }
            Problem::InvalidPackageSelector(value) => Self::InvalidPackageSelector {
                value: value.clone(),
            },
            Problem::UnknownPackage(value) => Self::UnknownPackage {
                value: value.clone(),
            },
            Problem::UnknownTarget(value) => Self::UnknownTarget {
                value: value.clone(),
            },
            Problem::ProductTargetUnavailable { product, target } => {
                Self::ProductTargetUnavailable {
                    product: product.clone(),
                    target: target.clone(),
                }
            }
            Problem::InvalidInstallName(value) => Self::InvalidInstallName {
                value: value.clone(),
            },
            Problem::MissingExecutable => Self::MissingExecutable,
            Problem::MissingExecutableOutput => Self::MissingExecutableOutput,
            Problem::MissingTestExecutableOutput => Self::MissingTestExecutableOutput,
            Problem::MissingTestCatalogOutput => Self::MissingTestCatalogOutput,
            Problem::MissingTestHost => Self::MissingTestHost,
            Problem::UnsupportedInspectionProduct(value) => Self::UnsupportedInspectionProduct {
                value: value.clone(),
            },
            Problem::TargetRequired => Self::TargetRequired,
            Problem::InvalidTestFilter(value) => Self::InvalidTestFilter {
                value: value.clone(),
            },
        }
    }
}
