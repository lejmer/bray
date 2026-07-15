//! Stable target identities and machine-model contracts shared across compiler phases.

#![forbid(unsafe_code)]

mod identity;
mod machine;
mod output;

pub use identity::TargetIdentity;
pub use machine::{
    CodeModel, Endianness, ObjectFormat, RelocationModel, TargetArchitecture,
    TargetMachineProperties,
};
pub use output::{
    TargetOutputDescription, TargetOutputDescriptionBuildError, TargetOutputKind, TargetOutputName,
    TargetOutputNameBuildError,
};
