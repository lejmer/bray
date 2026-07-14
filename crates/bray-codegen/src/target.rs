mod abi;
mod compatibility;
mod identity;
mod layout;
mod machine;
mod model;
mod symbol;

pub use abi::{CallableAbiMapping, TargetAbi, TargetAbiBuildError, TargetCallingConvention};
pub use compatibility::TargetCompatibility;
pub use identity::TargetIdentity;
pub use layout::{
    TargetAddressSpace, TargetAddressSpaceKind, TargetDataLayout, TargetDataLayoutBuildError,
    TargetScalarKind, TargetScalarLayout, TargetScalarLayoutBuildError,
};
pub use machine::{
    CodeModel, Endianness, ObjectFormat, RelocationModel, TargetArchitecture,
    TargetMachineProperties,
};
pub use model::{CodegenTarget, CodegenTargetBuildError, TargetContract, TargetMachineSelection};
pub use symbol::{CodegenLinkage, TargetSymbolConvention, TargetSymbolConventionBuildError};
