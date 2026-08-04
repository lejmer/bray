//! Stable metadata and command protocol shared by Bray test producers and consumers.

#![forbid(unsafe_code)]

mod capture;
mod catalog;
mod codec;
mod event;
mod execution;
mod failure;
mod filter;
mod identity;
mod report;
mod scheduling;
#[cfg(test)]
mod test_support;

pub use capture::{
    CapturedStream, CapturedStreamPolicy, TestCaptureLimits, TestCapturePolicy, TestStreamFailure,
    TestStreamFailureKind, TestStreamKind,
};
pub use catalog::{TestCatalog, TestCatalogBuildError, TestEntryMetadata};
pub use codec::{
    TestCatalogDigest, TestProtocolError, decode_test_catalog, encode_test_catalog,
    read_host_command, read_host_control, read_host_result, write_host_command, write_host_control,
    write_host_result,
};
pub use event::{TestHostEvent, TestHostEventKind, TestHostEventSequence};
pub use execution::{
    TestCancellationSource, TestCatalogEntryId, TestDuration, TestErrorTypeIdentity,
    TestHostCommand, TestHostCommandId, TestHostControl, TestHostResult, TestInfrastructureFailure,
    TestInfrastructureFailureKind, TestInvocationPlan, TestInvocationResult, TestOutcome,
    TestPanicCause, TestPanicReport, TestTimeoutPolicy,
};
pub use failure::{AssertionFailure, ExplicitTestFailure};
pub use filter::{TestFilter, TestSelection, TestSelectionQuery, TestShard};
pub use identity::{TestDeclarationPath, TestIdentity, TestSourceAnchor};
pub use report::{TestCommandReport, TestOutcomeCounts, TestProductReport, TestSelectionSummary};
pub use scheduling::{
    TestAdmission, TestAdmissionSchedule, TestExecutionMode, TestExecutionPlan,
    TestExecutionPlanBuildError, TestSchedulingError, TestStopReason,
};
