//! Stable target identities and machine-model contracts shared across compiler phases.

#![forbid(unsafe_code)]

mod facts;
mod identity;
mod machine;
mod output;
mod profile;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use facts::{
    TargetAbiFacts, TargetAddressSpaceFacts, TargetAlignmentFacts, TargetAtomicFacts,
    TargetFactKind, TargetFactValue, TargetFacts, TargetIdentityFacts, TargetScalarFacts,
};
pub use identity::TargetIdentity;
pub use machine::{
    CodeModel, Endianness, ObjectFormat, RelocationModel, TargetArchitecture,
    TargetMachineProperties,
};
pub use output::{
    TargetOutputDescription, TargetOutputDescriptionBuildError, TargetOutputKind, TargetOutputName,
    TargetOutputNameBuildError,
};
pub use profile::{TargetProfile, TargetProfileBuildError};
