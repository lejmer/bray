mod abi;
mod compatibility;
mod layout;
mod model;
mod symbol;

pub use abi::{CallableAbiMapping, TargetAbi, TargetAbiBuildError, TargetCallingConvention};
pub use compatibility::TargetCompatibility;
pub use layout::{
    TargetAddressSpace, TargetAddressSpaceKind, TargetDataLayout, TargetDataLayoutBuildError,
    TargetScalarKind, TargetScalarLayout, TargetScalarLayoutBuildError,
};
pub use model::{CodegenTarget, CodegenTargetBuildError, TargetContract, TargetMachineSelection};
pub use symbol::{CodegenLinkage, TargetSymbolConvention, TargetSymbolConventionBuildError};
