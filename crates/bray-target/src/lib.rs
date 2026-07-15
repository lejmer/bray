//! Stable target identities and machine-model contracts shared across compiler phases.

#![forbid(unsafe_code)]

mod identity;
mod machine;

pub use identity::TargetIdentity;
pub use machine::{
    CodeModel, Endianness, ObjectFormat, RelocationModel, TargetArchitecture,
    TargetMachineProperties,
};
