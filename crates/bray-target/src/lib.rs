//! Stable target identities and machine-model contracts shared across compiler phases.

#![forbid(unsafe_code)]

mod facts;
mod identity;
mod layout;
mod machine;
mod output;
mod profile;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use facts::{
    TargetAbiFacts, TargetAbiScalarFacts, TargetAddressSpaceFacts, TargetAlignmentFacts,
    TargetAtomicFacts, TargetFactKind, TargetFactValue, TargetFacts, TargetForeignAbiFacts,
    TargetIdentityFacts, TargetOperationFacts, TargetScalarFacts, TargetScalarKind,
};
pub use identity::TargetIdentity;
pub use layout::{TargetLayoutContract, TargetValueLayout};
pub use machine::{
    CodeModel, Endianness, ObjectFormat, RelocationModel, TargetArchitecture,
    TargetMachineProperties,
};
pub use output::{
    TargetOutputDescription, TargetOutputDescriptionBuildError, TargetOutputKind, TargetOutputName,
    TargetOutputNameBuildError,
};
pub use profile::{TargetProfile, TargetProfileBuildError};
