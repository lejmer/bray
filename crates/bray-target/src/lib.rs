//! Stable target identities and machine-model contracts shared across compiler phases.

#![forbid(unsafe_code)]

mod control;
mod properties;
mod identity;
mod layout;
mod machine;
mod native;
mod output;
mod profile;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use properties::{
    TargetAbiSupport, TargetAbiScalars, TargetAddressSpaces, TargetAlignmentLimits,
    TargetAtomicSupport, TargetAtomicOperations, TargetAtomicRepresentation,
    TargetAtomicRepresentationSupport, TargetCDataModel, TargetCScalarKind, TargetPropertyKind,
    TargetPropertyValue, TargetProperties, TargetForeignAbiContract, TargetPlatformIdentity,
    TargetOperationSupport, TargetScalarSupport, TargetScalarKind,
};
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
