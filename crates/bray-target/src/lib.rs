//! Stable target identities and machine-model contracts shared across compiler phases.

#![forbid(unsafe_code)]

mod control;
mod identity;
mod layout;
mod machine;
mod native;
mod output;
mod profile;
mod properties;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use control::{InlineAssemblyOptions, TargetControlSupport};
pub use identity::TargetIdentity;
pub use layout::{TargetLayoutContract, TargetValueLayout};
pub use machine::{
    CodeModel, Endianness, ObjectFormat, RelocationModel, TargetArchitecture,
    TargetMachineProperties,
};
pub use native::NativeTarget;
pub use output::{
    TargetOutputDescription, TargetOutputDescriptionBuildError, TargetOutputKind, TargetOutputName,
    TargetOutputNameBuildError,
};
pub use profile::{TargetProfile, TargetProfileBuildError};
pub use properties::{
    TargetAbiScalars, TargetAbiSupport, TargetAddressSpaces, TargetAlignmentLimits,
    TargetAtomicOperations, TargetAtomicRepresentation, TargetAtomicRepresentationSupport,
    TargetAtomicSupport, TargetCDataModel, TargetCScalarKind, TargetForeignAbiContract,
    TargetOperationSupport, TargetPlatformIdentity, TargetProperties, TargetPropertyKind,
    TargetPropertyValue, TargetScalarKind, TargetScalarSupport,
};
