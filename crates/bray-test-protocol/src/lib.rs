//! Stable metadata and command protocol shared by Bray test producers and consumers.

#![forbid(unsafe_code)]

mod capture;
mod catalog;
mod event;
mod execution;
mod failure;
mod filter;
mod identity;

pub use capture::{
    CapturedStream, CapturedStreamPolicy, TestCaptureLimits, TestCapturePolicy, TestStreamFailure,
    TestStreamFailureKind, TestStreamKind,
};
pub use catalog::{TestCatalog, TestCatalogBuildError, TestEntryMetadata};
pub use event::{TestHostEvent, TestHostEventKind, TestHostEventSequence};
pub use execution::{
    TestCancellationSource, TestDuration, TestErrorTypeIdentity, TestInfrastructureFailure,
    TestInfrastructureFailureKind, TestInvocationPlan, TestInvocationResult, TestOutcome,
    TestPanicCause, TestPanicReport, TestTimeoutPolicy,
};
pub use failure::{AssertionFailure, ExplicitTestFailure};
pub use filter::{TestFilter, TestSelection, TestSelectionQuery, TestShard};
pub use identity::{TestDeclarationPath, TestIdentity, TestSourceAnchor};
