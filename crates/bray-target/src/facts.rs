//! Language-defined facts supplied by one target profile.

mod abi;
mod kind;
mod model;
mod scalar;
mod value;

pub use abi::{TargetAbiFacts, TargetAbiScalarFacts, TargetForeignAbiFacts};
pub use kind::TargetFactKind;
pub use model::{
    TargetAddressSpaceFacts, TargetAlignmentFacts, TargetAtomicFacts, TargetFacts,
    TargetIdentityFacts, TargetOperationFacts,
};
pub use scalar::{TargetScalarFacts, TargetScalarKind};
pub use value::TargetFactValue;
